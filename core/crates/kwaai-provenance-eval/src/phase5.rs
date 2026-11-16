//! Versioned, fail-closed import of Phase 5 JSONL. Metadata is assertion, not attestation.
use crate::*;
use kwaai_provenance::shadow::EvaluationKind;
use kwaai_provenance_rag::shadow::{self, ProvenanceValidation, ShadowEvaluation};

pub const ADAPTER_VERSION: u32 = 1;
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Sidecar {
    pub schema_version: u32,
    pub source_sha256: String,
    pub corpus: Manifest,
    pub model: ModelConfig,
    pub seed: u64,
    pub repetition: u32,
    /// Original evidence ID -> corpus reference. This association is caller asserted.
    pub evidence: BTreeMap<String, Evidence>,
    pub assertion_source: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Bundle {
    pub phase5_adapter_version: u32,
    pub phase6_schema_version: u32,
    /// Exact UTF-8 output bytes, including the producer header and summary.
    pub source_jsonl: String,
    pub sidecar: Sidecar,
    pub sidecar_sha256: String,
}
/// Retains all producer metadata, verification implementations, input hashes,
/// reason codes and caller assertions without promoting any into authentication.
pub struct Imported {
    pub normalized: DecisionImport,
    pub original: Bundle,
    pub evaluations: Vec<ShadowEvaluation>,
    pub header: serde_json::Value,
    pub timing_authenticated: bool,
}
pub fn import(raw: &[u8]) -> Result<Imported> {
    let bundle: Bundle = serde_json::from_slice(raw)?;
    ensure!(
        bundle.phase5_adapter_version == ADAPTER_VERSION && bundle.phase6_schema_version == SCHEMA,
        "unsupported integration schema"
    );
    let s = &bundle.sidecar;
    ensure!(
        s.schema_version == 1
            && hash(bundle.source_jsonl.as_bytes()) == s.source_sha256
            && object_hash(s)? == bundle.sidecar_sha256,
        "sidecar/source hash mismatch"
    );
    s.corpus.validate()?;
    ensure!(
        nonempty(&s.model.identifier) && nonempty(&s.assertion_source),
        "missing model/assertion identity"
    );
    ensure!(
        s.model.digest.as_ref().is_none_or(|d| nonempty(d)),
        "empty model digest"
    );
    let lines: Vec<serde_json::Value> = bundle
        .source_jsonl
        .lines()
        .map(serde_json::from_str)
        .collect::<std::result::Result<_, _>>()?;
    ensure!(lines.len() >= 3, "missing Phase 5 records");
    let header = lines[0].clone();
    let tail = &lines[lines.len() - 1];
    ensure!(
        header["record"] == "shadow_run"
            && header["schema_version"] == shadow::SCHEMA_VERSION
            && tail["record"] == "shadow_summary"
            && tail["schema_version"] == shadow::SCHEMA_VERSION,
        "unsupported Phase 5 envelope"
    );
    ensure!(
        header["manifest_sha256"].as_str().is_some_and(digest),
        "missing producer input hash"
    );
    ensure!(
        header["supplied_metadata"]["corpus_identity"].as_str() == Some(&s.corpus.corpus_version),
        "corpus identity mismatch"
    );
    let mut evaluations = Vec::new();
    let mut records = Vec::new();
    let mut seen = BTreeSet::new();
    let mut attribution = None;
    for line in &lines[1..lines.len() - 1] {
        ensure!(
            line["record"] == "shadow_evaluation",
            "unknown Phase 5 record"
        );
        let row: ShadowEvaluation = serde_json::from_value(line["evaluation"].clone())?;
        ensure!(
            row.schema_version == shadow::SCHEMA_VERSION,
            "unsupported Phase 5 schema"
        );
        // Recompute validation and all three mappings using the actual producer contract.
        let expected = shadow::evaluate(row.input.clone()).map_err(anyhow::Error::msg)?;
        ensure!(
            row.provenance_validation != ProvenanceValidation::Authenticated
                && row.provenance_validation == expected.provenance_validation,
            "provenance promotion or contradiction"
        );
        ensure!(
            row.decisions == expected.decisions,
            "missing, duplicate, inconsistent or reordered policies"
        );
        let c = &row.input.context;
        ensure!(
            header["run_id"].as_str() == Some(&c.run_id) && c.cycle > 0,
            "run/cycle mismatch"
        );
        let key = (
            c.run_id.clone(),
            c.cycle,
            c.stage.clone(),
            match c.kind {
                EvaluationKind::PostStage => 0,
                EvaluationKind::RetainedProposalReplay => 1,
            },
        );
        ensure!(
            attribution.as_ref().is_none_or(|a| a == &key),
            "mixed run/cycle/stage/timing categories"
        );
        attribution = Some(key);
        let claim = &row.input.request.claim;
        ensure!(seen.insert(claim.id.clone()), "repeated claim ID in cell");
        let evidence = row
            .input
            .request
            .evidence
            .iter()
            .map(|e| {
                let mapped = s.evidence.get(&e.id).context("missing evidence mapping")?;
                validate_evidence(mapped, &s.corpus)?;
                Ok(mapped.clone())
            })
            .collect::<Result<Vec<_>>>()?;
        let judgment =
            serde_json::from_value(serde_json::to_value(row.input.assessment.result.status)?)?;
        records.push(Observation {
            claim_id: claim.id.clone(),
            claim_content: serde_json::to_string(claim)?,
            evidence,
            judgment: Some(judgment),
            traceable: None,
            // Unverified is unknown, not an assessed authentication failure.
            authenticated: (row.provenance_validation == ProvenanceValidation::Invalid)
                .then_some(false),
            authentication_basis: (row.provenance_validation == ProvenanceValidation::Invalid)
                .then(|| "Phase 5 identity/hash validation failed".into()),
            verification_ms: None,
            baseline_ms: None,
        });
        evaluations.push(row);
    }
    ensure!(
        tail["summary"] == serde_json::to_value(shadow::summarize(&evaluations))?,
        "producer summary mismatch"
    );
    let cycle = u32::try_from(attribution.context("empty import")?.1)?;
    let normalized = DecisionImport {
        adapter_version: SCHEMA,
        producer_schema: "phase5-shadow-v1/integration-v1".into(),
        corpus_version: s.corpus.corpus_version.clone(),
        model_identifier: s.model.identifier.clone(),
        seed: s.seed,
        cycle,
        repetition: s.repetition,
        records,
    };
    Ok(Imported {
        normalized,
        original: bundle,
        evaluations,
        header,
        timing_authenticated: false,
    })
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Attribution {
    pub adapter_version: u32,
    pub phase5_schema_version: u32,
    pub phase6_schema_version: u32,
    pub context: kwaai_provenance::shadow::EvaluationContext,
    pub source_sha256: String,
    pub sidecar_sha256: String,
    pub capture_timing_authenticated: bool,
    pub metadata_authority: String,
    pub invalid_provenance: usize,
    pub unverified_provenance: usize,
    pub authenticated_provenance: usize,
}
impl Imported {
    pub fn attribution(&self) -> Result<Attribution> {
        Ok(Attribution {
            adapter_version: ADAPTER_VERSION,
            phase5_schema_version: shadow::SCHEMA_VERSION,
            phase6_schema_version: SCHEMA,
            context: self
                .evaluations
                .first()
                .context("empty import")?
                .input
                .context
                .clone(),
            source_sha256: self.original.sidecar.source_sha256.clone(),
            sidecar_sha256: self.original.sidecar_sha256.clone(),
            capture_timing_authenticated: false,
            metadata_authority:
                "caller assertions; hash integrity is not lineage or execution authentication"
                    .into(),
            invalid_provenance: self
                .evaluations
                .iter()
                .filter(|r| r.provenance_validation == ProvenanceValidation::Invalid)
                .count(),
            unverified_provenance: self
                .evaluations
                .iter()
                .filter(|r| r.provenance_validation == ProvenanceValidation::Unverified)
                .count(),
            authenticated_provenance: 0,
        })
    }
}

/// Adapter trait entry point. Retain the original bundle for audit; use `import`
/// when original metadata and attribution are also needed (as the runner does).
pub struct Phase5Adapter;
impl DecisionAdapter for Phase5Adapter {
    fn normalize(&self, raw: &[u8]) -> Result<DecisionImport> {
        Ok(import(raw)?.normalized)
    }
}
