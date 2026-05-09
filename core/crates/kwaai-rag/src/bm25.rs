//! In-memory BM25 index for keyword-based retrieval.
//!
//! Used alongside dense vector search (RRF hybrid) to handle acronym and
//! exact-name queries that semantic embeddings handle poorly.
//!
//! k1=1.5, b=0.75 per Robertson & Zaragoza (2009).

use std::collections::HashMap;

const K1: f64 = 1.5;
const B: f64 = 0.75;

pub struct BM25Index {
    /// chunk_id → list of (term, tf) for that chunk
    doc_terms: HashMap<i64, HashMap<String, f64>>,
    /// term → document frequency (# chunks containing the term)
    df: HashMap<String, usize>,
    /// total number of chunks
    n: usize,
    /// average chunk length in tokens
    avgdl: f64,
}

impl BM25Index {
    /// Build index from (chunk_id, doc_name, text) triples.
    ///
    /// Doc name tokens are injected at 3× weight so that title-word queries
    /// (e.g. "TLSA Teacher League") discriminate on filename even when the
    /// acronym appears in many chapters.
    pub fn build(chunks: &[(i64, &str, &str)]) -> Self {
        const DOC_NAME_WEIGHT: f64 = 3.0;
        let n = chunks.len();
        let mut doc_terms: HashMap<i64, HashMap<String, f64>> = HashMap::new();
        let mut df: HashMap<String, usize> = HashMap::new();
        let mut total_len = 0usize;

        for &(chunk_id, doc_name, text) in chunks {
            let text_terms = tokenize(text);
            // Sanitise doc name: strip extension, underscores → spaces.
            let name_clean = doc_name
                .trim_end_matches(".docx")
                .trim_end_matches(".pdf")
                .replace(['_', '-'], " ");
            let name_terms = tokenize(&name_clean);

            total_len += text_terms.len() + name_terms.len();
            let mut tf: HashMap<String, f64> = HashMap::new();
            for term in &text_terms {
                *tf.entry(term.clone()).or_default() += 1.0;
            }
            for term in &name_terms {
                *tf.entry(term.clone()).or_default() += DOC_NAME_WEIGHT;
            }
            for term in tf.keys() {
                *df.entry(term.clone()).or_default() += 1;
            }
            doc_terms.insert(chunk_id, tf);
        }

        let avgdl = if n > 0 {
            total_len as f64 / n as f64
        } else {
            1.0
        };
        Self {
            doc_terms,
            df,
            n,
            avgdl,
        }
    }

    /// Score all chunks for `query`. Returns sorted (chunk_id, score) descending.
    pub fn search(&self, query: &str, top_k: usize) -> Vec<(i64, f64)> {
        if self.n == 0 || top_k == 0 {
            return vec![];
        }
        let query_terms = tokenize(query);
        if query_terms.is_empty() {
            return vec![];
        }

        let mut scores: HashMap<i64, f64> = HashMap::new();
        for term in &query_terms {
            let df = *self.df.get(term).unwrap_or(&0);
            if df == 0 {
                continue;
            }
            let idf = ((self.n as f64 - df as f64 + 0.5) / (df as f64 + 0.5) + 1.0).ln();
            for (&chunk_id, tf_map) in &self.doc_terms {
                let tf = *tf_map.get(term).unwrap_or(&0.0);
                if tf == 0.0 {
                    continue;
                }
                let dl = tf_map.values().sum::<f64>();
                let bm25 = idf * (tf * (K1 + 1.0)) / (tf + K1 * (1.0 - B + B * dl / self.avgdl));
                *scores.entry(chunk_id).or_default() += bm25;
            }
        }

        let mut ranked: Vec<(i64, f64)> = scores.into_iter().collect();
        ranked.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        ranked.truncate(top_k);
        ranked
    }
}

/// Minimal tokenizer: lowercase, split on non-alphanumeric, filter stop words and short tokens.
fn tokenize(text: &str) -> Vec<String> {
    text.to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|t| t.len() >= 2 && !STOP_WORDS.contains(t))
        .map(|t| t.to_string())
        .collect()
}

const STOP_WORDS: &[&str] = &[
    "a", "an", "the", "and", "or", "but", "not", "no", "nor", "is", "are", "was", "were", "be",
    "been", "being", "been", "have", "has", "had", "do", "does", "did", "will", "would", "could",
    "should", "may", "might", "can", "shall", "must", "ought", "what", "which", "who", "whom",
    "whose", "how", "when", "where", "why", "that", "this", "these", "those", "it", "its",
    "itself", "he", "she", "they", "we", "you", "i", "me", "him", "her", "us", "them", "his",
    "her", "their", "our", "your", "my", "its", "in", "on", "at", "to", "for", "of", "from", "by",
    "with", "as", "into", "about", "above", "after", "before", "between", "during", "through",
    "if", "so", "then", "than", "such", "each", "any", "all", "both", "up", "out", "off", "over",
    "under", "again", "further", "there", "here", "where", "when", "while", "although", "though",
    "also", "just", "more", "very", "too", "quite",
];

/// Reciprocal Rank Fusion: merge two ranked lists into one.
/// k=60 is standard (Cormack et al. 2009).
pub fn rrf_merge(semantic: &[(i64, f64)], keyword: &[(i64, f64)], top_k: usize) -> Vec<(i64, f64)> {
    const K: f64 = 60.0;
    let mut rrf: HashMap<i64, f64> = HashMap::new();

    for (rank, &(id, _)) in semantic.iter().enumerate() {
        *rrf.entry(id).or_default() += 1.0 / (K + rank as f64 + 1.0);
    }
    for (rank, &(id, _)) in keyword.iter().enumerate() {
        *rrf.entry(id).or_default() += 1.0 / (K + rank as f64 + 1.0);
    }

    let mut merged: Vec<(i64, f64)> = rrf.into_iter().collect();
    merged.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    merged.truncate(top_k);
    merged
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bm25_finds_relevant() {
        let chunks = vec![
            (
                1i64,
                "Chapter_14_Teacher_League_South_Africa.docx",
                "the teacher's league of south africa tlsa was founded",
            ),
            (
                2i64,
                "Chapter_5_District_Six.docx",
                "district six characters caledon street cape town",
            ),
            (
                3i64,
                "Chapter_15_Apartheid.docx",
                "apartheid laws racial segregation south africa act",
            ),
        ];
        let idx = BM25Index::build(&chunks);
        let results = idx.search("tlsa teacher league south africa", 3);
        assert!(!results.is_empty());
        assert_eq!(results[0].0, 1, "chunk 1 should rank first for TLSA query");
    }

    #[test]
    fn rrf_combines_lists() {
        let semantic = vec![(2i64, 0.9), (1i64, 0.8), (3i64, 0.7)];
        let keyword = vec![(1i64, 5.0), (3i64, 3.0), (2i64, 1.0)];
        let merged = rrf_merge(&semantic, &keyword, 3);
        assert_eq!(merged.len(), 3);
    }
}
