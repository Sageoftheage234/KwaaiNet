use kwaai_provenance::{
    verification::*, Claim, ClaimKind, EvidenceKind, EvidenceRef, SupportStatus,
};
use kwaai_provenance_rag::verification::OriginalEvidenceResolver;
use kwaai_rag::meta_store::{ChunkMeta, MetaStore};
use std::collections::BTreeMap;
use uuid::Uuid;

#[tokio::test]
async fn original_store_round_trip_read_only_and_hash_drift() {
    let dir = tempfile::tempdir().unwrap();
    let tenant = Uuid::new_v4();
    let text = "Alice was born in 1901.";
    let chunk = ChunkMeta {
        doc_name: "original.txt".into(),
        chunk_index: 0,
        text: text.into(),
        surrounding: "Generated summary must never be used".into(),
        page_num: None,
        ingested_at: "fixture".into(),
        section_name: None,
        skip_extraction: false,
        section_note: None,
        section_type: Default::default(),
    };
    let store = MetaStore::open(dir.path(), tenant).unwrap();
    store
        .put_chunks(std::slice::from_ref(&chunk), &[42])
        .unwrap();
    drop(store);
    let path = dir.path().join(format!("{tenant}.db"));
    let before = std::fs::read(&path).unwrap();
    let claim = Claim::new(ClaimKind::EntityField, "Alice", "birth_year", "1901");
    let evidence = EvidenceRef::new(
        EvidenceKind::Original,
        format!("kb:{tenant}"),
        source_hash(text),
        Some(42),
        vec![],
    );
    let mut request = VerificationRequest {
        claim,
        evidence: vec![evidence],
        entity_bindings: BTreeMap::new(),
        metadata: BTreeMap::new(),
    };
    let backend = MockVerifier {
        responses: BTreeMap::from([(
            request.claim.id.clone(),
            Ok(r#"{"status":"supported","rationale":"Explicit year in original text"}"#.into()),
        )]),
    };
    let store = MetaStore::open_read_only(dir.path(), tenant).unwrap();
    let resolver = OriginalEvidenceResolver(&store);
    let assessment = verify(&request, &resolver, &backend, 100_000).await;
    assert_eq!(assessment.result.status, SupportStatus::Supported);
    assert_eq!(assessment.sources[0].passage.text, text);
    assert_eq!(assessment.sources[0].passage.document_id, "original.txt");
    let gate = kwaai_provenance::gate::Gate::new(kwaai_provenance::gate::GateMode::Observe);
    assert_eq!(
        gate.decide(&assessment.result).applied,
        kwaai_provenance::gate::Decision::Allow
    );
    request.evidence[0] = EvidenceRef::new(
        EvidenceKind::Original,
        format!("kb:{}", Uuid::new_v4()),
        source_hash(text),
        Some(42),
        vec![],
    );
    assert_eq!(
        verify(&request, &resolver, &backend, 100_000).await.failure,
        Some(VerificationFailure::MissingEvidence)
    );
    assert!(store
        .put_chunks(std::slice::from_ref(&chunk), &[99])
        .is_err());
    drop(store);
    assert_eq!(std::fs::read(&path).unwrap(), before);
    let writable = MetaStore::open(dir.path(), tenant).unwrap();
    let mut altered = chunk;
    altered.text = "Alice was born in 1902.".into();
    writable.put_chunks(&[altered], &[42]).unwrap();
    drop(writable);
    request.evidence[0] = EvidenceRef::new(
        EvidenceKind::Original,
        format!("kb:{tenant}"),
        source_hash(text),
        Some(42),
        vec![],
    );
    let store = MetaStore::open_read_only(dir.path(), tenant).unwrap();
    assert_eq!(
        verify(
            &request,
            &OriginalEvidenceResolver(&store),
            &backend,
            100_000
        )
        .await
        .failure,
        Some(VerificationFailure::HashMismatch)
    );
}

#[test]
fn read_only_open_never_creates_a_missing_store() {
    let dir = tempfile::tempdir().unwrap();
    assert!(MetaStore::open_read_only(dir.path(), Uuid::new_v4()).is_err());
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
}

#[cfg(feature = "verification")]
#[test]
fn cli_bounds_claims_records_failures_and_refuses_overwrite() {
    let dir = tempfile::tempdir().unwrap();
    let tenant = Uuid::new_v4();
    let store = MetaStore::open(dir.path(), tenant).unwrap();
    let text = "Alice was born in 1901.";
    let chunk = ChunkMeta {
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
    };
    store.put_chunks(&[chunk], &[42]).unwrap();
    drop(store);
    let request = VerificationRequest {
        claim: Claim::new(ClaimKind::EntityField, "Alice", "born", "1901"),
        evidence: vec![EvidenceRef::new(
            EvidenceKind::Original,
            format!("kb:{tenant}"),
            source_hash(text),
            Some(42),
            vec![],
        )],
        entity_bindings: BTreeMap::new(),
        metadata: BTreeMap::from([("lineage_digest".into(), "fixture-digest".into())]),
    };
    let mut missing = request.clone();
    missing.evidence.clear();
    let manifest = dir.path().join("requests.jsonl");
    std::fs::write(
        &manifest,
        format!(
            "{}\n{}\n{}\n",
            serde_json::to_string(&request).unwrap(),
            serde_json::to_string(&missing).unwrap(),
            serde_json::to_string(&request).unwrap()
        ),
    )
    .unwrap();
    let config = dir.path().join("ollama.json");
    // No listener: exercise explicit unavailable-backend outcomes, no model needed.
    std::fs::write(&config,r#"{"endpoint":"http://127.0.0.1:1","model":"absent","timeout_secs":1,"seed":0,"num_ctx":8192,"num_predict":512}"#).unwrap();
    let output = dir.path().join("results.jsonl");
    let run = || {
        std::process::Command::new(env!("CARGO_BIN_EXE_provenance-verify"))
            .arg(&manifest)
            .arg(dir.path())
            .arg(tenant.to_string())
            .arg("fixture-corpus-v1")
            .arg(&output)
            .arg("2")
            .arg(&config)
            .output()
            .unwrap()
    };
    let executed = run();
    assert!(
        executed.status.success(),
        "{}",
        String::from_utf8_lossy(&executed.stderr)
    );
    let results = std::fs::read_to_string(&output).unwrap();
    let rows: Vec<serde_json::Value> = results
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    assert_eq!(rows.len(), 4);
    assert!(rows[0]["git_commit"].is_string());
    assert!(rows[0]["timestamp"].is_string());
    assert_eq!(rows[0]["corpus_identity"], "fixture-corpus-v1");
    assert!(rows[0]["startup_error"].is_string());
    assert_eq!(rows[1]["assessment"]["failure"], "inference");
    assert_eq!(
        rows[1]["assessment"]["sources"][0]["retrieved_hash"],
        source_hash(text)
    );
    assert_eq!(rows[2]["assessment"]["failure"], "missing_evidence");
    assert_eq!(rows[3]["claims_evaluated"], 2);
    assert_eq!(rows[3]["unresolved"], 2);
    assert_eq!(rows[3]["missing_evidence"], 1);
    assert_eq!(rows[3]["verification_failures"], 2);
    assert!(!run().status.success());
    assert_eq!(std::fs::read_to_string(output).unwrap(), results);
}
