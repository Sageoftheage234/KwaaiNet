//! Opt-in post-stage comparison of retained Phase 4 assessments. No graph handle,
//! inference, source retrieval, or observer registration is performed here.
use kwaai_provenance::{
    claim_id, evidence_id,
    shadow::{compare, EvaluationContext, ShadowDecision, ShadowPolicy},
    verification::{EvidenceVerification, VerificationFailure, VerificationRequest},
    SupportStatus,
};
use serde::{Deserialize, Serialize};

/// Separate envelope version; does not change provenance or verification schemas.
pub const SCHEMA_VERSION: u32 = 1;

/// Association authenticity, separate from semantic support and ID consistency.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProvenanceValidation {
    /// Reserved for a trusted lineage association validator; never emitted here.
    Authenticated,
    /// Caller association has not been authenticated against trusted lineage.
    Unverified,
    /// Identity/hash validation failed. This does not alter the supplied verdict.
    Invalid,
}

/// Optional independent labels. Their independence is a caller assertion retained
/// for audit; verifier classifications must never be used to populate this field.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroundTruth {
    /// Independently labeled semantic support, not verifier output.
    pub status: SupportStatus,
    /// External annotation source / dataset version.
    pub source: String,
}

/// One verification-ready completed-stage observation or retained proposal.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ShadowInput {
    /// Snapshot/proposal attribution, explicitly supplied, not independently validated.
    pub context: EvaluationContext,
    /// Full unchanged Phase 4 request; historical gate logs alone are insufficient.
    pub request: VerificationRequest,
    /// Full unchanged Phase 4 assessment, including failure and source hashes.
    pub assessment: EvidenceVerification,
    /// Independent annotations, if available.
    #[serde(default)]
    pub ground_truth: Option<GroundTruth>,
}

/// A comparison, preserving all original inputs.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShadowEvaluation {
    /// Version of this output envelope.
    pub schema_version: u32,
    /// Unchanged request, assessment and supplied attribution.
    pub input: ShadowInput,
    /// Independently computed validation classification, never caller-authenticated.
    pub provenance_validation: ProvenanceValidation,
    /// All policies over the same immutable result.
    pub decisions: [ShadowDecision; 3],
}

/// Consume a retained assessment. Malformed envelopes abort rather than silently
/// mixing claim identities. Invalid identities remain explicit observations.
pub fn evaluate(input: ShadowInput) -> Result<ShadowEvaluation, String> {
    if input.context.run_id.trim().is_empty() || input.context.stage.trim().is_empty() {
        return Err("run ID and stage must be nonempty".into());
    }
    if input.assessment.verification_schema_version != 1 {
        return Err("unsupported verification envelope version".into());
    }
    let request = &input.request;
    let result = &input.assessment.result;
    if result.claim_id != request.claim.id
        || result.evidence_ids
            != request
                .evidence
                .iter()
                .map(|e| e.id.clone())
                .collect::<Vec<_>>()
    {
        return Err("assessment does not match retained request".into());
    }
    if let Some(label) = &input.ground_truth {
        if label.source.trim().is_empty() || label.status == SupportStatus::Unresolved {
            return Err("ground truth requires a source and supported/unsupported label".into());
        }
    }
    let invalid = request.claim.id
        != claim_id(
            request.claim.kind.clone(),
            &request.claim.subject,
            &request.claim.predicate,
            &request.claim.object,
        )
        || request.evidence.iter().any(|e| {
            e.id != evidence_id(e.kind.clone(), &e.source_id, &e.content_hash, e.chunk_id)
        })
        || matches!(
            input.assessment.failure,
            Some(VerificationFailure::InvalidIdentity | VerificationFailure::HashMismatch)
        );
    let provenance_validation = if invalid {
        ProvenanceValidation::Invalid
    } else {
        ProvenanceValidation::Unverified
    };
    let decisions = compare(result, &input.context);
    Ok(ShadowEvaluation {
        schema_version: SCHEMA_VERSION,
        input,
        provenance_validation,
        decisions,
    })
}

/// Per-policy totals and optional independently labeled confusion counts.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyMetrics {
    /// Policy identity.
    pub policy: ShadowPolicy,
    /// Hypothetical accepted count.
    pub accepted: usize,
    /// Hypothetical rejected count.
    pub rejected: usize,
    /// Rejected / evaluated; absent for empty inputs.
    pub rejection_rate: Option<f64>,
    /// Accepted unresolved claims.
    pub unresolved_accepted: usize,
    /// Rejected unresolved claims.
    pub unresolved_rejected: usize,
    /// Labeled supported claims incorrectly rejected.
    pub false_rejections: usize,
    /// Labeled unsupported claims incorrectly accepted.
    pub false_acceptances: usize,
    /// False rejections / labeled supported claims.
    pub false_rejection_rate: Option<f64>,
    /// False acceptances / labeled unsupported claims.
    pub false_acceptance_rate: Option<f64>,
    /// Unsupported labels among rejected labeled claims.
    pub unsupported_precision: Option<f64>,
    /// Rejected unsupported labels / all unsupported labels.
    pub unsupported_recall: Option<f64>,
}
/// Aggregate comparison; runtime is measured by the runner, not inferred here.
#[derive(Debug, Serialize, Deserialize)]
pub struct ShadowSummary {
    /// Total input claims.
    pub total: usize,
    /// Supported verdicts.
    pub supported: usize,
    /// Unsupported verdicts.
    pub unsupported: usize,
    /// Unresolved verdicts.
    pub unresolved: usize,
    /// Phase 4 missing-evidence failures.
    pub missing_evidence: usize,
    /// All Phase 4 verification failures, including missing evidence.
    pub verification_failures: usize,
    /// Invalid provenance observations.
    pub invalid_provenance: usize,
    /// Unauthenticated associations.
    pub unverified_provenance: usize,
    /// Number with caller-asserted independent labels.
    pub labeled: usize,
    /// One comparison per policy.
    pub policies: Vec<PolicyMetrics>,
}
fn ratio(n: usize, d: usize) -> Option<f64> {
    (d != 0).then(|| n as f64 / d as f64)
}
/// Compare policies using optional independent annotations, never verifier verdicts as truth.
pub fn summarize(rows: &[ShadowEvaluation]) -> ShadowSummary {
    let count = |status| {
        rows.iter()
            .filter(|r| r.input.assessment.result.status == status)
            .count()
    };
    let supported_labels = rows
        .iter()
        .filter(|r| {
            r.input
                .ground_truth
                .as_ref()
                .is_some_and(|l| l.status == SupportStatus::Supported)
        })
        .count();
    let unsupported_labels = rows
        .iter()
        .filter(|r| {
            r.input
                .ground_truth
                .as_ref()
                .is_some_and(|l| l.status == SupportStatus::Unsupported)
        })
        .count();
    let policies = ShadowPolicy::ALL
        .into_iter()
        .enumerate()
        .map(|(i, policy)| {
            let accepted = rows.iter().filter(|r| r.decisions[i].accepted).count();
            let rejected = rows.len() - accepted;
            let mut fp = 0;
            let mut fn_ = 0;
            let mut tp = 0;
            for row in rows {
                if let Some(label) = &row.input.ground_truth {
                    match (label.status, row.decisions[i].accepted) {
                        (SupportStatus::Supported, false) => fp += 1,
                        (SupportStatus::Unsupported, true) => fn_ += 1,
                        (SupportStatus::Unsupported, false) => tp += 1,
                        _ => {}
                    }
                }
            }
            let unresolved_accepted = rows
                .iter()
                .filter(|r| {
                    r.input.assessment.result.status == SupportStatus::Unresolved
                        && r.decisions[i].accepted
                })
                .count();
            PolicyMetrics {
                policy,
                accepted,
                rejected,
                rejection_rate: ratio(rejected, rows.len()),
                unresolved_accepted,
                unresolved_rejected: count(SupportStatus::Unresolved) - unresolved_accepted,
                false_rejections: fp,
                false_acceptances: fn_,
                false_rejection_rate: ratio(fp, supported_labels),
                false_acceptance_rate: ratio(fn_, unsupported_labels),
                unsupported_precision: ratio(tp, tp + fp),
                unsupported_recall: ratio(tp, unsupported_labels),
            }
        })
        .collect();
    ShadowSummary {
        total: rows.len(),
        supported: count(SupportStatus::Supported),
        unsupported: count(SupportStatus::Unsupported),
        unresolved: count(SupportStatus::Unresolved),
        missing_evidence: rows
            .iter()
            .filter(|r| r.input.assessment.failure == Some(VerificationFailure::MissingEvidence))
            .count(),
        verification_failures: rows
            .iter()
            .filter(|r| r.input.assessment.failure.is_some())
            .count(),
        invalid_provenance: rows
            .iter()
            .filter(|r| r.provenance_validation == ProvenanceValidation::Invalid)
            .count(),
        unverified_provenance: rows
            .iter()
            .filter(|r| r.provenance_validation == ProvenanceValidation::Unverified)
            .count(),
        labeled: supported_labels + unsupported_labels,
        policies,
    }
}

/// Verify an explicitly retained completed-stage request through Phase 4, then
/// compare policies. Caller must obtain the request from the corresponding
/// snapshot; this function never reconstructs historical data or opens a graph.
pub async fn verify_post_stage(
    context: EvaluationContext,
    request: VerificationRequest,
    resolver: &dyn kwaai_provenance::verification::EvidenceResolver,
    backend: &dyn kwaai_provenance::verification::EvidenceBackend,
    max_input_bytes: usize,
) -> Result<ShadowEvaluation, String> {
    if context.kind != kwaai_provenance::shadow::EvaluationKind::PostStage {
        return Err("verify_post_stage requires post_stage attribution".into());
    }
    let assessment =
        kwaai_provenance::verification::verify(&request, resolver, backend, max_input_bytes).await;
    evaluate(ShadowInput {
        context,
        request,
        assessment,
        ground_truth: None,
    })
}
