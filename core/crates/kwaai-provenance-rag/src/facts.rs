//! What counts as a fact, and how one is read from the graph.
//!
//! Two kinds of graph content are facts here:
//!
//! - a relation `(src, relation_type, dst)`, with the relation's evidence chunks;
//! - a non-empty entity field value `(entity, field, value)`, with the field's
//!   evidence chunks.
//!
//! Entity descriptions, schema types and embeddings are not facts: they are prose
//! or classification derived from the facts and carry no evidence of their own.
//!
//! Entities are referred to by their graph ID (`entity:<id>`), so a merge, which
//! moves a fact from the alias's ID to the canonical's, changes the fact's claim
//! ID. That is intended: the planner records it as a rewrite.

use kwaai_provenance::{Claim, ClaimKind};
use kwaai_rag::graph::GraphStore;
use std::collections::{BTreeMap, BTreeSet};

/// Structural identity of a fact in the graph.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum FactKey {
    /// A relation between two entities.
    Relation {
        /// Source entity ID.
        src: i64,
        /// Relation type.
        relation: String,
        /// Target entity ID.
        dst: i64,
    },
    /// A field value on one entity.
    Field {
        /// Entity ID.
        entity: i64,
        /// Field name.
        field: String,
        /// Field value.
        value: String,
    },
}

impl FactKey {
    /// The provenance claim for this fact.
    pub fn claim(&self) -> Claim {
        match self {
            FactKey::Relation { src, relation, dst } => Claim::new(
                ClaimKind::Relation,
                entity_ref(*src),
                relation.as_str(),
                entity_ref(*dst),
            ),
            FactKey::Field {
                entity,
                field,
                value,
            } => Claim::new(
                ClaimKind::EntityField,
                entity_ref(*entity),
                field.as_str(),
                value.as_str(),
            ),
        }
    }

    /// This fact with every entity ID passed through `map` (used to follow merges).
    pub fn map_entities(&self, map: impl Fn(i64) -> i64) -> FactKey {
        match self {
            FactKey::Relation { src, relation, dst } => FactKey::Relation {
                src: map(*src),
                relation: relation.clone(),
                dst: map(*dst),
            },
            FactKey::Field {
                entity,
                field,
                value,
            } => FactKey::Field {
                entity: map(*entity),
                field: field.clone(),
                value: value.clone(),
            },
        }
    }

    /// For a field fact, `(entity, field)`; `None` for relations.
    pub fn field_slot(&self) -> Option<(i64, &str)> {
        match self {
            FactKey::Field { entity, field, .. } => Some((*entity, field.as_str())),
            FactKey::Relation { .. } => None,
        }
    }

    /// True for relations.
    pub fn is_relation(&self) -> bool {
        matches!(self, FactKey::Relation { .. })
    }
}

/// A fact with its claim and the evidence chunk IDs the graph currently holds for it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fact {
    /// Structural identity.
    pub key: FactKey,
    /// Claim built from `key`.
    pub claim: Claim,
    /// Evidence chunk IDs as stored in the graph (may include placeholders such as `0`).
    pub evidence: BTreeSet<i64>,
}

impl Fact {
    /// Build a fact from its key and evidence.
    pub fn new(key: FactKey, evidence: impl IntoIterator<Item = i64>) -> Self {
        let claim = key.claim();
        Fact {
            key,
            claim,
            evidence: evidence.into_iter().collect(),
        }
    }
}

/// All facts in the graph, keyed by claim ID.
pub type Snapshot = BTreeMap<String, Fact>;

/// Read every fact from the graph. Read-only.
pub fn snapshot(graph: &GraphStore) -> Snapshot {
    let mut out = Snapshot::new();
    for r in graph.all_relation_records() {
        let fact = Fact::new(
            FactKey::Relation {
                src: r.src_id,
                relation: r.relation_type.clone(),
                dst: r.dst_id,
            },
            r.evidence_chunk_ids.iter().copied(),
        );
        insert(&mut out, fact);
    }
    for node in graph.all_entities() {
        for (field, fv) in &node.fields {
            if fv.value.trim().is_empty() {
                continue;
            }
            let fact = Fact::new(
                FactKey::Field {
                    entity: node.id,
                    field: field.clone(),
                    value: fv.value.clone(),
                },
                fv.evidence_chunk_ids.iter().copied(),
            );
            insert(&mut out, fact);
        }
    }
    out
}

/// Two graph records can map to one claim (e.g. duplicate relation rows); merge
/// their evidence rather than keeping whichever came last.
fn insert(out: &mut Snapshot, fact: Fact) {
    match out.get_mut(&fact.claim.id) {
        Some(existing) => existing.evidence.extend(fact.evidence),
        None => {
            out.insert(fact.claim.id.clone(), fact);
        }
    }
}

/// Artifact ID used in lineage for a graph entity reference inside a claim.
pub fn entity_ref(id: i64) -> String {
    format!("entity:{id}")
}

/// Lineage artifact ID for an evidence chunk. A chunk ID that is not in the chunk
/// store (such as the dream cycle's placeholder `0`) is marked synthetic, so it
/// never counts as original source text.
pub fn chunk_artifact(chunk_id: i64, real_chunks: &BTreeSet<i64>) -> String {
    if real_chunks.contains(&chunk_id) {
        format!("chunk:{chunk_id}")
    } else {
        format!("synthetic:chunk:{chunk_id}")
    }
}

/// Lineage input used when a fact has no evidence chunks at all.
pub const NO_EVIDENCE: &str = "synthetic:no-evidence";

/// True if a lineage artifact is an original source chunk.
pub fn is_original(artifact: &str) -> bool {
    artifact.starts_with("chunk:")
}

/// Parse a chunk artifact (`chunk:N` or `synthetic:chunk:N`) back to its chunk ID.
pub fn parse_chunk_artifact(artifact: &str) -> Option<i64> {
    artifact
        .strip_prefix("chunk:")
        .or_else(|| artifact.strip_prefix("synthetic:chunk:"))
        .and_then(|n| n.parse().ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn claim_ids_differ_by_kind_endpoint_and_value() {
        let r = FactKey::Relation {
            src: 1,
            relation: "parent_of".into(),
            dst: 2,
        };
        let r2 = FactKey::Relation {
            src: 2,
            relation: "parent_of".into(),
            dst: 1,
        };
        let f = FactKey::Field {
            entity: 1,
            field: "born".into(),
            value: "1901".into(),
        };
        let f2 = FactKey::Field {
            entity: 1,
            field: "born".into(),
            value: "1902".into(),
        };
        let ids: BTreeSet<String> = [&r, &r2, &f, &f2].iter().map(|k| k.claim().id).collect();
        assert_eq!(ids.len(), 4);
        assert_eq!(r.claim().id, r.clone().claim().id);
    }

    #[test]
    fn map_entities_follows_a_merge() {
        let r = FactKey::Relation {
            src: 5,
            relation: "knows".into(),
            dst: 9,
        };
        let mapped = r.map_entities(|id| if id == 5 { 7 } else { id });
        assert_eq!(
            mapped,
            FactKey::Relation {
                src: 7,
                relation: "knows".into(),
                dst: 9
            }
        );
    }

    #[test]
    fn chunk_artifacts_mark_placeholders_synthetic() {
        let real = BTreeSet::from([3, 4]);
        assert_eq!(chunk_artifact(3, &real), "chunk:3");
        assert_eq!(chunk_artifact(0, &real), "synthetic:chunk:0");
        assert!(is_original("chunk:3"));
        assert!(!is_original("synthetic:chunk:0"));
        assert!(!is_original(NO_EVIDENCE));
        assert_eq!(parse_chunk_artifact("chunk:3"), Some(3));
        assert_eq!(parse_chunk_artifact("synthetic:chunk:0"), Some(0));
        assert_eq!(parse_chunk_artifact(NO_EVIDENCE), None);
    }
}
