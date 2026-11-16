use kwaai_provenance::{shadow::*, verification::*, Claim, ClaimKind, SupportStatus};
use kwaai_provenance_rag::shadow::*;
use std::collections::BTreeMap;

fn input(status: SupportStatus, failure: Option<VerificationFailure>) -> ShadowInput {
    let claim = Claim::new(ClaimKind::EntityField, "Alice", "birth_year", "1901");
    ShadowInput {
        context: EvaluationContext {
            run_id: "fixture-run".into(),
            cycle: 7,
            stage: "completion".into(),
            kind: EvaluationKind::PostStage,
        },
        request: VerificationRequest {
            claim: claim.clone(),
            evidence: vec![],
            entity_bindings: BTreeMap::new(),
            metadata: BTreeMap::from([("snapshot".into(), "retained-fixture".into())]),
        },
        assessment: EvidenceVerification {
            verification_schema_version: 1,
            result: kwaai_provenance::VerificationResult {
                claim_id: claim.id,
                evidence_ids: vec![],
                status,
                score: None,
                verifier: "scripted-fixture".into(),
            },
            implementation: VERSION.into(),
            backend: serde_json::json!({"model":"fixture","digest":null}),
            rationale: "independently scripted fixture".into(),
            failure,
            sources: vec![],
        },
        ground_truth: None,
    }
}
#[test]
fn all_nine_combinations_stable_and_attributed() {
    for (status, expected) in [
        (SupportStatus::Supported, [true, true, true]),
        (SupportStatus::Unsupported, [true, false, false]),
        (SupportStatus::Unresolved, [true, true, false]),
    ] {
        let original = input(status, None);
        let before = serde_json::to_string(&original).unwrap();
        let a = evaluate(original.clone()).unwrap();
        let b = evaluate(original).unwrap();
        assert_eq!(a.decisions, b.decisions);
        assert_eq!(serde_json::to_string(&a.input).unwrap(), before);
        assert_eq!(
            serde_json::to_string(&a).unwrap(),
            serde_json::to_string(&b).unwrap()
        );
        assert_eq!(a.provenance_validation, ProvenanceValidation::Unverified);
        for (i, d) in a.decisions.iter().enumerate() {
            assert_eq!(d.accepted, expected[i]);
            assert_eq!(d.status, status);
            assert_eq!(d.context.cycle, 7);
            assert_eq!(d.context.stage, "completion");
            assert_eq!(d.context.kind, EvaluationKind::PostStage);
            assert_eq!(d.claim_id, a.input.assessment.result.claim_id);
        }
    }
}
#[test]
fn failures_and_conflict_are_preserved_without_support_invention() {
    let mut conflict = input(SupportStatus::Unresolved, None);
    conflict.assessment.rationale = "Conflicting original passages; insufficient support".into();
    let rows = vec![
        evaluate(conflict).unwrap(),
        evaluate(input(
            SupportStatus::Unresolved,
            Some(VerificationFailure::MissingEvidence),
        ))
        .unwrap(),
        evaluate(input(
            SupportStatus::Unresolved,
            Some(VerificationFailure::HashMismatch),
        ))
        .unwrap(),
    ];
    let s = summarize(&rows);
    assert_eq!(
        (
            s.total,
            s.unresolved,
            s.missing_evidence,
            s.verification_failures,
            s.invalid_provenance,
            s.unverified_provenance
        ),
        (3, 3, 1, 2, 1, 2)
    );
    assert_eq!(s.policies[1].unresolved_accepted, 3);
    assert_eq!(s.policies[2].unresolved_rejected, 3);
    assert_eq!(s.policies[2].false_rejection_rate, None);
}
#[test]
fn malformed_identity_is_invalid_and_mismatched_assessment_rejected() {
    let mut record = input(SupportStatus::Supported, None);
    record.request.claim.object = "forged".into();
    assert_eq!(
        evaluate(record).unwrap().provenance_validation,
        ProvenanceValidation::Invalid
    );
    let mut record = input(SupportStatus::Supported, None);
    record.assessment.result.claim_id = "different".into();
    assert!(evaluate(record).is_err());
    let mut record = input(SupportStatus::Supported, None);
    record.context.run_id.clear();
    assert!(evaluate(record).is_err());
}
#[test]
fn independent_labels_use_correct_denominators() {
    let rows: Vec<_> = [
        (SupportStatus::Supported, SupportStatus::Unsupported),
        (SupportStatus::Unsupported, SupportStatus::Supported),
        (SupportStatus::Unresolved, SupportStatus::Supported),
    ]
    .into_iter()
    .map(|(status, label)| {
        let mut r = input(status, None);
        r.ground_truth = Some(GroundTruth {
            status: label,
            source: "external-annotation-v1".into(),
        });
        evaluate(r).unwrap()
    })
    .collect();
    let s = summarize(&rows);
    assert_eq!(s.labeled, 3);
    assert_eq!(s.policies[0].false_acceptance_rate, Some(1.0));
    assert_eq!(s.policies[1].false_rejection_rate, Some(0.5));
    assert_eq!(s.policies[2].false_rejection_rate, Some(1.0));
    assert_eq!(s.policies[2].unsupported_precision, Some(0.0));
    assert_eq!(s.policies[2].unsupported_recall, Some(0.0));
    assert_eq!(summarize(&[]).policies[0].rejection_rate, None);
}
#[cfg(feature = "verification")]
#[test]
fn cli_bounded_exclusive_and_no_source_or_graph_access() {
    let dir = tempfile::tempdir().unwrap();
    let manifest = dir.path().join("input.jsonl");
    let output = dir.path().join("output.jsonl");
    // Sentinel stands for the closest supported boundary: this runner never opens a graph.
    let graph = dir.path().join("graph-sentinel");
    std::fs::write(&graph, b"unchanged graph bytes").unwrap();
    let row = input(
        SupportStatus::Unresolved,
        Some(VerificationFailure::MissingEvidence),
    );
    std::fs::write(
        &manifest,
        format!(
            "{}\ninvalid record beyond limit\n",
            serde_json::to_string(&row).unwrap()
        ),
    )
    .unwrap();
    let run = || {
        std::process::Command::new(env!("CARGO_BIN_EXE_provenance-shadow"))
            .args([
                manifest.to_str().unwrap(),
                output.to_str().unwrap(),
                "1",
                "fixture-run",
                "fixture-corpus",
            ])
            .output()
            .unwrap()
    };
    assert!(run().status.success());
    let before = std::fs::read(&output).unwrap();
    let records: Vec<serde_json::Value> = std::str::from_utf8(&before)
        .unwrap()
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    assert_eq!(records.len(), 3);
    assert_eq!(records[2]["summary"]["missing_evidence"], 1);
    assert!(!run().status.success());
    assert_eq!(std::fs::read(&output).unwrap(), before);
    assert_eq!(std::fs::read(&graph).unwrap(), b"unchanged graph bytes");
    std::fs::remove_file(&output).unwrap();
    std::fs::write(&manifest, "malformed\n").unwrap();
    assert!(!run().status.success());
    assert!(!output.exists());
}

#[tokio::test]
async fn post_stage_reuses_phase4_for_missing_and_conflicting_evidence() {
    struct Resolver;
    impl EvidenceResolver for Resolver {
        fn resolve(
            &self,
            e: &kwaai_provenance::EvidenceRef,
        ) -> Result<Option<SourcePassage>, String> {
            let text = if e.chunk_id == Some(1) {
                "Alice was born in 1901."
            } else {
                "Alice was born in 1902."
            };
            Ok(Some(SourcePassage {
                source_id: e.source_id.clone(),
                document_id: "fixture.txt".into(),
                text: text.into(),
            }))
        }
    }
    let mut original = input(SupportStatus::Unresolved, None);
    let backend = MockVerifier { responses: BTreeMap::from([(original.request.claim.id.clone(),Ok(r#"{"status":"unresolved","rationale":"The two original passages conflict about the birth year."}"#.into()))]) };
    let missing = verify_post_stage(
        original.context.clone(),
        original.request.clone(),
        &Resolver,
        &backend,
        100_000,
    )
    .await
    .unwrap();
    assert_eq!(
        missing.input.assessment.failure,
        Some(VerificationFailure::MissingEvidence)
    );
    for (id, text) in [
        (1, "Alice was born in 1901."),
        (2, "Alice was born in 1902."),
    ] {
        original
            .request
            .evidence
            .push(kwaai_provenance::EvidenceRef::new(
                kwaai_provenance::EvidenceKind::Original,
                "fixture",
                source_hash(text),
                Some(id),
                vec![],
            ));
    }
    let conflict = verify_post_stage(
        original.context.clone(),
        original.request.clone(),
        &Resolver,
        &backend,
        100_000,
    )
    .await
    .unwrap();
    assert_eq!(
        conflict.input.assessment.result.status,
        SupportStatus::Unresolved
    );
    assert_eq!(conflict.input.assessment.failure, None);
    assert_eq!(conflict.input.assessment.sources.len(), 2);
    assert_eq!(
        conflict.provenance_validation,
        ProvenanceValidation::Unverified
    );
    assert_eq!(conflict.decisions.map(|d| d.accepted), [true, true, false]);
    original.context.kind = EvaluationKind::RetainedProposalReplay;
    assert!(verify_post_stage(
        original.context,
        original.request,
        &Resolver,
        &backend,
        100_000
    )
    .await
    .is_err());
}

#[tokio::test]
async fn completed_graph_snapshot_verification_does_not_change_persisted_graph() {
    use kwaai_provenance::{EvidenceKind, EvidenceRef};
    use kwaai_provenance_rag::{facts, verification::OriginalEvidenceResolver};
    use kwaai_rag::{
        graph::{EntityNode, FieldValue, GraphStore},
        meta_store::{ChunkMeta, MetaStore},
    };
    let dir = tempfile::tempdir().unwrap();
    let tenant = uuid::Uuid::new_v4();
    let text = "Alice was born in 1901.";
    let meta = MetaStore::open(dir.path(), tenant).unwrap();
    meta.put_chunks(
        &[ChunkMeta {
            doc_name: "original.txt".into(),
            chunk_index: 0,
            text: text.into(),
            surrounding: String::new(),
            page_num: None,
            ingested_at: "fixture".into(),
            section_name: None,
            skip_extraction: false,
            section_note: None,
            section_type: Default::default(),
        }],
        &[1],
    )
    .unwrap();
    drop(meta);
    let mut graph = GraphStore::open(dir.path(), tenant).unwrap();
    graph
        .upsert_entity(EntityNode {
            id: 42,
            name: "Alice".into(),
            entity_type: "Person".into(),
            description: String::new(),
            embedding: vec![0.1],
            mention_count: 2,
            first_chunk_id: 1,
            aliases: vec![],
            schema_type: None,
            gender: None,
            evidence: vec![],
            fields: std::collections::HashMap::from([(
                "birth_year".into(),
                FieldValue {
                    value: "1901".into(),
                    evidence_chunk_ids: vec![1],
                    confidence: 0.5,
                },
            )]),
            confidence: 0.5,
            extraction_confidence: 1.0,
        })
        .unwrap();
    let before = facts::snapshot(&graph);
    let fact = before.values().next().unwrap();
    let request = VerificationRequest {
        claim: fact.claim.clone(),
        evidence: vec![EvidenceRef::new(
            EvidenceKind::Original,
            format!("kb:{tenant}"),
            source_hash(text),
            Some(1),
            vec![],
        )],
        entity_bindings: BTreeMap::from([("entity:42".into(), "Alice".into())]),
        metadata: BTreeMap::from([("snapshot".into(), "completed-stage-fixture".into())]),
    };
    drop(graph);
    let graph_path = dir.path().join(format!("graph-{tenant}.db"));
    let graph_bytes = std::fs::read(&graph_path).unwrap();
    let source_path = dir.path().join(format!("{tenant}.db"));
    let source_bytes = std::fs::read(&source_path).unwrap();
    let backend = MockVerifier {
        responses: BTreeMap::from([(
            request.claim.id.clone(),
            Ok(
                r#"{"status":"supported","rationale":"The original passage gives the year."}"#
                    .into(),
            ),
        )]),
    };
    let meta = MetaStore::open_read_only(dir.path(), tenant).unwrap();
    let result = verify_post_stage(
        input(SupportStatus::Supported, None).context,
        request,
        &OriginalEvidenceResolver(&meta),
        &backend,
        100_000,
    )
    .await
    .unwrap();
    assert_eq!(
        result.input.assessment.result.status,
        SupportStatus::Supported
    );
    assert_eq!(
        result.provenance_validation,
        ProvenanceValidation::Unverified
    );
    drop(meta);
    assert_eq!(std::fs::read(graph_path).unwrap(), graph_bytes);
    assert_eq!(std::fs::read(source_path).unwrap(), source_bytes);
    let graph = GraphStore::open(dir.path(), tenant).unwrap();
    assert_eq!(facts::snapshot(&graph), before);
}
