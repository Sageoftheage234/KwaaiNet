use kwaai_provenance::{
    shadow::*, verification::*, Claim, ClaimKind, SupportStatus, VerificationResult,
};
use kwaai_provenance_eval::{phase5::*, *};
use kwaai_provenance_rag::shadow::{self, ShadowInput};
use serde_json::{json, Value};
use std::{collections::BTreeMap, fs};

fn fixture(status: SupportStatus) -> (tempfile::TempDir, Bundle) {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("source"), "independent source").unwrap();
    let corpus = Manifest::inventory(dir.path()).unwrap();
    let claim = Claim::new(ClaimKind::EntityField, "Alice", "year", "1901");
    let evidence = kwaai_provenance::EvidenceRef::new(
        kwaai_provenance::EvidenceKind::Original,
        "fixture-source",
        source_hash("independent source"),
        Some(1),
        vec![],
    );
    let row = shadow::evaluate(ShadowInput {
        context: EvaluationContext {
            run_id: "original-run".into(),
            cycle: 1,
            stage: "completion".into(),
            kind: EvaluationKind::PostStage,
        },
        request: VerificationRequest {
            claim: claim.clone(),
            evidence: vec![evidence.clone()],
            entity_bindings: BTreeMap::new(),
            metadata: BTreeMap::new(),
        },
        assessment: EvidenceVerification {
            verification_schema_version: 1,
            result: VerificationResult {
                claim_id: claim.id,
                evidence_ids: vec![evidence.id.clone()],
                status,
                score: None,
                verifier: "fixture".into(),
            },
            implementation: VERSION.into(),
            backend: json!({"model":"fixture"}),
            rationale: "scripted".into(),
            failure: None,
            sources: vec![],
        },
        ground_truth: None,
    })
    .unwrap();
    let source_jsonl = [json!({"record":"shadow_run","schema_version":1,"run_id":"original-run","manifest_sha256":hash(b"retained inputs"),"supplied_metadata":{"corpus_identity":corpus.corpus_version}}),json!({"record":"shadow_evaluation","evaluation":row}), json!({"record":"shadow_summary","schema_version":1,"summary":shadow::summarize(&[row])})].iter().map(Value::to_string).collect::<Vec<_>>().join("\n");
    let sidecar = Sidecar {
        schema_version: 1,
        source_sha256: hash(source_jsonl.as_bytes()),
        corpus: corpus.clone(),
        model: ModelConfig {
            identifier: "fixture".into(),
            digest: None,
            parameters: BTreeMap::new(),
        },
        seed: 42,
        repetition: 0,
        evidence: BTreeMap::from([(
            evidence.id,
            Evidence {
                document_id: corpus.documents[0].id.clone(),
                sha256: corpus.documents[0].sha256.clone(),
                locator: "bytes:0..18".into(),
            },
        )]),
        assertion_source: "retained experimental protocol".into(),
    };
    let sidecar_sha256 = object_hash(&sidecar).unwrap();
    (
        dir,
        Bundle {
            phase5_adapter_version: 1,
            phase6_schema_version: 1,
            source_jsonl,
            sidecar,
            sidecar_sha256,
        },
    )
}
fn encode(b: &Bundle) -> Vec<u8> {
    serde_json::to_vec(b).unwrap()
}
fn bind(b: &mut Bundle) {
    b.sidecar.source_sha256 = hash(b.source_jsonl.as_bytes());
    b.sidecar_sha256 = object_hash(&b.sidecar).unwrap();
}
fn mutate(b: &mut Bundle, f: impl FnOnce(&mut Vec<Value>)) {
    let mut rows = b
        .source_jsonl
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    f(&mut rows);
    b.source_jsonl = rows
        .iter()
        .map(Value::to_string)
        .collect::<Vec<_>>()
        .join("\n");
    bind(b);
}
#[test]
fn actual_serialization_all_nine_policy_decisions() {
    for (status, expected) in [
        (SupportStatus::Supported, [true, true, true]),
        (SupportStatus::Unsupported, [true, false, false]),
        (SupportStatus::Unresolved, [true, true, false]),
    ] {
        let (_d, b) = fixture(status);
        let i = import(&encode(&b)).unwrap();
        assert_eq!(
            i.evaluations[0].decisions.clone().map(|d| d.accepted),
            expected
        );
        assert_eq!(
            i.normalized.records[0].claim_id,
            i.evaluations[0].input.request.claim.id
        );
        for (policy, accepted) in [
            HypotheticalPolicy::Allow,
            HypotheticalPolicy::RejectUnsupported,
            HypotheticalPolicy::RequireSupported,
        ]
        .into_iter()
        .zip(expected)
        {
            assert_eq!(policy.accepts(i.normalized.records[0].judgment), accepted);
        }
        assert_eq!(i.original.source_jsonl, b.source_jsonl);
        assert!(!i.timing_authenticated);
    }
}
#[test]
fn rejects_malformed_policies_statuses_versions_and_identity() {
    for case in 0..9 {
        let (_d, mut b) = fixture(SupportStatus::Unresolved);
        mutate(&mut b, |rows| {
            let e = &mut rows[1]["evaluation"];
            match case {
                0 => {
                    e["decisions"].as_array_mut().unwrap().pop();
                }
                1 => e["decisions"][1] = e["decisions"][0].clone(),
                2 => e["decisions"][1]["accepted"] = json!(false),
                3 => e["schema_version"] = json!(2),
                4 => e["input"]["context"]["run_id"] = json!(""),
                5 => e["decisions"][0]["status"] = json!("unsupported"),
                6 => e["decisions"][0]["policy"] = json!("unknown"),
                7 => e["provenance_validation"] = json!("authenticated"),
                _ => e["decisions"][0]["reason"] = json!("supported"),
            }
        });
        assert!(import(&encode(&b)).is_err(), "case {case}");
    }
}
#[test]
fn invalid_and_unverified_remain_distinct() {
    let (_d, mut b) = fixture(SupportStatus::Supported);
    assert!(import(&encode(&b)).unwrap().normalized.records[0]
        .authenticated
        .is_none());
    mutate(&mut b, |rows| {
        let mut row: shadow::ShadowEvaluation =
            serde_json::from_value(rows[1]["evaluation"].clone()).unwrap();
        row.input.request.claim.object = "tampered".into();
        let row = shadow::evaluate(row.input).unwrap();
        rows[1]["evaluation"] = serde_json::to_value(&row).unwrap();
        rows[2]["summary"] = serde_json::to_value(shadow::summarize(&[row])).unwrap();
    });
    assert_eq!(
        import(&encode(&b)).unwrap().normalized.records[0].authenticated,
        Some(false)
    );
}
#[test]
fn retained_replay_is_an_unauthenticated_timing_assertion() {
    let (_d, mut b) = fixture(SupportStatus::Supported);
    mutate(&mut b, |rows| {
        let e = &mut rows[1]["evaluation"];
        e["input"]["context"]["kind"] = json!("retained_proposal_replay");
        for d in e["decisions"].as_array_mut().unwrap() {
            d["context"]["kind"] = json!("retained_proposal_replay");
        }
    });
    let i = import(&encode(&b)).unwrap();
    assert_eq!(
        i.evaluations[0].input.context.kind,
        EvaluationKind::RetainedProposalReplay
    );
    assert!(!i.timing_authenticated);
}
#[test]
fn required_sidecar_metadata_and_hash_binding() {
    for case in 0..5 {
        let (_d, mut b) = fixture(SupportStatus::Supported);
        match case {
            0 => b.sidecar.model.identifier.clear(),
            1 => b.sidecar.corpus.corpus_version.clear(),
            2 => b.sidecar.seed += 1,
            3 => b.source_jsonl.push('\n'),
            _ => b.phase6_schema_version = 99,
        }
        if case < 2 {
            bind(&mut b);
        }
        assert!(import(&encode(&b)).is_err());
    }
}
#[test]
fn repeated_ids_and_mixed_timing_are_rejected() {
    for mixed in [false, true] {
        let (_d, mut b) = fixture(SupportStatus::Supported);
        mutate(&mut b, |rows| {
            let mut duplicate = rows[1].clone();
            if mixed {
                duplicate["evaluation"]["input"]["context"]["kind"] =
                    json!("retained_proposal_replay");
            }
            rows.insert(2, duplicate);
        });
        assert!(import(&encode(&b)).is_err());
    }
}
#[test]
fn end_to_end_evaluation_independent_annotations_and_resume_integrity() {
    let (dir, b) = fixture(SupportStatus::Unsupported);
    let i = import(&encode(&b)).unwrap();
    let o = &i.normalized.records[0];
    let a = Annotation {
        schema_version: 1,
        annotation_id: "independent".into(),
        claim_id: o.claim_id.clone(),
        claim_content: o.claim_content.clone(),
        evidence: vec![],
        label: Label::Unsupported,
        rationale: "independent search".into(),
        annotator_id: "annotator".into(),
        timestamp: "2026-01-01T00:00:00Z".parse().unwrap(),
        corpus_version: b.sidecar.corpus.corpus_version.clone(),
        origin: AnnotationOrigin::Independent,
    };
    assert_eq!(
        evaluate(
            &i.normalized.records,
            &[],
            &b.sidecar.corpus,
            HypotheticalPolicy::Allow
        )
        .unwrap()
        .missing_ground_truth,
        1
    );
    assert_eq!(
        evaluate(
            &i.normalized.records,
            std::slice::from_ref(&a),
            &b.sidecar.corpus,
            HypotheticalPolicy::Allow
        )
        .unwrap()
        .false_acceptance_rate
        .unwrap()
        .denominator,
        1
    );
    let root = dir.path().join("corpus");
    fs::create_dir(&root).unwrap();
    fs::rename(dir.path().join("source"), root.join("source")).unwrap();
    let path = |name: &str| dir.path().join(name).to_str().unwrap().to_owned();
    write_new(path("manifest"), &b.sidecar.corpus).unwrap();
    write_new(path("annotations"), &vec![a]).unwrap();
    fs::write(path("input"), encode(&b)).unwrap();
    let c = Config {
        schema_version: 1,
        run_id: "integration".into(),
        models: vec![b.sidecar.model.clone()],
        manifest: path("manifest"),
        corpus_root: root.to_str().unwrap().into(),
        sample_count: 1,
        seeds: vec![42],
        cycles: 1,
        repetitions: 1,
        verification: BTreeMap::new(),
        policies: vec![
            HypotheticalPolicy::Allow,
            HypotheticalPolicy::RejectUnsupported,
            HypotheticalPolicy::RequireSupported,
        ],
        annotations: path("annotations"),
        inputs: vec![InputCell {
            model: "fixture".into(),
            repetition: 0,
            cycle: 1,
            path: path("input"),
        }],
        output_dir: path("output"),
    };
    run(c.clone(), false).unwrap();
    run(c.clone(), true).unwrap();
    let mut changed = b.clone();
    changed.sidecar.assertion_source.push_str(" changed");
    bind(&mut changed);
    fs::write(path("input"), encode(&changed)).unwrap();
    assert!(run(c.clone(), true).is_err());
    let mut changed = b.clone();
    changed.source_jsonl.push('\n');
    bind(&mut changed);
    assert!(import(&encode(&changed)).is_ok());
    fs::write(path("input"), encode(&changed)).unwrap();
    assert!(run(c, true).is_err());
}

#[test]
fn evidence_references_are_preserved_and_missing_mappings_rejected() {
    let (_d, mut b) = fixture(SupportStatus::Supported);
    let i = import(&encode(&b)).unwrap();
    let original = &i.evaluations[0].input.request.evidence[0];
    assert_eq!(
        i.normalized.records[0].evidence[0].sha256,
        original.content_hash
    );
    assert_eq!(
        i.normalized.records[0].evidence[0].document_id,
        b.sidecar.corpus.documents[0].id
    );
    b.sidecar.evidence.clear();
    bind(&mut b);
    assert!(import(&encode(&b)).is_err());
}

#[test]
fn producer_labels_are_retained_but_never_become_independent_annotations() {
    let (_d, mut b) = fixture(SupportStatus::Supported);
    mutate(&mut b, |rows| {
        let mut row: shadow::ShadowEvaluation =
            serde_json::from_value(rows[1]["evaluation"].clone()).unwrap();
        row.input.ground_truth = Some(shadow::GroundTruth {
            status: SupportStatus::Unsupported,
            source: "caller-asserted label".into(),
        });
        let row = shadow::evaluate(row.input).unwrap();
        rows[1]["evaluation"] = serde_json::to_value(&row).unwrap();
        rows[2]["summary"] = serde_json::to_value(shadow::summarize(&[row])).unwrap();
    });
    let i = import(&encode(&b)).unwrap();
    assert!(i.evaluations[0].input.ground_truth.is_some());
    let metrics = evaluate(
        &i.normalized.records,
        &[],
        &b.sidecar.corpus,
        HypotheticalPolicy::Allow,
    )
    .unwrap();
    assert_eq!(metrics.missing_ground_truth, 1);
    assert!(metrics.supported_fact_rate.is_none());
    let o = &i.normalized.records[0];
    let mut a = Annotation {
        schema_version: 1,
        annotation_id: "a".into(),
        claim_id: o.claim_id.clone(),
        claim_content: o.claim_content.clone(),
        evidence: o.evidence.clone(),
        label: Label::Unresolved,
        rationale: "independent uncertainty".into(),
        annotator_id: "human".into(),
        timestamp: "2026-01-01T00:00:00Z".parse().unwrap(),
        corpus_version: b.sidecar.corpus.corpus_version.clone(),
        origin: AnnotationOrigin::Independent,
    };
    assert_eq!(
        evaluate(
            &i.normalized.records,
            std::slice::from_ref(&a),
            &b.sidecar.corpus,
            HypotheticalPolicy::Allow
        )
        .unwrap()
        .missing_ground_truth,
        1
    );
    let unresolved = a.clone();
    a.annotation_id = "b".into();
    a.label = Label::Supported;
    assert_eq!(
        evaluate(
            &i.normalized.records,
            &[unresolved, a],
            &b.sidecar.corpus,
            HypotheticalPolicy::Allow
        )
        .unwrap()
        .conflicting_ground_truth,
        1
    );
}
