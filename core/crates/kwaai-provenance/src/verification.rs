//! Opt-in factual support verification. No graph mutation or lineage rewriting.
use crate::{
    claim_id, evidence_id, Claim, EvidenceKind, EvidenceRef, SupportStatus, VerificationResult,
};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

/// Version of the prompt, validation rules, and implementation.
pub const VERSION: &str = "kwaai-evidence-verifier/1";
/// Exact original-text hash used by the provenance observer.
pub fn source_hash(text: &str) -> String {
    hex::encode(Sha256::digest(text.as_bytes()))
}

/// A claim and recorded evidence references. Entity bindings provide names for
/// opaque graph IDs without changing the content-derived claim identity.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerificationRequest {
    /// Existing provenance claim, never replaced by a paraphrase.
    pub claim: Claim,
    /// Recorded references, including original hashes (not rebaselined at read time).
    pub evidence: Vec<EvidenceRef>,
    /// Exact entity ID -> name bindings from the same graph snapshot.
    #[serde(default)]
    pub entity_bindings: BTreeMap<String, String>,
    /// Caller provenance context (e.g. lineage digest and snapshot identity).
    #[serde(default)]
    pub metadata: BTreeMap<String, String>,
}

/// Retrieved original source text and its document name.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourcePassage {
    /// Source namespace, e.g. `kb:<tenant>`.
    pub source_id: String,
    /// Original document identifier in the source store.
    pub document_id: String,
    /// Full original chunk, never a graph summary.
    pub text: String,
}

/// Read-only original evidence resolution.
pub trait EvidenceResolver {
    /// Return an original passage, absent passage, or retrieval error.
    fn resolve(&self, evidence: &EvidenceRef) -> Result<Option<SourcePassage>, String>;
}

/// Validated passage, retaining the recorded identity and computed hash.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResolvedEvidence {
    /// Original recorded reference.
    pub reference: EvidenceRef,
    /// Retrieved source text.
    pub passage: SourcePassage,
    /// Hash computed from the retrieved bytes.
    pub retrieved_hash: String,
}

/// Backend output is deliberately untrusted until parsed by [`verify`].
#[async_trait]
pub trait EvidenceBackend: Sync {
    /// Stable verifier identity, including backend/model when applicable.
    fn identity(&self) -> String;
    /// Implementation, model, digest, and inference configuration.
    fn configuration(&self) -> serde_json::Value;
    /// Assess entailment of the specific claim by the original evidence.
    async fn infer(
        &self,
        request: &VerificationRequest,
        evidence: &[ResolvedEvidence],
    ) -> Result<String, String>;
}

/// Explicit failure categories; ordinary semantic uncertainty has no failure.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VerificationFailure {
    /// Claim/evidence identity or entity binding invalid.
    InvalidIdentity,
    /// No original source text was available.
    MissingEvidence,
    /// Resolver failed.
    Retrieval,
    /// Retrieved bytes do not match the recorded SHA-256.
    HashMismatch,
    /// Timeout, unavailable model, or transport error.
    Inference,
    /// Invalid JSON, classification, or explanation.
    InvalidResponse,
    /// Full input exceeds the configured context budget.
    InputTooLarge,
}

/// Additive envelope preserves the existing `VerificationResult` schema.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvidenceVerification {
    /// Envelope version, separate from provenance v1 IDs and records.
    pub verification_schema_version: u32,
    /// Reusable result for the existing policy gate.
    pub result: VerificationResult,
    /// Version of prompt and validation implementation.
    pub implementation: String,
    /// Backend/model identity and inference parameters for this assessment.
    pub backend: serde_json::Value,
    /// Model rationale or explicit failure explanation.
    pub rationale: String,
    /// Explicit failure, when verification could not complete.
    pub failure: Option<VerificationFailure>,
    /// Validated original sources actually retrieved before completion/failure.
    pub sources: Vec<ResolvedEvidence>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Verdict {
    status: SupportStatus,
    rationale: String,
}

/// Resolve and validate every reference before inference. Any missing or invalid
/// reference makes the entire assessment unresolved, even if others are valid.
/// The budget rejects oversized inputs; it never silently truncates evidence.
pub async fn verify(
    request: &VerificationRequest,
    resolver: &dyn EvidenceResolver,
    backend: &dyn EvidenceBackend,
    max_input_bytes: usize,
) -> EvidenceVerification {
    let mut out = EvidenceVerification {
        verification_schema_version: 1,
        result: VerificationResult {
            claim_id: request.claim.id.clone(),
            evidence_ids: request.evidence.iter().map(|e| e.id.clone()).collect(),
            status: SupportStatus::Unresolved,
            score: None,
            verifier: backend.identity(),
        },
        implementation: VERSION.into(),
        backend: backend.configuration(),
        rationale: String::new(),
        failure: None,
        sources: vec![],
    };
    let c = &request.claim;
    let failure = if claim_id(c.kind.clone(), &c.subject, &c.predicate, &c.object) != c.id
        || [&c.subject, &c.object].iter().any(|s| {
            s.starts_with("entity:")
                && request
                    .entity_bindings
                    .get(*s)
                    .is_none_or(|n| n.trim().is_empty())
        }) {
        Some((
            VerificationFailure::InvalidIdentity,
            "invalid claim ID or missing entity binding".into(),
        ))
    } else if request.evidence.is_empty() {
        Some((
            VerificationFailure::MissingEvidence,
            "no evidence references".into(),
        ))
    } else {
        let mut error = None;
        for e in &request.evidence {
            if evidence_id(e.kind.clone(), &e.source_id, &e.content_hash, e.chunk_id) != e.id {
                error = Some((
                    VerificationFailure::InvalidIdentity,
                    format!("invalid evidence ID: {}", e.id),
                ));
                break;
            }
            if e.kind != EvidenceKind::Original {
                error = Some((
                    VerificationFailure::MissingEvidence,
                    format!("derived evidence is not an original passage: {}", e.id),
                ));
                break;
            }
            match resolver.resolve(e) {
                Ok(Some(p)) if p.source_id == e.source_id && !p.text.trim().is_empty() => {
                    let hash = source_hash(&p.text);
                    if hash != e.content_hash {
                        error = Some((
                            VerificationFailure::HashMismatch,
                            format!("source hash mismatch: {}", e.id),
                        ));
                        break;
                    }
                    out.sources.push(ResolvedEvidence {
                        reference: e.clone(),
                        passage: p,
                        retrieved_hash: hash,
                    });
                }
                Ok(_) => {
                    error = Some((
                        VerificationFailure::MissingEvidence,
                        format!("missing or wrong source: {}", e.id),
                    ));
                    break;
                }
                Err(e) => {
                    error = Some((VerificationFailure::Retrieval, e));
                    break;
                }
            }
        }
        error
    };
    if let Some((kind, reason)) = failure {
        out.failure = Some(kind);
        out.rationale = reason;
        return out;
    }
    let bytes = serde_json::to_vec(&(request, &out.sources))
        .expect("serializable evidence")
        .len();
    if bytes > max_input_bytes {
        out.failure = Some(VerificationFailure::InputTooLarge);
        out.rationale = format!("input {bytes} bytes exceeds budget {max_input_bytes}");
        return out;
    }
    match backend.infer(request, &out.sources).await {
        Err(e) => {
            out.failure = Some(VerificationFailure::Inference);
            out.rationale = e;
        }
        Ok(raw) => match serde_json::from_str::<Verdict>(&raw) {
            Ok(v) if !v.rationale.trim().is_empty() => {
                out.result.status = v.status;
                out.rationale = v.rationale;
            }
            _ => {
                out.failure = Some(VerificationFailure::InvalidResponse);
                out.rationale = "invalid structured verdict".into();
            }
        },
    }
    out
}

/// Deterministic scripted backend for tests and offline harness plumbing.
/// Labels are specified independently by the caller, never inferred by this mock.
pub struct MockVerifier {
    /// Claim ID -> raw model response or explicit inference failure.
    pub responses: BTreeMap<String, Result<String, String>>,
}
#[async_trait]
impl EvidenceBackend for MockVerifier {
    fn identity(&self) -> String {
        format!("mock/{VERSION}")
    }
    fn configuration(&self) -> serde_json::Value {
        serde_json::json!({"backend":"mock", "version":VERSION})
    }
    async fn infer(
        &self,
        request: &VerificationRequest,
        _: &[ResolvedEvidence],
    ) -> Result<String, String> {
        self.responses
            .get(&request.claim.id)
            .cloned()
            .unwrap_or_else(|| Err("no scripted response".into()))
    }
}

#[cfg(feature = "ollama")]
pub mod ollama;
