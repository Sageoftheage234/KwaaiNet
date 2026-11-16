//! State-scoped Phase 7B evaluation. Separate explicit API, no Phase 5 reattribution.
use crate::{evaluate, Annotation, HypotheticalPolicy, Manifest, Metrics, Observation};
use anyhow::{ensure, Result};
use kwaai_provenance::SupportStatus;
use kwaai_provenance_rag::history::{
    validate_assessment, validate_export, Export, HistoricalAssessment,
};

/// Annotations must use the state-occurrence ID and exact serialized native Claim.
/// Native IDs and complete historical attribution remain in `export`. Evaluate
/// each state separately; never put native-ID annotations into this protocol.
/// No verifier output is promoted to an independent annotation.
pub fn evaluate_state(
    export: &Export,
    retained: &HistoricalAssessment,
    annotations: &[Annotation],
    manifest: &Manifest,
    policy: HypotheticalPolicy,
) -> Result<Metrics> {
    let state = validate_export(export).map_err(anyhow::Error::msg)?;
    validate_assessment(export, retained).map_err(anyhow::Error::msg)?;
    ensure!(
        export.capture.run.corpus.as_deref() == Some(manifest.corpus_version.as_str()),
        "historical corpus must match evaluation manifest"
    );
    let assessment = &retained.assessment;
    ensure!(
        annotations
            .iter()
            .all(|a| a.claim_id == state.occurrence_id),
        "annotations must be scoped to this state occurrence"
    );
    let judgment = Some(match assessment.result.status {
        SupportStatus::Supported => crate::Label::Supported,
        SupportStatus::Unsupported => crate::Label::Unsupported,
        SupportStatus::Unresolved => crate::Label::Unresolved,
    });
    let observation = Observation {
        claim_id: state.occurrence_id.clone(),
        claim_content: serde_json::to_string(&state.claim)?,
        // Phase 6 document hashes cannot be inferred from passage hashes.
        evidence: vec![],
        judgment,
        traceable: None,
        authenticated: None,
        authentication_basis: None,
        verification_ms: None,
        baseline_ms: None,
    };
    evaluate(&[observation], annotations, manifest, policy)
}
