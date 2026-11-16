use kwaai_provenance::{verification::*, Claim, ClaimKind, SupportStatus, VerificationResult};
use kwaai_provenance_eval::{history::evaluate_state, *};
use kwaai_provenance_rag::history::{
    occurrence_id, request_hash, verification_request, Capture, Export, FileIdentity,
    HistoricalAssessment, Run, State, StoreBinding,
};
use std::collections::BTreeMap;
fn fixture() -> (
    tempfile::TempDir,
    Manifest,
    Export,
    HistoricalAssessment,
    Annotation,
) {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("source"), "independent passage").unwrap();
    let m = Manifest::inventory(dir.path()).unwrap();
    let claim = Claim::new(ClaimKind::EntityField, "Alice", "born", "1901");
    let mut c = Capture {
        schema_version: 2,
        run: Run::new(Default::default(), Some(m.corpus_version.clone())),
        cycle: 1,
        stage: "completion".into(),
        boundary: "post_stage".into(),
        sequence: 2,
        captured_at: "2026-11-16T00:00:00Z".into(),
        complete: true,
        error: None,
        cycle_end: None,
        same_state_as: None,
        states: vec![],
        store_binding: Some(StoreBinding {
            graph: FileIdentity {
                canonical_path: "/fixture/graph.db".into(),
                device: 1,
                inode: 1,
            },
            evidence: FileIdentity {
                canonical_path: "/fixture/meta.db".into(),
                device: 1,
                inode: 2,
            },
        }),
    };
    let mut s = State {
        occurrence_id: String::new(),
        claim: claim.clone(),
        bindings: BTreeMap::new(),
        evidence: vec![],
        extraction_contexts: vec![],
    };
    s.occurrence_id = occurrence_id(&c, &s).unwrap();
    c.states.push(s.clone());
    let e = Export {
        schema_version: 2,
        capture: c.clone(),
        occurrence_id: s.occurrence_id.clone(),
        request: verification_request(&c, &s),
    };
    let v = EvidenceVerification {
        verification_schema_version: 1,
        result: VerificationResult {
            claim_id: claim.id.clone(),
            evidence_ids: vec![],
            status: SupportStatus::Unresolved,
            score: None,
            verifier: "fixture".into(),
        },
        implementation: VERSION.into(),
        backend: serde_json::json!({}),
        rationale: "no evidence".into(),
        failure: Some(VerificationFailure::MissingEvidence),
        sources: vec![],
    };
    let a = Annotation {
        schema_version: 1,
        annotation_id: "independent-1".into(),
        claim_id: s.occurrence_id,
        claim_content: serde_json::to_string(&claim).unwrap(),
        evidence: vec![],
        label: Label::Unsupported,
        rationale: "independent fixture label".into(),
        annotator_id: "fixture human".into(),
        timestamp: "2026-11-16T00:00:00Z".parse().unwrap(),
        corpus_version: m.corpus_version.clone(),
        origin: AnnotationOrigin::Independent,
    };
    let v = HistoricalAssessment {
        schema_version: 1,
        occurrence_id: e.occurrence_id.clone(),
        request_sha256: request_hash(&e.request).unwrap(),
        assessment: v,
    };
    (dir, m, e, v, a)
}
#[test]
fn state_annotations_isolated_from_native_ids_and_other_boundaries() {
    let (_dir, m, e, v, a) = fixture();
    let policy = HypotheticalPolicy::Allow;
    let metrics = evaluate_state(&e, &v, std::slice::from_ref(&a), &m, policy).unwrap();
    assert_eq!(metrics.missing_ground_truth, 0);
    assert!(metrics.source_traceability.is_none());
    let mut native = a.clone();
    native.claim_id = e.request.claim.id.clone();
    assert!(evaluate_state(&e, &v, &[native], &m, policy).is_err());
    let mut next = e.clone();
    next.capture.sequence += 1;
    next.capture.states[0].occurrence_id =
        occurrence_id(&next.capture, &next.capture.states[0]).unwrap();
    next.occurrence_id = next.capture.states[0].occurrence_id.clone();
    next.request = verification_request(&next.capture, &next.capture.states[0]);
    assert!(evaluate_state(&next, &v, &[a], &m, policy).is_err());
    assert!(evaluate_state(&next, &v, &[], &m, policy).is_err());
    let mut next_v = v.clone();
    next_v.occurrence_id = next.occurrence_id.clone();
    next_v.request_sha256 = request_hash(&next.request).unwrap();
    assert_eq!(
        evaluate_state(&next, &next_v, &[], &m, policy)
            .unwrap()
            .missing_ground_truth,
        1
    );
}
#[test]
fn verifier_labels_are_not_ground_truth_and_mismatches_fail() {
    let (_dir, m, e, v, mut a) = fixture();
    let policy = HypotheticalPolicy::Allow;
    a.origin = AnnotationOrigin::Verifier;
    assert_eq!(
        evaluate_state(&e, &v, &[a], &m, policy)
            .unwrap()
            .missing_ground_truth,
        1
    );
    let mut bad = e.clone();
    bad.request.claim.object = "different".into();
    assert!(evaluate_state(&bad, &v, &[], &m, policy).is_err());
    let mut bad = e.clone();
    bad.capture.run.corpus = None;
    assert!(evaluate_state(&bad, &v, &[], &m, policy).is_err());
    let mut bad = v.clone();
    bad.assessment.result.evidence_ids.push("wrong".into());
    assert!(evaluate_state(&e, &bad, &[], &m, policy).is_err());
}

#[test]
fn audit_contradictory_completeness_is_rejected() {
    let (_dir, m, mut e, v, a) = fixture();
    e.capture.error = Some("capture failed".into());
    assert!(evaluate_state(&e, &v, &[a], &m, HypotheticalPolicy::Allow).is_err());
}

#[test]
fn same_native_claim_different_evidence_states_accept_opposing_independent_labels() {
    use kwaai_provenance::{EvidenceKind, EvidenceRef};
    use kwaai_provenance_rag::history::Evidence;
    let (_dir, m, base, v, a) = fixture();
    let mut retained = Vec::new();
    for (chunk, label) in [(1, Label::Supported), (2, Label::Unsupported)] {
        let mut e = base.clone();
        let state = &mut e.capture.states[0];
        state.evidence = vec![Evidence {
            reference: EvidenceRef::new(
                EvidenceKind::Derived,
                format!("kb:{}", e.capture.run.tenant_id),
                "",
                Some(chunk),
                vec![],
            ),
            document_id: None,
            chunk_index: None,
            section_type: None,
            blob: None,
            retrieval: "missing".into(),
        }];
        e.capture.states[0].occurrence_id =
            occurrence_id(&e.capture, &e.capture.states[0]).unwrap();
        e.occurrence_id = e.capture.states[0].occurrence_id.clone();
        e.request = verification_request(&e.capture, &e.capture.states[0]);
        let mut assessment = v.clone();
        assessment.occurrence_id = e.occurrence_id.clone();
        assessment.request_sha256 = request_hash(&e.request).unwrap();
        assessment.assessment.result.evidence_ids =
            e.request.evidence.iter().map(|r| r.id.clone()).collect();
        let mut label_record = a.clone();
        label_record.claim_id = e.occurrence_id.clone();
        label_record.label = label;
        label_record.evidence = vec![kwaai_provenance_eval::Evidence {
            document_id: m.documents[0].id.clone(),
            sha256: m.documents[0].sha256.clone(),
            locator: format!("independently supplied reference for evidence state {chunk}"),
        }];
        label_record.annotation_id = format!("human-{chunk}");
        let metrics = evaluate_state(
            &e,
            &assessment,
            std::slice::from_ref(&label_record),
            &m,
            HypotheticalPolicy::Allow,
        )
        .unwrap();
        assert_eq!(metrics.missing_ground_truth, 0);
        let rate = metrics.supported_fact_rate.unwrap();
        assert_eq!(rate.numerator, u64::from(label == Label::Supported));
        assert_eq!(rate.denominator, 1);
        retained.push((e, assessment, label_record));
    }
    assert_eq!(retained[0].0.request.claim, retained[1].0.request.claim);
    assert_ne!(retained[0].0.occurrence_id, retained[1].0.occurrence_id);
    assert!(evaluate_state(
        &retained[0].0,
        &retained[0].1,
        std::slice::from_ref(&retained[1].2),
        &m,
        HypotheticalPolicy::Allow
    )
    .is_err());
    let mut verifier_label = retained[0].2.clone();
    verifier_label.origin = AnnotationOrigin::Verifier;
    assert_eq!(
        evaluate_state(
            &retained[0].0,
            &retained[0].1,
            &[verifier_label],
            &m,
            HypotheticalPolicy::Allow
        )
        .unwrap()
        .missing_ground_truth,
        1
    );
}
#[test]
fn standalone_export_rejects_invalid_metadata_native_ids_duplicates_and_assessment_sources() {
    let (_dir, m, e, v, a) = fixture();
    for mode in 0..8 {
        let mut bad = e.clone();
        match mode {
            0 => bad.capture.run.schema_version = 99,
            1 => bad.capture.captured_at = "invalid timestamp".into(),
            2 => bad.capture.states.push(bad.capture.states[0].clone()),
            3 => {
                bad.capture.states[0].claim.id = "invalid".into();
                bad.request.claim.id = "invalid".into();
            }
            4 => bad.capture.boundary = "invented".into(),
            5 => bad.capture.store_binding = None,
            6 => bad.capture.complete = false,
            _ => bad.capture.same_state_as = Some(bad.capture.sequence),
        }
        assert!(
            evaluate_state(
                &bad,
                &v,
                std::slice::from_ref(&a),
                &m,
                HypotheticalPolicy::Allow
            )
            .is_err(),
            "mode {mode}"
        );
    }
    let mut bad = v.clone();
    bad.assessment.result.status = SupportStatus::Supported;
    assert!(evaluate_state(
        &e,
        &bad,
        std::slice::from_ref(&a),
        &m,
        HypotheticalPolicy::Allow
    )
    .is_err());
    bad.assessment.failure = None;
    assert!(
        evaluate_state(&e, &bad, &[a], &m, HypotheticalPolicy::Allow).is_err(),
        "no-source support is invalid"
    );
}
