use kwaai_provenance_eval::*;
use std::{collections::BTreeMap, fs};
fn corpus() -> (tempfile::TempDir, Manifest) {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("a.txt"), b"abc").unwrap();
    fs::write(dir.path().join("b.txt"), b"def").unwrap();
    let m = Manifest::inventory(dir.path()).unwrap();
    (dir, m)
}
fn evidence(m: &Manifest) -> Evidence {
    Evidence {
        document_id: m.documents[0].id.clone(),
        sha256: m.documents[0].sha256.clone(),
        locator: "bytes:0..3".into(),
    }
}
fn annotation(m: &Manifest, label: Label) -> Annotation {
    Annotation {
        schema_version: 1,
        annotation_id: "a1".into(),
        claim_id: "c1".into(),
        claim_content: "A claim".into(),
        evidence: vec![evidence(m)],
        label,
        rationale: "independently assigned fixture".into(),
        annotator_id: "human-fixture".into(),
        timestamp: "2026-01-01T00:00:00Z".parse().unwrap(),
        corpus_version: m.corpus_version.clone(),
        origin: AnnotationOrigin::Independent,
    }
}
fn observation(m: &Manifest) -> Observation {
    Observation {
        claim_id: "c1".into(),
        claim_content: "A claim".into(),
        evidence: vec![evidence(m)],
        judgment: Some(Label::Supported),
        traceable: Some(true),
        authenticated: None,
        authentication_basis: None,
        verification_ms: Some(2),
        baseline_ms: Some(10),
    }
}
#[test]
fn hashing_inventory_sampling_integrity() {
    let (dir, m) = corpus();
    assert_eq!(
        hash(b"abc"),
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
    assert_eq!(m, Manifest::inventory(dir.path()).unwrap());
    assert_eq!(m.sample(42, 1).unwrap(), m.sample(42, 1).unwrap());
    assert_eq!(
        m.sample(42, 2).unwrap(),
        vec![
            "doc:18b7cb099a9ea3f50ba899b5ba81e0d377a5f3b16f8f6eeb8b3e58cd4692b993",
            "doc:ffa0da5d885fba09d903c782713b6b098c8cf21f56a3a35d9aa920613220d2e1"
        ]
    );
    assert!(m.sample(42, 3).is_err());
    let id = m.documents[0].id.clone();
    fs::write(dir.path().join("a.txt"), "changed").unwrap();
    assert!(m.check(dir.path()).is_err());
    assert_eq!(id, Manifest::inventory(dir.path()).unwrap().documents[0].id);
}
#[test]
fn annotation_validation_and_disagreements() {
    let (_, m) = corpus();
    let a = annotation(&m, Label::Supported);
    let mut b = annotation(&m, Label::Unsupported);
    b.annotation_id = "a2".into();
    validate_annotations(&[a.clone(), b.clone()], &m).unwrap();
    assert!(validate_annotations(&[a.clone(), a.clone()], &m).is_err());
    let metrics = evaluate(
        &[observation(&m)],
        &[a, b],
        &m,
        HypotheticalPolicy::RequireSupported,
    )
    .unwrap();
    assert_eq!(metrics.conflicting_ground_truth, 1);
    assert!(metrics.precision.is_none());
    let mut bad = annotation(&m, Label::Supported);
    bad.evidence[0].sha256 = "0".repeat(64);
    assert!(validate_annotations(&[bad], &m).is_err());
}
#[test]
fn missing_labels_never_use_verifier_as_truth() {
    let (_, m) = corpus();
    let mut a = annotation(&m, Label::Supported);
    a.origin = AnnotationOrigin::Verifier;
    let v = evaluate(&[observation(&m)], &[a], &m, HypotheticalPolicy::Allow).unwrap();
    assert!(v.supported_fact_rate.is_none());
    assert!(v.precision.is_none());
    assert_eq!(v.missing_ground_truth, 1);
    assert_eq!(v.source_traceability.unwrap().estimate, 1.0);
    assert!(v.provenance_authentication_rate.is_none());
}
#[test]
fn metrics_confusion_matrix_and_intervals() {
    let (_, m) = corpus();
    let mut obs = Vec::new();
    let mut labels = Vec::new();
    for (i, (truth, judgment)) in [
        (Label::Supported, Label::Supported),
        (Label::Supported, Label::Unsupported),
        (Label::Unsupported, Label::Supported),
        (Label::Unsupported, Label::Unsupported),
    ]
    .into_iter()
    .enumerate()
    {
        let mut o = observation(&m);
        o.claim_id = format!("c{i}");
        o.judgment = Some(judgment);
        obs.push(o);
        let mut a = annotation(&m, truth);
        a.claim_id = format!("c{i}");
        a.annotation_id = format!("a{i}");
        labels.push(a);
    }
    let v = evaluate(&obs, &labels, &m, HypotheticalPolicy::RequireSupported).unwrap();
    for r in [
        v.supported_fact_rate,
        v.unsupported_fact_rate,
        v.false_acceptance_rate,
        v.false_rejection_rate,
        v.precision,
        v.recall,
    ] {
        let r = r.unwrap();
        assert_eq!(r.estimate, 0.5);
        assert!(r.wilson_95[0] < 0.5 && r.wilson_95[1] > 0.5);
    }
    assert_eq!(v.runtime_overhead_ratio, Some(0.2));
    let baseline = evaluate(&obs, &labels, &m, HypotheticalPolicy::Baseline).unwrap();
    assert_eq!(baseline.recall.unwrap().estimate, 1.0);
    assert_eq!(baseline.false_acceptance_rate.unwrap().estimate, 1.0);
    assert!(HypotheticalPolicy::RejectUnsupported.accepts(None));
    assert!(!HypotheticalPolicy::RequireSupported.accepts(None));
}
#[test]
fn invalid_provenance_and_schema() {
    let (_, m) = corpus();
    let mut o = observation(&m);
    o.evidence[0].sha256 = "invalid".into();
    assert!(evaluate(&[o], &[], &m, HypotheticalPolicy::Allow).is_err());
    assert!(FixtureAdapter
        .normalize(br#"{"adapter_version":2,"producer_schema":"future","records":[]}"#)
        .is_err());
    assert!(FixtureAdapter
        .normalize(
            br#"{"adapter_version":1,"producer_schema":"fixture-v1","records":[],"unknown":true}"#
        )
        .is_err());
    let mut bad = m.clone();
    bad.schema_version = 2;
    assert!(bad.validate().is_err());
    let (_, m) = corpus();
    let mut a = annotation(&m, Label::Supported);
    a.schema_version = 2;
    assert!(validate_annotations(&[a], &m).is_err());
}
fn setup() -> (tempfile::TempDir, Config) {
    let (dir, m) = corpus();
    let p = |name: &str| dir.path().join(name).to_str().unwrap().to_owned();
    // Keep corpus originals in their own root, separate from generated experiment artifacts.
    let root = dir.path().join("corpus");
    fs::create_dir(&root).unwrap();
    fs::rename(dir.path().join("a.txt"), root.join("a.txt")).unwrap();
    fs::rename(dir.path().join("b.txt"), root.join("b.txt")).unwrap();
    write_new(p("manifest.json"), &m).unwrap();
    write_new(
        p("annotations.json"),
        &vec![annotation(&m, Label::Supported)],
    )
    .unwrap();
    write_new(
        p("input.json"),
        &DecisionImport {
            adapter_version: 1,
            producer_schema: "deterministic-fixture-v1".into(),
            corpus_version: m.corpus_version.clone(),
            model_identifier: "fixture".into(),
            seed: 42,
            cycle: 1,
            repetition: 0,
            records: vec![observation(&m)],
        },
    )
    .unwrap();
    let c = Config {
        schema_version: 1,
        run_id: "test".into(),
        models: vec![ModelConfig {
            identifier: "fixture".into(),
            digest: None,
            parameters: BTreeMap::new(),
        }],
        manifest: p("manifest.json"),
        corpus_root: p("corpus"),
        sample_count: 2,
        seeds: vec![42],
        cycles: 1,
        repetitions: 1,
        verification: BTreeMap::new(),
        policies: vec![
            HypotheticalPolicy::Baseline,
            HypotheticalPolicy::RequireSupported,
        ],
        annotations: p("annotations.json"),
        inputs: vec![InputCell {
            model: "fixture".into(),
            repetition: 0,
            cycle: 1,
            path: p("input.json"),
        }],
        output_dir: p("outputs"),
    };
    (dir, c)
}
#[test]
fn resume_retains_complete_cells_and_rejects_changed_inputs() {
    let (dir, c) = setup();
    run(c.clone(), false).unwrap();
    let out = dir.path().join("outputs/test/cell-000000.json");
    let original = fs::read(&out).unwrap();
    run(c.clone(), true).unwrap();
    assert_eq!(original, fs::read(&out).unwrap());
    assert!(run(c.clone(), false).is_err());
    fs::write(&c.inputs[0].path, b"{}").unwrap();
    assert!(run(c, true).is_err());
}
#[test]
fn partial_cell_is_never_completed_and_missing_cell_is_recovered() {
    let (dir, c) = setup();
    run(c.clone(), false).unwrap();
    let out = dir.path().join("outputs/test/cell-000000.json");
    fs::rename(&out, dir.path().join("retained-cell.json")).unwrap();
    run(c.clone(), true).unwrap();
    assert!(out.exists());
    fs::write(&out, b"{\"schema_version\":").unwrap();
    assert!(run(c, true).is_err());
}
#[test]
fn failed_cells_have_reports_and_incomplete_matrices_fail() {
    let (dir, mut c) = setup();
    let mut imported: DecisionImport = read_json(&c.inputs[0].path).unwrap();
    imported.adapter_version = 99;
    fs::write(&c.inputs[0].path, serde_json::to_vec(&imported).unwrap()).unwrap();
    assert!(run(c.clone(), false).is_err());
    let report: serde_json::Value =
        read_json(dir.path().join("outputs/test/attempt-000000.json")).unwrap();
    assert_eq!(report["complete"], false);
    assert!(!dir.path().join("outputs/test/cell-000000.json").exists());
    assert!(run(c.clone(), true).is_err());
    c.inputs.clear();
    assert!(run(c, true).is_err());
}
#[cfg(unix)]
#[test]
fn symlinks_are_rejected() {
    use std::os::unix::fs::symlink;
    let (dir, _) = corpus();
    symlink("a.txt", dir.path().join("link")).unwrap();
    assert!(Manifest::inventory(dir.path()).is_err());
}

#[test]
fn missing_measurements_and_unresolved_truth() {
    let (_, m) = corpus();
    let mut o = observation(&m);
    o.verification_ms = None;
    o.traceable = None;
    let a = annotation(&m, Label::Unresolved);
    let v = evaluate(&[o.clone()], &[a], &m, HypotheticalPolicy::RequireSupported).unwrap();
    assert!(v.runtime_overhead_ratio.is_none());
    assert!(v.source_traceability.is_none());
    assert!(v.supported_fact_rate.is_none());
    o.authenticated = Some(true);
    assert!(evaluate(&[o.clone()], &[], &m, HypotheticalPolicy::Allow).is_err());
    o.authentication_basis = Some("fixture-test: retained synthetic assessment abc".into());
    assert_eq!(
        evaluate(&[o], &[], &m, HypotheticalPolicy::Allow)
            .unwrap()
            .provenance_authentication_rate
            .unwrap()
            .estimate,
        1.0
    );
}
#[test]
fn committed_matrix_fixture_and_schema_roundtrip() {
    let scratch = tempfile::tempdir().unwrap();
    let crate_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let core = crate_root.parent().unwrap().parent().unwrap();
    let mut c: Config = read_json(crate_root.join("fixtures/config.json")).unwrap();
    let absolute = |s: &str| core.join(s).to_str().unwrap().to_owned();
    c.manifest = absolute(&c.manifest);
    c.corpus_root = absolute(&c.corpus_root);
    c.annotations = absolute(&c.annotations);
    for cell in &mut c.inputs {
        cell.path = absolute(&cell.path);
    }
    c.output_dir = scratch.path().to_str().unwrap().to_owned();
    let roundtrip: Config = serde_json::from_slice(&serde_json::to_vec(&c).unwrap()).unwrap();
    run(roundtrip, false).unwrap();
    run(c.clone(), true).unwrap();
    for i in 0..8 {
        let cell: CellResult = read_json(
            scratch
                .path()
                .join("synthetic-smoke")
                .join(format!("cell-{i:06}.json")),
        )
        .unwrap();
        assert_eq!(cell.metrics.len(), 4);
    }
    let mut imported: DecisionImport = read_json(&c.inputs[0].path).unwrap();
    imported.adapter_version = 2;
    assert!(FixtureAdapter
        .normalize(&serde_json::to_vec(&imported).unwrap())
        .is_err());
    imported.adapter_version = 1;
    assert!(FixtureAdapter
        .normalize(&serde_json::to_vec(&imported).unwrap())
        .is_ok());
}
#[test]
fn imported_trial_identity_mismatch_is_reported() {
    let (dir, c) = setup();
    let mut input: DecisionImport = read_json(&c.inputs[0].path).unwrap();
    input.seed = 999;
    fs::write(&c.inputs[0].path, serde_json::to_vec(&input).unwrap()).unwrap();
    assert!(run(c, false).is_err());
    let report: serde_json::Value =
        read_json(dir.path().join("outputs/test/attempt-000000.json")).unwrap();
    assert!(report["failures"][0]
        .as_str()
        .unwrap()
        .contains("identity mismatch"));
}

#[test]
fn timestamps_and_unknown_annotation_fields_are_rejected() {
    let (_, m) = corpus();
    let mut a = serde_json::to_value(annotation(&m, Label::Supported)).unwrap();
    a["timestamp"] = serde_json::json!("yesterday");
    assert!(serde_json::from_value::<Annotation>(a.clone()).is_err());
    a["timestamp"] = serde_json::json!("2026-01-01T00:00:00Z");
    a["unexpected"] = serde_json::json!(true);
    assert!(serde_json::from_value::<Annotation>(a).is_err());
}

#[test]
fn ground_truth_cannot_use_evidence_outside_sample() {
    let (dir, mut c) = setup();
    c.sample_count = 1;
    let m: Manifest = read_json(&c.manifest).unwrap();
    let mut a: Vec<Annotation> = read_json(&c.annotations).unwrap();
    a[0].evidence = vec![Evidence {
        document_id: m.documents[1].id.clone(),
        sha256: m.documents[1].sha256.clone(),
        locator: "line:1".into(),
    }];
    fs::write(&c.annotations, serde_json::to_vec(&a).unwrap()).unwrap();
    assert!(run(c, false).is_err());
    let report: serde_json::Value =
        read_json(dir.path().join("outputs/test/attempt-000000.json")).unwrap();
    assert!(report["failures"][0]
        .as_str()
        .unwrap()
        .contains("ground truth evidence outside selected corpus"));
}

fn corrupt_cell(c: &Config, change: impl FnOnce(&mut serde_json::Value)) -> String {
    let path = std::path::Path::new(&c.output_dir)
        .join(&c.run_id)
        .join("cell-000000.json");
    let mut cell: serde_json::Value = read_json(&path).unwrap();
    change(&mut cell);
    fs::write(&path, serde_json::to_vec(&cell).unwrap()).unwrap();
    let before = fs::read(&path).unwrap();
    let error = format!("{:#}", run(c.clone(), true).unwrap_err());
    assert_eq!(
        before,
        fs::read(&path).unwrap(),
        "invalid results must be preserved for audit"
    );
    assert!(
        !path.parent().unwrap().join("attempt-000001.json").exists(),
        "invalid resume must not report success"
    );
    error
}

#[test]
fn resume_rejects_empty_metrics() {
    let (_dir, c) = setup();
    run(c.clone(), false).unwrap();
    assert!(
        corrupt_cell(&c, |cell| cell["metrics"] = serde_json::json!({}))
            .contains("policy coverage")
    );
}

#[test]
fn resume_rejects_missing_and_unexpected_policies() {
    for missing in [true, false] {
        let (_dir, c) = setup();
        run(c.clone(), false).unwrap();
        let error = corrupt_cell(&c, |cell| {
            if missing {
                cell["metrics"].as_object_mut().unwrap().remove("baseline");
            } else {
                cell["metrics"]["allow"] = cell["metrics"]["baseline"].clone();
            }
        });
        assert!(error.contains("policy coverage"), "{error}");
    }
}

#[test]
fn resume_rejects_duplicate_policy_keys() {
    let (dir, c) = setup();
    run(c.clone(), false).unwrap();
    let path = dir.path().join("outputs/test/cell-000000.json");
    let cell: serde_json::Value = read_json(&path).unwrap();
    let raw = fs::read_to_string(&path).unwrap();
    let duplicate = format!(
        "\"metrics\": {{\"baseline\": {},",
        cell["metrics"]["baseline"]
    );
    fs::write(&path, raw.replacen("\"metrics\": {", &duplicate, 1)).unwrap();
    let error = format!("{:#}", run(c, true).unwrap_err());
    assert!(error.contains("duplicate saved policy result"), "{error}");
}

#[test]
fn resume_requires_every_metric_field_even_when_nullable() {
    for field in [
        "supported_fact_rate",
        "unsupported_fact_rate",
        "source_traceability",
        "provenance_authentication_rate",
        "false_rejection_rate",
        "false_acceptance_rate",
        "precision",
        "recall",
        "runtime_overhead_ratio",
        "observations",
        "missing_ground_truth",
        "conflicting_ground_truth",
    ] {
        let (_dir, c) = setup();
        run(c.clone(), false).unwrap();
        let error = corrupt_cell(&c, |cell| {
            cell["metrics"]["baseline"]
                .as_object_mut()
                .unwrap()
                .remove(field);
        });
        assert!(error.contains("missing field"), "{field}: {error}");
    }
}

#[test]
fn resume_rejects_wrong_denominators_and_inconsistent_confusion_matrix() {
    for field in [
        "supported_fact_rate",
        "recall",
        "precision",
        "false_rejection_rate",
    ] {
        let (_dir, c) = setup();
        run(c.clone(), false).unwrap();
        let error = corrupt_cell(&c, |cell| {
            cell["metrics"]["baseline"][field]["denominator"] = serde_json::json!(99)
        });
        assert!(error.contains("numerator/denominator"), "{field}: {error}");
    }
    let (_dir, c) = setup();
    run(c.clone(), false).unwrap();
    // Each rate is individually plausible but recall=0 contradicts the known
    // baseline TP=1 and false-rejection=0. Recomputed counts must reject it.
    let error = corrupt_cell(
        &c,
        |cell| cell["metrics"]["baseline"]["recall"] = serde_json::json!({"numerator":0,"denominator":1,"estimate":0.0,"wilson_95":[0.0,0.7934506856227626]}),
    );
    assert!(error.contains("confusion-matrix mismatch"), "{error}");
}

#[test]
fn resume_rejects_corrupt_estimates_intervals_counts_and_nulls() {
    for (field, value) in [
        ("estimate", serde_json::json!(0.25)),
        ("estimate", serde_json::json!(0.9999999999999)),
        ("wilson_95", serde_json::json!([0.9, 0.1])),
        ("wilson_95", serde_json::json!([0.0, 1.0])),
    ] {
        let (_dir, c) = setup();
        run(c.clone(), false).unwrap();
        let error = corrupt_cell(&c, |cell| {
            cell["metrics"]["baseline"]["precision"][field] = value
        });
        assert!(error.contains("invalid"), "{error}");
    }
    for field in [
        "observations",
        "missing_ground_truth",
        "conflicting_ground_truth",
    ] {
        let (_dir, c) = setup();
        run(c.clone(), false).unwrap();
        assert!(corrupt_cell(&c, |cell| cell["metrics"]["baseline"][field] =
            serde_json::json!(42))
        .contains("count mismatch"));
    }
    let (_dir, c) = setup();
    run(c.clone(), false).unwrap();
    assert!(
        corrupt_cell(&c, |cell| cell["metrics"]["baseline"]["precision"] =
            serde_json::Value::Null)
        .contains("missing or unexpected metric")
    );
    let (_dir, c) = setup();
    run(c.clone(), false).unwrap();
    assert!(corrupt_cell(&c, |cell| cell["metrics"]["baseline"]
        ["runtime_overhead_ratio"] =
        serde_json::json!(0.7))
    .contains("runtime overhead"));
}

#[test]
fn resume_rejects_changed_implementation_identity_and_legacy_headers() {
    let (dir, c) = setup();
    run(c.clone(), false).unwrap();
    let path = dir.path().join("outputs/test/run.json");
    let original = fs::read(&path).unwrap();
    let mut meta: RunMetadata = read_json(&path).unwrap();
    let current = implementation_identity();
    assert_eq!(meta.evaluator, current);
    // Model a correctly framed fingerprint for a different source implementation.
    meta.evaluator.files.insert(
        "core/crates/kwaai-provenance-eval/src/lib.rs".into(),
        hash(b"changed implementation"),
    );
    let mut bytes = b"kwaai-provenance-eval-implementation-v1\0".to_vec();
    bytes.extend(serde_json::to_vec(&meta.evaluator.files).unwrap());
    meta.evaluator.sha256 = hash(&bytes);
    fs::write(&path, serde_json::to_vec(&meta).unwrap()).unwrap();
    let error = format!("{:#}", run(c.clone(), true).unwrap_err());
    assert!(
        error.contains("implementation fingerprint mismatch"),
        "{error}"
    );
    fs::write(&path, &original).unwrap();
    run(c.clone(), true).unwrap();
    let report: serde_json::Value =
        read_json(dir.path().join("outputs/test/attempt-000001.json")).unwrap();
    assert_eq!(report["evaluator"]["sha256"], current.sha256);
    let mut legacy: serde_json::Value = serde_json::from_slice(&original).unwrap();
    legacy.as_object_mut().unwrap().remove("evaluator");
    fs::write(&path, serde_json::to_vec(&legacy).unwrap()).unwrap();
    assert!(format!("{:#}", run(c, true).unwrap_err()).contains("legacy run metadata"));
}

#[test]
fn resume_rejects_saved_cell_from_different_implementation() {
    let (_dir, c) = setup();
    run(c.clone(), false).unwrap();
    assert!(corrupt_cell(&c, |cell| cell["evaluator"]["sha256"] =
        serde_json::json!("0".repeat(64)))
    .contains("incompatible completed cell"));
}
