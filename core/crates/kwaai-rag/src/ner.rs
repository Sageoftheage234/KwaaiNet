//! Lightweight proper-noun pre-screening and pronoun resolution for entity extraction.
//!
//! Feeds `graph::extract_from_text()` with:
//! - `candidates`: proper noun phrases for the LLM to classify (guided attention)
//! - `pronoun_map`: resolved (pronoun, entity_name) pairs so the LLM never
//!   receives raw pronouns as entity candidates
//!
//! Both functions are pure Rust with no external NLP dependencies.

/// Maximum candidates forwarded to the LLM (prevents prompt blowout on dense passages).
const CANDIDATE_CAP: usize = 40;

/// Uppercase-starting words that are not proper nouns.
const STOP_WORDS: &[&str] = &[
    // Articles & determiners
    "The",
    "A",
    "An",
    "This",
    "That",
    "These",
    "Those",
    "Its",
    "It",
    // Personal pronouns
    "He",
    "Him",
    "His",
    "She",
    "Her",
    "Hers",
    "They",
    "Them",
    "Their",
    "Theirs",
    "We",
    "Our",
    "You",
    "Your",
    "I",
    "My",
    "Me",
    // Conjunctions & prepositions
    "In",
    "On",
    "At",
    "By",
    "For",
    "From",
    "With",
    "Of",
    "About",
    "And",
    "Or",
    "But",
    "If",
    "As",
    "So",
    "Yet",
    "Nor",
    "Both",
    "Also",
    "To",
    "Into",
    "Up",
    "Between",
    "Among",
    "Before",
    "After",
    "During",
    "Through",
    "Within",
    "Over",
    // Days of the week
    "Monday",
    "Tuesday",
    "Wednesday",
    "Thursday",
    "Friday",
    "Saturday",
    "Sunday",
    // Months
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];

const MALE_PRONOUNS: &[&str] = &["he", "him", "his", "himself"];
const FEMALE_PRONOUNS: &[&str] = &["she", "her", "hers", "herself"];
const NEUTRAL_PRONOUNS: &[&str] = &["they", "them", "their", "theirs", "themselves"];

fn core(word: &str) -> &str {
    word.trim_matches(|c: char| !c.is_alphanumeric())
}

/// Returns true when `word` ends a phrase — either a full sentence (`.!?`) or a
/// list/clause separator (`,;:)"'`) that makes consecutive capitalized words
/// belong to different named entities rather than one multi-word phrase.
fn ends_phrase(word: &str) -> bool {
    matches!(
        word.chars().last(),
        Some('.' | '!' | '?' | ',' | ';' | ':' | ')' | '"' | '\'')
    )
}

fn is_stop_word(s: &str) -> bool {
    STOP_WORDS.contains(&s)
}

/// Extract proper noun candidate phrases from `text`.
///
/// Scans all capitalised word sequences (not just mid-sentence ones), merges
/// consecutive capitalised words into multi-word phrases, stops phrase
/// accumulation at sentence boundaries, and deduplicates results.
///
/// The caller passes the returned list to the LLM as a focus list; the LLM
/// classifies candidates as entities or discards non-entities.
pub fn extract_proper_noun_candidates(text: &str) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    let mut result = Vec::new();

    let words: Vec<&str> = text.split_whitespace().collect();
    let n = words.len();
    let mut i = 0;

    while i < n && result.len() < CANDIDATE_CAP {
        let w = words[i];
        let c = core(w);

        let is_candidate =
            c.len() > 1 && c.starts_with(|ch: char| ch.is_uppercase()) && !is_stop_word(c);

        if is_candidate {
            let mut parts = vec![c.to_string()];
            let mut j = i + 1;

            // Extend phrase across consecutive capitalised words, stopping at
            // any phrase boundary (sentence-end or list separator) or at 5 words
            // (entity names in this corpus never exceed 5 words).
            while j < n && !ends_phrase(words[j - 1]) && parts.len() < 5 {
                let nc = core(words[j]);
                if nc.len() > 1 && nc.starts_with(|ch: char| ch.is_uppercase()) && !is_stop_word(nc)
                {
                    parts.push(nc.to_string());
                    j += 1;
                } else {
                    break;
                }
            }

            let phrase = parts.join(" ");
            if seen.insert(phrase.clone()) {
                result.push(phrase);
            }
            i = j;
        } else {
            i += 1;
        }
    }

    result
}

/// Resolve pronouns in `text` to entity names.
///
/// `entities` is a snapshot of `(name, gender)` for Person entities already in
/// the graph, pre-computed before any async work starts. Gender is `"Male"`,
/// `"Female"`, or `None` (ambiguous).
///
/// Resolution strategy per pronoun:
/// 1. Scan `entities` from end to start (most recently known = most likely referent).
/// 2. If no gender match found, scan forward in the remaining text for the next
///    capitalised proper-noun sequence.
///
/// Returns one `(pronoun, entity_name)` pair per unique pronoun type found.
/// Unresolved pronouns are omitted rather than guessed.
pub fn resolve_pronouns(
    text: &str,
    entities: &[(String, Option<String>)],
) -> Vec<(String, String)> {
    let words: Vec<&str> = text.split_whitespace().collect();
    let mut resolved: Vec<(String, String)> = Vec::new();
    let mut seen_pronouns = std::collections::HashSet::new();

    for (idx, &w) in words.iter().enumerate() {
        let lower = core(w).to_lowercase();

        let gender_wanted = if MALE_PRONOUNS.contains(&lower.as_str()) {
            Some("Male")
        } else if FEMALE_PRONOUNS.contains(&lower.as_str()) {
            Some("Female")
        } else if NEUTRAL_PRONOUNS.contains(&lower.as_str()) {
            Some("Neutral")
        } else {
            None
        };

        let Some(gender) = gender_wanted else {
            continue;
        };

        if !seen_pronouns.insert(lower.clone()) {
            continue;
        }

        // Strategy 1: most-recent entity with matching gender.
        let found = entities.iter().rev().find(|(_, g)| match g.as_deref() {
            Some(eg) => eg == gender || gender == "Neutral",
            None => gender == "Neutral",
        });

        let name = found
            .map(|(n, _)| n.clone())
            .or_else(|| forward_name(&words[idx + 1..]));

        if let Some(name) = name {
            resolved.push((lower, name));
        }
    }

    resolved
}

/// Scan forward through `words` past lowercase words to find the first capitalised
/// proper-noun sequence (stops extending the phrase on the first non-candidate word
/// after the sequence has started).
fn forward_name(words: &[&str]) -> Option<String> {
    let mut parts: Vec<String> = Vec::new();
    let mut in_phrase = false;

    for &w in words.iter().take(40) {
        let c = core(w);
        let is_candidate =
            c.len() > 1 && c.starts_with(|ch: char| ch.is_uppercase()) && !is_stop_word(c);

        if is_candidate {
            parts.push(c.to_string());
            in_phrase = true;
        } else if in_phrase {
            break; // phrase ended
        }
        // else: keep scanning past lowercase / stop words
    }

    if parts.is_empty() {
        None
    } else {
        Some(parts.join(" "))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ner_proper_nouns_basic() {
        let text = "We lived in District Six. My father Abdullah Gool worked there.";
        let c = extract_proper_noun_candidates(text);
        assert!(c.contains(&"District Six".to_string()), "{c:?}");
        assert!(c.contains(&"Abdullah Gool".to_string()), "{c:?}");
    }

    #[test]
    fn ner_proper_nouns_stops_at_sentence_boundary() {
        let text = "We visited Cape Town. London is far away.";
        let c = extract_proper_noun_candidates(text);
        assert!(c.contains(&"Cape Town".to_string()), "{c:?}");
        assert!(c.contains(&"London".to_string()), "{c:?}");
        assert!(!c.contains(&"Cape Town London".to_string()), "{c:?}");
    }

    #[test]
    fn ner_proper_nouns_filters_stop_words() {
        let text = "The doctor said He was fine. She visited The Hospital.";
        let c = extract_proper_noun_candidates(text);
        assert!(!c.contains(&"The".to_string()), "{c:?}");
        assert!(!c.contains(&"He".to_string()), "{c:?}");
        assert!(c.contains(&"Hospital".to_string()), "{c:?}");
    }

    #[test]
    fn ner_resolve_pronouns_male() {
        let entities = vec![("Abdullah Gool".to_string(), Some("Male".to_string()))];
        let map = resolve_pronouns("Abdullah arrived. He was tired.", &entities);
        assert!(
            map.iter().any(|(p, n)| p == "he" && n == "Abdullah Gool"),
            "{map:?}"
        );
    }

    #[test]
    fn ner_resolve_pronouns_female() {
        let entities = vec![
            ("Hassan".to_string(), Some("Male".to_string())),
            ("Zainab".to_string(), Some("Female".to_string())),
        ];
        let map = resolve_pronouns("Zainab left. She took the train.", &entities);
        assert!(
            map.iter().any(|(p, n)| p == "she" && n == "Zainab"),
            "{map:?}"
        );
    }

    #[test]
    fn ner_phrase_boundary_comma_list() {
        // Comma-separated names must not merge into one giant phrase.
        let text =
            "The meeting was attended by Soviet Ambassador, Cissie Gool, Moses Kotane, Bill Andrews.";
        let c = extract_proper_noun_candidates(text);
        // Each name should be its own candidate
        assert!(c.contains(&"Soviet Ambassador".to_string()), "{c:?}");
        assert!(c.contains(&"Cissie Gool".to_string()), "{c:?}");
        assert!(c.contains(&"Moses Kotane".to_string()), "{c:?}");
        assert!(c.contains(&"Bill Andrews".to_string()), "{c:?}");
        // The entire list must NOT be one merged candidate
        assert!(
            !c.iter()
                .any(|s| s.contains("Soviet") && s.contains("Kotane")),
            "merged phrase found: {c:?}"
        );
    }

    #[test]
    fn ner_phrase_boundary_five_word_cap() {
        // A single run of six consecutive capitalized words must be capped at 5.
        let text = "He studied at Royal Cape Town University Medical School annually.";
        let c = extract_proper_noun_candidates(text);
        assert!(c.iter().all(|p| p.split_whitespace().count() <= 5), "{c:?}");
    }

    #[test]
    fn ner_resolve_pronouns_forward_scan() {
        let entities: Vec<(String, Option<String>)> = vec![];
        let map = resolve_pronouns(
            "He was a great leader. Nelson Mandela inspired millions.",
            &entities,
        );
        assert!(
            map.iter().any(|(p, n)| p == "he" && n == "Nelson Mandela"),
            "{map:?}"
        );
    }
}
