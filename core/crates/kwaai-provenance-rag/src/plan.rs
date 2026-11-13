//! Turn two fact snapshots into lineage steps. Pure: no graph, no I/O.
//!
//! Given the facts before and after a stage:
//!
//! - a fact that disappeared and reappears under the canonical entity of a merge
//!   is a **rewrite** of the old fact into the new one;
//! - a field fact whose value changed on the same entity and field is a
//!   **rewrite**;
//! - any other fact that disappeared is a **removal**;
//! - any other new fact is an **extraction** from its evidence chunks;
//! - a fact that gained evidence chunks gets an **evidence-added** extraction from
//!   the new chunks only.
//!
//! Evidence that a fact loses is not recorded: lineage is history, and the fact
//! was still derived from that chunk.

use crate::facts::Snapshot;
use std::collections::{BTreeMap, BTreeSet};

/// One lineage step, in claim IDs and chunk IDs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Step {
    /// A new fact, derived from these evidence chunks (may be empty).
    Extract {
        /// New claim.
        to: String,
        /// Its evidence chunks.
        chunks: Vec<i64>,
    },
    /// An existing (or rewritten) fact gained these evidence chunks.
    EvidenceAdded {
        /// Claim that gained evidence.
        to: String,
        /// The chunks it gained.
        chunks: Vec<i64>,
    },
    /// One fact became another (merge or field-value change).
    Rewrite {
        /// Claim that disappeared.
        from: String,
        /// Claim that replaced it.
        to: String,
    },
    /// A fact disappeared with no replacement.
    Remove {
        /// Claim that disappeared.
        claim: String,
    },
}

/// Resolve `(alias, canonical)` merge pairs into a function from any entity ID to
/// the ID it ended up under. Chains (`a → b`, `b → c`) are followed; a loop is
/// cut after as many hops as there are pairs.
pub fn merge_resolver(pairs: &[(i64, i64)]) -> impl Fn(i64) -> i64 + '_ {
    let map: BTreeMap<i64, i64> = pairs.iter().copied().filter(|(a, c)| a != c).collect();
    move |id| {
        let mut cur = id;
        for _ in 0..=map.len() {
            match map.get(&cur) {
                Some(&next) => cur = next,
                None => break,
            }
        }
        cur
    }
}

/// Lineage steps that explain how `prev` became `next`. `merges` lists the merge
/// pairs of this stage (empty for stages that do not merge). Output order is
/// deterministic.
pub fn plan(prev: &Snapshot, next: &Snapshot, merges: &[(i64, i64)]) -> Vec<Step> {
    let resolve = merge_resolver(merges);
    let removed: Vec<&String> = prev.keys().filter(|k| !next.contains_key(*k)).collect();
    let added: BTreeSet<&String> = next.keys().filter(|k| !prev.contains_key(*k)).collect();
    let mut explained: BTreeSet<&String> = BTreeSet::new();
    // For each rewrite target, the evidence its sources already had.
    let mut inherited: BTreeMap<&String, BTreeSet<i64>> = BTreeMap::new();
    let mut steps = Vec::new();

    for from in removed {
        let old = &prev[from];
        let moved = old.key.map_entities(&resolve);
        let moved_id = moved.claim().id;
        let target: Option<&String> = if &moved_id != from {
            next.get_key_value(&moved_id).map(|(k, _)| k)
        } else {
            None
        };
        let target = target.or_else(|| {
            let (entity, field) = moved.field_slot()?;
            added
                .iter()
                .copied()
                .filter(|a| !explained.contains(a))
                .find(|a| next[*a].key.field_slot() == Some((entity, field)))
        });
        match target {
            Some(to) => {
                if added.contains(to) {
                    explained.insert(to);
                }
                inherited
                    .entry(to)
                    .or_default()
                    .extend(old.evidence.iter().copied());
                steps.push(Step::Rewrite {
                    from: from.clone(),
                    to: to.clone(),
                });
            }
            None => steps.push(Step::Remove {
                claim: from.clone(),
            }),
        }
    }

    for to in &added {
        if !explained.contains(to) {
            steps.push(Step::Extract {
                to: (*to).clone(),
                chunks: next[*to].evidence.iter().copied().collect(),
            });
        }
    }

    // Evidence growth: on facts present in both snapshots, and on rewrite targets
    // beyond what their sources carried.
    for (id, fact) in next {
        let before: Option<BTreeSet<i64>> = match prev.get(id) {
            Some(p) => {
                let mut b = p.evidence.clone();
                if let Some(extra) = inherited.get(id) {
                    b.extend(extra.iter().copied());
                }
                Some(b)
            }
            None => inherited.get(id).cloned(),
        };
        if let Some(before) = before {
            let gained: Vec<i64> = fact.evidence.difference(&before).copied().collect();
            if !gained.is_empty() {
                steps.push(Step::EvidenceAdded {
                    to: id.clone(),
                    chunks: gained,
                });
            }
        }
    }
    steps
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::facts::{Fact, FactKey};

    fn rel(src: i64, r: &str, dst: i64, ev: &[i64]) -> Fact {
        Fact::new(
            FactKey::Relation {
                src,
                relation: r.into(),
                dst,
            },
            ev.iter().copied(),
        )
    }

    fn field(e: i64, f: &str, v: &str, ev: &[i64]) -> Fact {
        Fact::new(
            FactKey::Field {
                entity: e,
                field: f.into(),
                value: v.into(),
            },
            ev.iter().copied(),
        )
    }

    fn snap(facts: &[Fact]) -> Snapshot {
        facts
            .iter()
            .map(|f| (f.claim.id.clone(), f.clone()))
            .collect()
    }

    fn id(f: &Fact) -> String {
        f.claim.id.clone()
    }

    #[test]
    fn unchanged_graph_plans_nothing() {
        let s = snap(&[rel(1, "knows", 2, &[10]), field(1, "born", "1901", &[11])]);
        assert!(plan(&s, &s, &[]).is_empty());
    }

    #[test]
    fn new_fact_is_extracted_from_its_chunks() {
        let a = rel(1, "knows", 2, &[10, 12]);
        let steps = plan(&snap(&[]), &snap(std::slice::from_ref(&a)), &[]);
        assert_eq!(
            steps,
            vec![Step::Extract {
                to: id(&a),
                chunks: vec![10, 12]
            }]
        );
    }

    #[test]
    fn vanished_fact_is_removed() {
        let a = rel(1, "knows", 2, &[10]);
        assert_eq!(
            plan(&snap(std::slice::from_ref(&a)), &snap(&[]), &[]),
            vec![Step::Remove { claim: id(&a) }]
        );
    }

    #[test]
    fn merge_rewrites_alias_facts_to_canonical() {
        // alias 5 merged into canonical 7
        let old = rel(5, "knows", 9, &[10]);
        let new = rel(7, "knows", 9, &[10]);
        let steps = plan(
            &snap(std::slice::from_ref(&old)),
            &snap(std::slice::from_ref(&new)),
            &[(5, 7)],
        );
        assert_eq!(
            steps,
            vec![Step::Rewrite {
                from: id(&old),
                to: id(&new)
            }]
        );
    }

    #[test]
    fn merge_into_an_existing_fact_is_a_rewrite_plus_new_evidence() {
        let alias_fact = rel(5, "knows", 9, &[10]);
        let canon_before = rel(7, "knows", 9, &[20]);
        let canon_after = rel(7, "knows", 9, &[10, 20]);
        let steps = plan(
            &snap(&[alias_fact.clone(), canon_before]),
            &snap(std::slice::from_ref(&canon_after)),
            &[(5, 7)],
        );
        // chunk 10 came with the alias fact, so it is not new evidence
        assert_eq!(
            steps,
            vec![Step::Rewrite {
                from: id(&alias_fact),
                to: id(&canon_after)
            }]
        );
    }

    #[test]
    fn merge_chains_are_followed() {
        let old = rel(1, "knows", 9, &[10]);
        let new = rel(3, "knows", 9, &[10]);
        let steps = plan(
            &snap(std::slice::from_ref(&old)),
            &snap(std::slice::from_ref(&new)),
            &[(1, 2), (2, 3)],
        );
        assert_eq!(
            steps,
            vec![Step::Rewrite {
                from: id(&old),
                to: id(&new)
            }]
        );
        let looped = merge_resolver(&[(1, 2), (2, 1)]);
        let _ = looped(1); // terminates
    }

    #[test]
    fn merge_with_no_surviving_fact_is_a_removal() {
        let old = rel(5, "spouse_of", 9, &[10]);
        let steps = plan(&snap(std::slice::from_ref(&old)), &snap(&[]), &[(5, 7)]);
        assert_eq!(steps, vec![Step::Remove { claim: id(&old) }]);
    }

    #[test]
    fn field_value_change_is_a_rewrite() {
        let old = field(1, "born", "1901", &[10]);
        let new = field(1, "born", "1902", &[10, 11]);
        let steps = plan(
            &snap(std::slice::from_ref(&old)),
            &snap(std::slice::from_ref(&new)),
            &[],
        );
        assert_eq!(
            steps,
            vec![
                Step::Rewrite {
                    from: id(&old),
                    to: id(&new)
                },
                Step::EvidenceAdded {
                    to: id(&new),
                    chunks: vec![11]
                },
            ]
        );
    }

    #[test]
    fn evidence_growth_on_an_existing_fact() {
        let before = rel(1, "knows", 2, &[10]);
        let after = rel(1, "knows", 2, &[10, 0, 12]);
        let steps = plan(&snap(&[before]), &snap(std::slice::from_ref(&after)), &[]);
        assert_eq!(
            steps,
            vec![Step::EvidenceAdded {
                to: id(&after),
                chunks: vec![0, 12]
            }]
        );
    }

    #[test]
    fn evidence_loss_is_not_recorded() {
        let before = rel(1, "knows", 2, &[10, 12]);
        let after = rel(1, "knows", 2, &[10]);
        assert!(plan(&snap(&[before]), &snap(&[after]), &[]).is_empty());
    }

    #[test]
    fn plan_is_deterministic() {
        let prev = snap(&[
            rel(1, "a", 2, &[1]),
            rel(3, "b", 4, &[2]),
            field(5, "x", "1", &[3]),
        ]);
        let next = snap(&[
            rel(1, "a", 2, &[1, 4]),
            rel(6, "c", 7, &[5]),
            field(5, "x", "2", &[3]),
        ]);
        assert_eq!(plan(&prev, &next, &[]), plan(&prev, &next, &[]));
    }
}
