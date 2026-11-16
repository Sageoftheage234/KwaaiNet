//! Deterministic research policies. These never apply a graph mutation.
use crate::{
    gate::{Decision, Policy},
    SupportStatus, VerificationResult,
};
use serde::{Deserialize, Serialize};

/// Experimental policies, evaluated over exactly the same verification result.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ShadowPolicy {
    /// Accept every status.
    Allow,
    /// Reject only explicit unsupported verdicts.
    RejectUnsupported,
    /// Accept only explicit supported verdicts.
    RequireSupported,
}
impl ShadowPolicy {
    /// Stable comparison order.
    pub const ALL: [Self; 3] = [Self::Allow, Self::RejectUnsupported, Self::RequireSupported];
    /// Reuse the existing gate mapping without invoking verification or enforcement.
    pub fn accepts(self, status: &SupportStatus) -> bool {
        let policy = match self {
            Self::Allow => Policy {
                on_supported: Decision::Allow,
                on_unsupported: Decision::Allow,
                on_unresolved: Decision::Allow,
            },
            Self::RejectUnsupported => Policy {
                on_unresolved: Decision::Allow,
                ..Policy::default()
            },
            Self::RequireSupported => Policy {
                on_unresolved: Decision::Reject,
                ..Policy::default()
            },
        };
        policy.decide(status) == Decision::Allow
    }
}

/// Timing is explicit; stage observations must never be called proposals.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvaluationKind {
    /// A completed-stage observation, supplied with its retained request.
    PostStage,
    /// Offline evaluation of an explicitly retained pre-application proposal.
    RetainedProposalReplay,
}

/// Attribution supplied by the snapshot/proposal producer, not authenticated here.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EvaluationContext {
    /// Unique evaluation run identity.
    pub run_id: String,
    /// Dream cycle number.
    pub cycle: u64,
    /// Lifecycle stage name.
    pub stage: String,
    /// Distinguishes observations from retained proposals.
    pub kind: EvaluationKind,
}

/// Machine-readable reason, independent of model prose.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReasonCode {
    /// Policy accepts everything.
    AllowAll,
    /// Evidence supported the claim.
    Supported,
    /// Explicit unsupported verdict.
    Unsupported,
    /// Policy permits uncertainty.
    UnresolvedAccepted,
    /// Policy requires support.
    UnresolvedRejected,
}

/// Hypothetical decision only; has no applied-mutation field.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShadowDecision {
    /// Original claim identity.
    pub claim_id: String,
    /// Original, unchanged verification verdict.
    pub status: SupportStatus,
    /// Policy evaluated.
    pub policy: ShadowPolicy,
    /// Hypothetical acceptance.
    pub accepted: bool,
    /// Deterministic reason.
    pub reason: ReasonCode,
    /// Timing and cycle attribution.
    pub context: EvaluationContext,
}

/// Evaluate all policies without inference, I/O, or mutation of the result.
pub fn compare(result: &VerificationResult, context: &EvaluationContext) -> [ShadowDecision; 3] {
    ShadowPolicy::ALL.map(|policy| {
        let accepted = policy.accepts(&result.status);
        let reason = match (policy, result.status, accepted) {
            (ShadowPolicy::Allow, _, _) => ReasonCode::AllowAll,
            (_, SupportStatus::Supported, _) => ReasonCode::Supported,
            (_, SupportStatus::Unsupported, _) => ReasonCode::Unsupported,
            (_, SupportStatus::Unresolved, true) => ReasonCode::UnresolvedAccepted,
            (_, SupportStatus::Unresolved, false) => ReasonCode::UnresolvedRejected,
        };
        ShadowDecision {
            claim_id: result.claim_id.clone(),
            status: result.status,
            policy,
            accepted,
            reason,
            context: context.clone(),
        }
    })
}
