use kwaai_provenance::verification::EvidenceResolver;
use kwaai_provenance_rag::history::*;
use kwaai_rag::dream::{CycleEnd, DreamObserver, DreamStage};
use kwaai_rag::graph::{EntityNode, FieldValue, GraphStore};
use kwaai_rag::meta_store::{ChunkMeta, MetaStore};
use std::collections::HashMap;
use uuid::Uuid;
fn node(id: i64, value: &str, ev: &[i64]) -> EntityNode {
    EntityNode {
        id,
        name: format!("person {id}"),
        entity_type: "Person".into(),
        description: "fixture".into(),
        embedding: vec![],
        mention_count: 1,
        first_chunk_id: 1,
        aliases: vec![],
        schema_type: None,
        gender: None,
        evidence: vec![],
        fields: HashMap::from([(
            "born".into(),
            FieldValue {
                value: value.into(),
                evidence_chunk_ids: ev.into(),
                confidence: 1.0,
            },
        )]),
        confidence: 1.0,
        extraction_confidence: 1.0,
    }
}
fn chunk(text: &str, document: &str) -> ChunkMeta {
    ChunkMeta {
        doc_name: document.into(),
        chunk_index: 0,
        text: text.into(),
        surrounding: String::new(),
        page_num: None,
        ingested_at: "fixture".into(),
        section_name: None,
        skip_extraction: false,
        section_note: None,
        section_type: Default::default(),
    }
}
struct Fixture {
    data: tempfile::TempDir,
    archive: tempfile::TempDir,
    run: Run,
    graph: GraphStore,
    meta: MetaStore,
}
impl Fixture {
    fn new() -> Self {
        let data = tempfile::tempdir().unwrap();
        let archive = tempfile::tempdir().unwrap();
        let run = Run::new(Uuid::new_v4(), Some("fixture-v1".into()));
        let meta = MetaStore::open(data.path(), run.tenant_id).unwrap();
        meta.put_chunks(
            &[
                chunk("A\r\nexact passage.\0", "a.txt"),
                chunk("Other evidence.", "b.txt"),
            ],
            &[1, 2],
        )
        .unwrap();
        let mut graph = GraphStore::open(data.path(), run.tenant_id).unwrap();
        graph.upsert_entity(node(1, "1901", &[1])).unwrap();
        graph.upsert_entity(node(2, "1902", &[2])).unwrap();
        Self {
            data,
            archive,
            run,
            graph,
            meta,
        }
    }
    fn observer(&self) -> HistoricalObserver {
        HistoricalObserver::open(self.archive.path(), self.data.path(), self.run.clone()).unwrap()
    }
    fn read(&self) -> Vec<Capture> {
        read_archive(self.archive.path()).unwrap()
    }
}
#[test]
fn persisted_creation_deletion_fields_relations_evidence_and_names() {
    let mut f = Fixture::new();
    let mut o = f.observer();
    o.cycle_started(&f.graph, &f.meta);
    f.graph
        .upsert_entity(node(1, "1901", &[1, 2, 999]))
        .unwrap();
    f.graph.upsert_relation(1, 2, "knows", 2).unwrap();
    o.stage_finished(DreamStage::Completion, &f.graph);
    let states = f.read();
    let old = states[0]
        .states
        .iter()
        .find(|s| s.claim.subject == "entity:1")
        .unwrap();
    let added = states[1]
        .states
        .iter()
        .find(|s| s.claim.id == old.claim.id)
        .unwrap();
    assert_eq!(added.evidence.len(), 3);
    assert_eq!(added.evidence[2].retrieval, "missing");
    assert_ne!(old.occurrence_id, added.occurrence_id);
    assert_eq!(states[1].states.len(), 3);
    let mut changed = node(1, "1903", &[2]);
    changed.name = "renamed person".into();
    f.graph.delete_entity(1).unwrap();
    f.graph.upsert_entity(changed).unwrap();
    f.graph.delete_relation(1, 2, "knows").unwrap();
    o.stage_finished(DreamStage::Merge { pairs: &[] }, &f.graph);
    let states = f.read();
    assert_eq!(states[2].states.len(), 2);
    assert!(!states[2].states.iter().any(|s| s.claim.id == old.claim.id));
    let new = states[2]
        .states
        .iter()
        .find(|s| s.claim.subject == "entity:1")
        .unwrap();
    assert_eq!(new.claim.object, "1903");
    assert_eq!(new.bindings["entity:1"], "renamed person");
    f.graph.delete_entity(2).unwrap();
    o.stage_finished(DreamStage::Prune, &f.graph);
    o.stage_finished(DreamStage::Sanitize, &f.graph);
    o.cycle_finished(&f.graph, CycleEnd::Completed);
    assert!(o.failure().is_none(), "{:?}", o.failure());
    let all = f.read();
    assert_eq!(all.len(), 6);
    assert_eq!(all[3].states.len(), 1);
    assert_eq!(all[4].same_state_as, Some(4));
    assert_eq!(all[5].same_state_as, Some(5));
    assert_ne!(
        all[4].states[0].occurrence_id,
        all[5].states[0].occurrence_id
    );
}
#[test]
fn export_originals_survive_live_changes() {
    let f = Fixture::new();
    let mut o = f.observer();
    o.cycle_started(&f.graph, &f.meta);
    o.cycle_finished(&f.graph, CycleEnd::NothingToDo);
    let exports = export(f.archive.path()).unwrap();
    assert_eq!(exports.len(), 4);
    let resolver = ArchivedResolver::open(f.archive.path()).unwrap();
    f.meta
        .put_chunks(&[chunk("REPLACED", "a.txt")], &[1])
        .unwrap();
    let e = exports
        .iter()
        .find(|e| e.request.evidence[0].chunk_id == Some(1))
        .unwrap();
    let p = resolver.resolve(&e.request.evidence[0]).unwrap().unwrap();
    assert_eq!(p.text.as_bytes(), b"A\r\nexact passage.\0");
    assert_eq!(p.document_id, "a.txt");
    assert_eq!(
        e.request.metadata["execution_identity_authenticated"],
        "false"
    );
}
#[test]
fn skipped_resume_and_duplicate_detection() {
    let f = Fixture::new();
    let mut o = f.observer();
    o.cycle_started(&f.graph, &f.meta);
    o.stage_finished(DreamStage::Completion, &f.graph);
    o.cycle_finished(&f.graph, CycleEnd::SkippedDestructiveStages);
    assert_eq!(f.read().len(), 3);
    assert!(HistoricalObserver::open(f.archive.path(), f.data.path(), f.run.clone()).is_err());
    drop(o);
    let mut o = f.observer();
    o.cycle_started(&f.graph, &f.meta);
    assert_eq!(f.read().last().unwrap().cycle, 2);
    drop(o);
    let o = f.observer();
    let records = f.read();
    assert!(!records.last().unwrap().complete);
    assert!(records
        .last()
        .unwrap()
        .error
        .as_ref()
        .unwrap()
        .contains("no observed finish"));
    drop(o);
    let mut other = f.run.clone();
    other.corpus = None;
    assert!(HistoricalObserver::open(f.archive.path(), f.data.path(), other).is_err());
}
#[test]
fn failure_explicit_and_graph_unchanged() {
    let f = Fixture::new();
    let missing = tempfile::tempdir().unwrap();
    let before = kwaai_provenance_rag::facts::snapshot(&f.graph);
    let mut o = HistoricalObserver::open(f.archive.path(), missing.path(), f.run.clone()).unwrap();
    o.cycle_started(&f.graph, &f.meta);
    o.cycle_finished(&f.graph, CycleEnd::NothingToDo);
    assert!(o.failure().is_some());
    let records = f.read();
    assert_eq!(records.len(), 2);
    assert!(records
        .iter()
        .all(|r| !r.complete && r.error.is_some() && r.states.is_empty()));
    assert_eq!(before, kwaai_provenance_rag::facts::snapshot(&f.graph));
    let report = completeness_report(f.archive.path(), &plan("NothingToDo")).unwrap();
    assert!(report.archive_structurally_valid && report.lifecycle_valid);
    assert_eq!(report.incomplete_captures, [1, 2]);
    assert!(report.cycles[0].lifecycle_complete);
    assert!(!report.cycles[0].captures_complete);
    assert!(!report.eligible_for_factual_support_trajectory_analysis);
}
#[test]
fn corruption_truncation_and_duplicate_records_refuse_resume() {
    for mode in 0..4 {
        let f = Fixture::new();
        let mut o = f.observer();
        o.cycle_started(&f.graph, &f.meta);
        drop(o);
        let path = f.archive.path().join("records/00000000000000000001.json");
        match mode {
            0 => {
                let b = std::fs::read(&path).unwrap();
                std::fs::write(&path, &b[..b.len() / 2]).unwrap();
            }
            1 => {
                let mut b: serde_json::Value =
                    serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
                b["capture"]["cycle"] = 99.into();
                std::fs::write(&path, serde_json::to_vec(&b).unwrap()).unwrap();
            }
            2 => {
                let c = f.read();
                let blob = c[0].states[0].evidence[0].blob.as_ref().unwrap();
                std::fs::write(f.archive.path().join(blob), b"corrupt").unwrap();
            }
            _ => {
                std::fs::copy(
                    &path,
                    f.archive.path().join("records/00000000000000000002.json"),
                )
                .unwrap();
            }
        }
        assert!(read_archive(f.archive.path()).is_err());
        assert!(HistoricalObserver::open(f.archive.path(), f.data.path(), f.run.clone()).is_err());
    }
}
#[test]
fn deterministic_ids_bind_content_bindings_evidence_run_boundary() {
    let f = Fixture::new();
    let mut o = f.observer();
    o.cycle_started(&f.graph, &f.meta);
    let c = &f.read()[0];
    let s = &c.states[0];
    let id = occurrence_id(c, s).unwrap();
    assert_eq!(id, occurrence_id(c, s).unwrap());
    for mode in 0..9 {
        let mut c = c.clone();
        let mut s = s.clone();
        match mode {
            0 => c.run.run_id = Uuid::nil(),
            1 => c.cycle += 1,
            2 => c.stage = "merge".into(),
            3 => c.boundary = "cycle_finish".into(),
            4 => c.sequence += 1,
            5 => s.claim.object.push(' '),
            6 => {
                s.bindings.insert("entity:1".into(), "different".into());
            }
            7 => s.evidence.clear(),
            _ => s.claim.predicate.push('x'),
        }
        assert_ne!(id, occurrence_id(&c, &s).unwrap());
    }
}
#[tokio::test]
async fn real_empty_cycle_matches_uninstrumented() {
    use kwaai_rag::{
        dream::{run_dream_cycle, run_dream_cycle_observed, DreamConfig},
        embedder::EmbedClient,
    };
    let data = tempfile::tempdir().unwrap();
    let plain = tempfile::tempdir().unwrap();
    let archive = tempfile::tempdir().unwrap();
    let run = Run::new(Uuid::new_v4(), None);
    let mut o = HistoricalObserver::open(archive.path(), data.path(), run.clone()).unwrap();
    let embed = EmbedClient::new(Some("http://127.0.0.1:9".into()), None);
    let cfg = DreamConfig::default();
    let start = std::time::Instant::now();
    let a = run_dream_cycle_observed(
        data.path(),
        run.tenant_id,
        &embed,
        &cfg,
        &[],
        "unused",
        None::<fn(usize, usize, &str)>,
        &mut o,
    )
    .await
    .unwrap();
    let enabled = start.elapsed();
    let start = std::time::Instant::now();
    let b = run_dream_cycle(
        plain.path(),
        run.tenant_id,
        &embed,
        &cfg,
        &[],
        "unused",
        None::<fn(usize, usize, &str)>,
    )
    .await
    .unwrap();
    eprintln!(
        "empty cycle enabled={enabled:?} disabled={:?}",
        start.elapsed()
    );
    assert_eq!(a.score_before, b.score_before);
    assert_eq!(a.score_after, b.score_after);
    assert_eq!(a.cycle_errors, b.cycle_errors);
    let records = read_archive(archive.path()).unwrap();
    assert_eq!(records.len(), 2);
    assert_eq!(records[1].same_state_as, Some(1));
}

#[test]
fn evidence_removal_and_relation_replacement_use_complete_sets() {
    let mut f = Fixture::new();
    f.graph.upsert_entity(node(1, "1901", &[1, 2])).unwrap();
    f.graph.upsert_relation(1, 2, "knows", 1).unwrap();
    let mut o = f.observer();
    o.cycle_started(&f.graph, &f.meta);
    f.graph.delete_entity(1).unwrap();
    f.graph.upsert_entity(node(1, "1901", &[2])).unwrap();
    f.graph.delete_relation(1, 2, "knows").unwrap();
    f.graph.upsert_relation(1, 2, "parent_of", 2).unwrap();
    o.stage_finished(DreamStage::Completion, &f.graph);
    let records = f.read();
    let old = records[0]
        .states
        .iter()
        .find(|s| s.claim.subject == "entity:1" && s.claim.predicate == "born")
        .unwrap();
    let new = records[1]
        .states
        .iter()
        .find(|s| s.claim.id == old.claim.id)
        .unwrap();
    assert_eq!(old.evidence.len(), 2);
    assert_eq!(new.evidence.len(), 1);
    assert_eq!(new.evidence[0].reference.chunk_id, Some(2));
    assert_ne!(old.occurrence_id, new.occurrence_id);
    let old_relation = records[0]
        .states
        .iter()
        .find(|s| s.claim.kind == kwaai_provenance::ClaimKind::Relation)
        .unwrap();
    assert!(!records[1]
        .states
        .iter()
        .any(|s| s.claim.id == old_relation.claim.id));
    let exported = export(f.archive.path()).unwrap();
    let new_export = exported
        .iter()
        .find(|e| e.occurrence_id == new.occurrence_id)
        .unwrap();
    let resolver =
        ArchivedResolver::for_state(f.archive.path(), &new_export.occurrence_id).unwrap();
    assert!(resolver
        .resolve(&old.evidence[0].reference)
        .unwrap()
        .is_none());
}

struct ScriptedBackend;
#[async_trait::async_trait]
impl kwaai_provenance::verification::EvidenceBackend for ScriptedBackend {
    fn identity(&self) -> String {
        "scripted historical fixture".into()
    }
    fn configuration(&self) -> serde_json::Value {
        serde_json::json!({"fixture":true})
    }
    async fn infer(
        &self,
        request: &kwaai_provenance::verification::VerificationRequest,
        sources: &[kwaai_provenance::verification::ResolvedEvidence],
    ) -> Result<String, String> {
        assert_eq!(request.evidence.len(), sources.len());
        assert!(sources.iter().all(|s| s.passage.text != "REPLACED"));
        Ok(r#"{"status":"supported","rationale":"scripted fixture only"}"#.into())
    }
}
#[tokio::test]
async fn historical_export_runs_actual_phase4_verifier_and_binds_assessment() {
    let f = Fixture::new();
    let mut o = f.observer();
    o.cycle_started(&f.graph, &f.meta);
    let items = export(f.archive.path()).unwrap();
    let e = &items[0];
    let result = verify_state(f.archive.path(), e, &ScriptedBackend, 100_000)
        .await
        .unwrap();
    assert_eq!(result.occurrence_id, e.occurrence_id);
    assert_eq!(result.request_sha256, request_hash(&e.request).unwrap());
    assert_eq!(
        result.assessment.result.status,
        kwaai_provenance::SupportStatus::Supported
    );
    assert_eq!(result.assessment.result.claim_id, e.request.claim.id);
    assert_eq!(result.assessment.sources.len(), 1);
    let mut changed = e.clone();
    changed.request.entity_bindings.clear();
    assert!(
        verify_state(f.archive.path(), &changed, &ScriptedBackend, 100_000)
            .await
            .is_err()
    );
    // Preserved blob tampering after resolver construction is still detected.
    let resolver = ArchivedResolver::for_state(f.archive.path(), &e.occurrence_id).unwrap();
    std::fs::write(
        f.archive
            .path()
            .join(e.capture.states[0].evidence[0].blob.as_ref().unwrap()),
        "tampered",
    )
    .unwrap();
    assert!(resolver.resolve(&e.request.evidence[0]).is_err());
}

#[tokio::test]
async fn missing_original_is_unresolved_and_name_changes_keep_native_id() {
    let mut f = Fixture::new();
    f.graph.upsert_entity(node(1, "1901", &[999])).unwrap();
    let mut o = f.observer();
    o.cycle_started(&f.graph, &f.meta);
    let before = export(f.archive.path())
        .unwrap()
        .into_iter()
        .find(|e| e.request.claim.subject == "entity:1")
        .unwrap();
    let result = verify_state(f.archive.path(), &before, &ScriptedBackend, 100_000)
        .await
        .unwrap();
    assert_eq!(
        result.assessment.result.status,
        kwaai_provenance::SupportStatus::Unresolved
    );
    assert_eq!(
        result.assessment.failure,
        Some(kwaai_provenance::verification::VerificationFailure::MissingEvidence)
    );
    f.graph.delete_entity(1).unwrap();
    let mut renamed = node(1, "1901", &[1, 999]);
    renamed.name = "different name".into();
    f.graph.upsert_entity(renamed).unwrap();
    o.stage_finished(DreamStage::Completion, &f.graph);
    let after = export(f.archive.path())
        .unwrap()
        .into_iter()
        .find(|e| e.capture.sequence == 2 && e.request.claim.subject == "entity:1")
        .unwrap();
    assert_eq!(before.request.claim, after.request.claim);
    assert_eq!(before.request.evidence, after.request.evidence);
    assert_ne!(
        before.request.entity_bindings,
        after.request.entity_bindings
    );
    assert_ne!(before.occurrence_id, after.occurrence_id);
}

// Audit regressions: added before implementation remediation.
#[test]
fn audit_window_context_is_retained() {
    let mut f = Fixture::new();
    f.graph.set_evidence_window(1);
    f.graph.upsert_relation(1, 2, "knows", 1).unwrap();
    let mut o = f.observer();
    o.cycle_started(&f.graph, &f.meta);
    let records = f.read();
    let relation = records[0]
        .states
        .iter()
        .find(|s| s.claim.kind == kwaai_provenance::ClaimKind::Relation)
        .unwrap();
    let value = serde_json::to_value(relation).unwrap();
    assert_eq!(value["extraction_contexts"][0]["evidence_window"], 1);
}
#[test]
fn audit_completed_requires_all_stages() {
    let f = Fixture::new();
    let mut o = f.observer();
    o.cycle_started(&f.graph, &f.meta);
    o.cycle_finished(&f.graph, CycleEnd::Completed);
    assert!(
        o.failure().is_some(),
        "missing stage callbacks must be diagnosed"
    );
    assert!(
        read_archive(f.archive.path()).is_err(),
        "impossible completed lifecycle must be rejected"
    );
}
#[test]
fn audit_store_binding_rejects_other_directory_same_tenant() {
    let f = Fixture::new();
    let other = tempfile::tempdir().unwrap();
    let wrong = MetaStore::open(other.path(), f.run.tenant_id).unwrap();
    wrong
        .put_chunks(&[chunk("WRONG STORE", "a.txt")], &[1])
        .unwrap();
    let mut o = HistoricalObserver::open(f.archive.path(), other.path(), f.run.clone()).unwrap();
    o.cycle_started(&f.graph, &f.meta);
    assert!(
        o.failure().is_some(),
        "matching tenant and chunk IDs do not bind stores"
    );
    assert!(f.read().iter().all(|c| !c.complete && c.states.is_empty()));
}
#[tokio::test]
async fn audit_unrelated_graph_growth_does_not_consume_verification_budget() {
    let mut f = Fixture::new();
    let mut o = f.observer();
    o.cycle_started(&f.graph, &f.meta);
    let before = export(f.archive.path())
        .unwrap()
        .into_iter()
        .find(|e| e.request.claim.subject == "entity:1")
        .unwrap();
    let first = verify_state(f.archive.path(), &before, &ScriptedBackend, 10_000)
        .await
        .unwrap();
    assert!(first.assessment.failure.is_none());
    for id in 10..50 {
        f.graph
            .upsert_entity(node(id, &"x".repeat(500), &[2]))
            .unwrap();
    }
    o.stage_finished(DreamStage::Completion, &f.graph);
    let after = export(f.archive.path())
        .unwrap()
        .into_iter()
        .find(|e| e.capture.sequence == 2 && e.request.claim.subject == "entity:1")
        .unwrap();
    assert_eq!(before.request.claim, after.request.claim);
    assert_eq!(before.request.evidence, after.request.evidence);
    let second = verify_state(f.archive.path(), &after, &ScriptedBackend, 10_000)
        .await
        .unwrap();
    assert!(
        second.assessment.failure.is_none(),
        "unrelated capture metadata exhausted budget: {:?}",
        second.assessment.failure
    );
}

fn plan(end: &str) -> ExperimentExpectation {
    ExperimentExpectation {
        schema_version: 1,
        cycles: vec![CycleExpectation {
            cycle: 1,
            cycle_end: end.into(),
        }],
        expected_archive_head: None,
    }
}
fn complete_cycle(o: &mut HistoricalObserver, g: &GraphStore) {
    o.stage_finished(DreamStage::Completion, g);
    o.stage_finished(DreamStage::Merge { pairs: &[] }, g);
    o.stage_finished(DreamStage::Prune, g);
    o.stage_finished(DreamStage::Sanitize, g);
    o.cycle_finished(g, CycleEnd::Completed);
}
#[test]
fn lifecycle_missing_duplicate_out_of_order_and_contradictory_callbacks_fail_closed() {
    for mode in 0..8 {
        let f = Fixture::new();
        let mut o = f.observer();
        if mode != 6 && mode != 7 {
            o.cycle_started(&f.graph, &f.meta);
        }
        match mode {
            0 => o.stage_finished(DreamStage::Sanitize, &f.graph),
            1 => {
                o.stage_finished(DreamStage::Completion, &f.graph);
                o.stage_finished(DreamStage::Completion, &f.graph);
            }
            2 => {
                o.stage_finished(DreamStage::Completion, &f.graph);
                o.cycle_finished(&f.graph, CycleEnd::NothingToDo);
            }
            3 => o.cycle_finished(&f.graph, CycleEnd::SkippedDestructiveStages),
            4 => o.cycle_started(&f.graph, &f.meta),
            5 => {
                o.cycle_finished(&f.graph, CycleEnd::NothingToDo);
                o.cycle_finished(&f.graph, CycleEnd::NothingToDo);
            }
            6 => o.stage_finished(DreamStage::Completion, &f.graph),
            _ => o.cycle_finished(&f.graph, CycleEnd::Completed),
        }
        assert!(o.failure().is_some(), "mode {mode}");
        assert!(read_archive(f.archive.path()).is_err(), "mode {mode}");
        let report = completeness_report(f.archive.path(), &plan("Completed")).unwrap();
        assert!(
            report.archive_structurally_valid,
            "mode {mode}: {:?}",
            report.structural_error
        );
        assert!(!report.lifecycle_valid);
        assert!(!report.unexpected_or_contradictory_events.is_empty());
        assert!(!report.eligible_for_factual_support_trajectory_analysis);
    }
}
#[test]
fn completeness_distinguishes_prefix_capture_cycle_experiment_and_expected_head() {
    let f = Fixture::new();
    let mut o = f.observer();
    let expected = plan("Completed");
    let empty = completeness_report(f.archive.path(), &expected).unwrap();
    assert!(empty.archive_structurally_valid);
    assert!(!empty.eligible_for_factual_support_trajectory_analysis);
    assert_eq!(empty.cycles[0].missing_boundaries.len(), 6);
    o.cycle_started(&f.graph, &f.meta);
    let prefix = completeness_report(f.archive.path(), &expected).unwrap();
    assert!(prefix.lifecycle_valid);
    assert!(prefix.incomplete_captures.is_empty());
    assert_eq!(
        prefix.cycles[0].missing_boundaries,
        [
            "completion",
            "merge",
            "prune",
            "sanitize",
            "finish:Completed"
        ]
    );
    assert!(!prefix.cycles[0].lifecycle_complete);
    complete_cycle(&mut o, &f.graph);
    let done = completeness_report(f.archive.path(), &expected).unwrap();
    assert!(
        done.eligible_for_factual_support_trajectory_analysis,
        "{done:?}"
    );
    let mut two = expected.clone();
    two.cycles.push(CycleExpectation {
        cycle: 2,
        cycle_end: "NothingToDo".into(),
    });
    assert!(
        !completeness_report(f.archive.path(), &two)
            .unwrap()
            .eligible_for_factual_support_trajectory_analysis
    );
    o.cycle_started(&f.graph, &f.meta);
    o.cycle_finished(&f.graph, CycleEnd::NothingToDo);
    two.expected_archive_head = Some(archive_head(f.archive.path()).unwrap());
    assert!(
        completeness_report(f.archive.path(), &two)
            .unwrap()
            .eligible_for_factual_support_trajectory_analysis
    );
    drop(o);
    // Remove a WHOLE valid trailing cycle; parsing alone cannot detect this loss.
    std::fs::remove_file(f.archive.path().join("records/00000000000000000007.json")).unwrap();
    std::fs::remove_file(f.archive.path().join("records/00000000000000000008.json")).unwrap();
    assert!(read_archive(f.archive.path()).is_ok());
    let report = completeness_report(f.archive.path(), &two).unwrap();
    assert_eq!(report.archive_head_matches, Some(false));
    assert_eq!(report.expected_cycles, [1, 2]);
    assert_eq!(report.observed_cycles, [1]);
    assert!(!report.eligible_for_factual_support_trajectory_analysis);
    // Even a shorter plan must fail when the externally retained head disagrees.
    two.cycles.pop();
    assert!(
        !completeness_report(f.archive.path(), &two)
            .unwrap()
            .eligible_for_factual_support_trajectory_analysis
    );
}
#[test]
fn legitimate_skips_incomplete_capture_missing_original_and_resume_are_reported() {
    let mut no_evidence = Fixture::new();
    no_evidence.graph.delete_entity(1).unwrap();
    no_evidence
        .graph
        .upsert_entity(node(1, "1901", &[]))
        .unwrap();
    let mut no_evidence_observer = no_evidence.observer();
    no_evidence_observer.cycle_started(&no_evidence.graph, &no_evidence.meta);
    no_evidence_observer.cycle_finished(&no_evidence.graph, CycleEnd::NothingToDo);
    let report = completeness_report(no_evidence.archive.path(), &plan("NothingToDo")).unwrap();
    assert!(report
        .missing_original_evidence
        .iter()
        .any(|e| e.reason == "no_evidence_associations"));
    assert!(!report.eligible_for_factual_support_trajectory_analysis);

    for skipped in [false, true] {
        let f = Fixture::new();
        let mut o = f.observer();
        o.cycle_started(&f.graph, &f.meta);
        let end = if skipped {
            o.stage_finished(DreamStage::Completion, &f.graph);
            o.cycle_finished(&f.graph, CycleEnd::SkippedDestructiveStages);
            "SkippedDestructiveStages"
        } else {
            o.cycle_finished(&f.graph, CycleEnd::NothingToDo);
            "NothingToDo"
        };
        assert!(
            completeness_report(f.archive.path(), &plan(end))
                .unwrap()
                .eligible_for_factual_support_trajectory_analysis
        );
    }
    let mut f = Fixture::new();
    f.graph.upsert_entity(node(1, "1901", &[999])).unwrap();
    let mut o = f.observer();
    o.cycle_started(&f.graph, &f.meta);
    o.cycle_finished(&f.graph, CycleEnd::NothingToDo);
    let missing = completeness_report(f.archive.path(), &plan("NothingToDo")).unwrap();
    assert!(missing.cycles[0].lifecycle_complete && missing.incomplete_captures.is_empty());
    assert!(!missing.missing_original_evidence.is_empty());
    assert!(!missing.eligible_for_factual_support_trajectory_analysis);
    drop(o);
    let mut o = f.observer();
    o.cycle_started(&f.graph, &f.meta);
    drop(o);
    let _o = f.observer();
    let resumed = completeness_report(f.archive.path(), &plan("NothingToDo")).unwrap();
    assert!(!resumed.incomplete_captures.is_empty());
    assert!(!resumed.eligible_for_factual_support_trajectory_analysis);
}
#[test]
fn window_metadata_changes_state_content_and_expanded_bytes_survive_without_live_store() {
    let mut f = Fixture::new();
    let mut centre = chunk("CENTRE\r\n", "window.txt");
    centre.chunk_index = 1;
    let mut left = chunk("LEFT\0", "window.txt");
    left.chunk_index = 0;
    let mut right = chunk("RIGHT", "window.txt");
    right.chunk_index = 2;
    let mut excluded = chunk("OTHER ZONE", "window.txt");
    excluded.chunk_index = 3;
    excluded.section_type = kwaai_rag::doc_schema::SectionType::Acknowledgements;
    f.meta
        .put_chunks(&[centre, left, right, excluded], &[20, 99, 7, 31])
        .unwrap();
    f.graph.set_evidence_window(0);
    f.graph.upsert_relation(1, 2, "knows", 20).unwrap();
    let mut o = f.observer();
    o.cycle_started(&f.graph, &f.meta);
    f.graph.delete_relation(1, 2, "knows").unwrap();
    f.graph.set_evidence_window(2);
    f.graph.upsert_relation(1, 2, "knows", 20).unwrap();
    o.stage_finished(DreamStage::Completion, &f.graph);
    let records = f.read();
    let relation = |c: &Capture| {
        c.states
            .iter()
            .find(|s| s.claim.kind == kwaai_provenance::ClaimKind::Relation)
            .unwrap()
            .clone()
    };
    let old = relation(&records[0]);
    let new = relation(&records[1]);
    assert_eq!(old.claim, new.claim);
    assert_eq!(old.evidence, new.evidence);
    assert_eq!(old.extraction_contexts[0].evidence_window, 0);
    assert_eq!(new.extraction_contexts[0].evidence_window, 2);
    assert_eq!(new.extraction_contexts[0].direct_chunk_ids, [20]);
    assert_eq!(
        new.extraction_contexts[0]
            .expanded
            .iter()
            .map(|e| e.reference.chunk_id.unwrap())
            .collect::<Vec<_>>(),
        [7, 20, 99]
    );
    // Hold every occurrence coordinate equal: only context changes this ID.
    assert_ne!(
        occurrence_id(&records[0], &old).unwrap(),
        occurrence_id(&records[0], &new).unwrap()
    );
    assert!(records[1].same_state_as.is_none());
    let resolver = ArchivedResolver::for_state(f.archive.path(), &new.occurrence_id).unwrap();
    f.meta.delete_doc("window.txt").unwrap();
    for (id, text) in [
        (7, b"RIGHT".as_slice()),
        (20, b"CENTRE\r\n".as_slice()),
        (99, b"LEFT\0".as_slice()),
    ] {
        let ev = new.extraction_contexts[0]
            .expanded
            .iter()
            .find(|e| e.reference.chunk_id == Some(id))
            .unwrap();
        let source = resolver.resolve(&ev.reference).unwrap().unwrap();
        assert_eq!(source.text.as_bytes(), text);
        assert_eq!(source.document_id, "window.txt");
    }
    let exported = export(f.archive.path())
        .unwrap()
        .into_iter()
        .find(|e| e.occurrence_id == new.occurrence_id)
        .unwrap();
    assert_eq!(exported.request.evidence.len(), 3);
    let blob = &new.extraction_contexts[0]
        .expanded
        .iter()
        .find(|e| e.reference.chunk_id == Some(99))
        .unwrap()
        .blob;
    std::fs::remove_file(f.archive.path().join(blob.as_ref().unwrap())).unwrap();
    assert!(
        read_archive(f.archive.path()).is_err(),
        "expanded-only missing blob must invalidate archive"
    );
}
#[tokio::test]
async fn missing_window_positions_and_centres_are_explicit_and_never_verified_as_complete() {
    let mut f = Fixture::new();
    let mut centre = chunk("CENTRE", "gap.txt");
    centre.chunk_index = 1;
    let mut far = chunk("AFTER GAP", "gap.txt");
    far.chunk_index = 3;
    f.meta.put_chunks(&[centre, far], &[10, 12]).unwrap();
    f.graph.set_evidence_window(1);
    f.graph.upsert_relation(1, 2, "knows", 10).unwrap();
    f.graph.upsert_relation(1, 2, "knows", 999).unwrap();
    let mut o = f.observer();
    o.cycle_started(&f.graph, &f.meta);
    let e = export(f.archive.path())
        .unwrap()
        .into_iter()
        .find(|e| e.request.claim.kind == kwaai_provenance::ClaimKind::Relation)
        .unwrap();
    let context = &e
        .capture
        .states
        .iter()
        .find(|s| s.occurrence_id == e.occurrence_id)
        .unwrap()
        .extraction_contexts[0];
    assert_eq!(context.unresolved_centres, [999]);
    assert_eq!(
        context
            .missing_positions
            .iter()
            .map(|p| p.chunk_index)
            .collect::<Vec<_>>(),
        [0, 2]
    );
    let result = verify_state(f.archive.path(), &e, &ScriptedBackend, 100_000)
        .await
        .unwrap();
    assert_eq!(
        result.assessment.failure,
        Some(kwaai_provenance::verification::VerificationFailure::MissingEvidence)
    );
    assert!(e
        .request
        .evidence
        .iter()
        .any(|r| r.kind == kwaai_provenance::EvidenceKind::Derived && r.chunk_id.is_none()));
    let resolver = ArchivedResolver::for_state(f.archive.path(), &e.occurrence_id).unwrap();
    let raw =
        kwaai_provenance::verification::verify(&e.request, &resolver, &ScriptedBackend, 100_000)
            .await;
    assert_eq!(
        raw.failure,
        Some(kwaai_provenance::verification::VerificationFailure::MissingEvidence)
    );
    validate_assessment(&e, &result).unwrap();
    o.cycle_finished(&f.graph, CycleEnd::NothingToDo);
    let report = completeness_report(f.archive.path(), &plan("NothingToDo")).unwrap();
    assert!(report
        .missing_original_evidence
        .iter()
        .any(|e| e.chunk_index == Some(2)));
    assert!(!report.eligible_for_factual_support_trajectory_analysis);
}
#[test]
fn callback_meta_and_graph_tenant_mismatch_and_midcycle_store_switch_are_incomplete() {
    for mode in 0..3 {
        let f = Fixture::new();
        let other = tempfile::tempdir().unwrap();
        let wrong_meta = MetaStore::open(other.path(), f.run.tenant_id).unwrap();
        wrong_meta
            .put_chunks(&[chunk("WRONG", "a.txt")], &[1])
            .unwrap();
        let wrong_graph = GraphStore::open(other.path(), f.run.tenant_id).unwrap();
        let mut o = f.observer();
        match mode {
            0 => o.cycle_started(&f.graph, &wrong_meta),
            1 => o.cycle_started(&wrong_graph, &f.meta),
            _ => {
                o.cycle_started(&f.graph, &f.meta);
                o.stage_finished(DreamStage::Completion, &wrong_graph);
            }
        }
        assert!(o.failure().is_some());
        assert!(!f.read().last().unwrap().complete);
    }
}

#[tokio::test]
async fn populated_scripted_cycle_matches_capture_disabled_without_model_inference() {
    use kwaai_rag::{
        dream::{run_dream_cycle, run_dream_cycle_observed, DreamConfig},
        embedder::EmbedClient,
    };
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let enabled = tempfile::tempdir().unwrap();
    let disabled = tempfile::tempdir().unwrap();
    let archive = tempfile::tempdir().unwrap();
    let run = Run::new(Uuid::new_v4(), Some("populated-scripted-fixture".into()));
    for dir in [enabled.path(), disabled.path()] {
        let meta = MetaStore::open(dir, run.tenant_id).unwrap();
        meta.put_chunks(
            &[chunk(
                "person 1 is associated with person 2.",
                "fixture.txt",
            )],
            &[1],
        )
        .unwrap();
        let mut graph = GraphStore::open(dir, run.tenant_id).unwrap();
        let mut first = node(1, "1901", &[1]);
        first.entity_type = "Unknown".into();
        first.evidence = vec![1];
        let mut second = node(2, "1902", &[1]);
        second.schema_type = Some("schema:Person".into());
        second.description = "A well documented second person. ".repeat(10);
        second.mention_count = 10;
        graph.upsert_entity(first).unwrap();
        graph.upsert_entity(second).unwrap();
        graph.link_chunk(1, &[1, 2]).unwrap();
    }
    // Fixed HTTP fixture, no model server or embedding inference is invoked.
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let server = tokio::spawn(async move {
        for _ in 0..2 {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut headers = Vec::new();
            loop {
                let mut b = [0];
                socket.read_exact(&mut b).await.unwrap();
                headers.push(b[0]);
                if headers.ends_with(b"\r\n\r\n") {
                    break;
                }
            }
            let header = String::from_utf8(headers).unwrap();
            assert!(header.starts_with("POST /v1/chat/completions"));
            let length: usize = header
                .lines()
                .find_map(|line| {
                    line.to_ascii_lowercase()
                        .strip_prefix("content-length:")
                        .map(|n| n.trim().parse().unwrap())
                })
                .unwrap();
            let mut body = vec![0; length];
            socket.read_exact(&mut body).await.unwrap();
            let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
            assert!(body["messages"][0]["content"]
                .as_str()
                .unwrap()
                .contains("person 1"));
            let content = serde_json::json!({"schema_type":"schema:Person", "description":"", "relations":[{"type":"associated_with", "target":"person 2"}]}).to_string();
            let response =
                serde_json::json!({"choices":[{"message":{"content":content}}]}).to_string();
            socket.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}", response.len(), response).as_bytes()).await.unwrap();
        }
    });
    let cfg = DreamConfig {
        completeness_threshold: 1.1,
        max_completions_per_cycle: 1,
        workers: 1,
        dedup_threshold: 1.1,
        prune_threshold: 0.0,
        ..DreamConfig::default()
    };
    let embed = EmbedClient::new(Some("http://127.0.0.1:9".into()), None);
    let mut o = HistoricalObserver::open(archive.path(), enabled.path(), run.clone()).unwrap();
    let a = run_dream_cycle_observed(
        enabled.path(),
        run.tenant_id,
        &embed,
        &cfg,
        std::slice::from_ref(&url),
        "scripted-fixture",
        None::<fn(usize, usize, &str)>,
        &mut o,
    )
    .await
    .unwrap();
    let b = run_dream_cycle(
        disabled.path(),
        run.tenant_id,
        &embed,
        &cfg,
        &[url],
        "scripted-fixture",
        None::<fn(usize, usize, &str)>,
    )
    .await
    .unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(5), server)
        .await
        .unwrap()
        .unwrap();
    assert!(o.failure().is_none(), "{:?}", o.failure());
    assert_eq!(a.entities_type_completed, 1);
    assert_eq!(a.entities_relations_added, 1);
    let mut av = serde_json::to_value(&a).unwrap();
    let mut bv = serde_json::to_value(&b).unwrap();
    for value in [&mut av, &mut bv] {
        value.as_object_mut().unwrap().remove("timestamp");
        value.as_object_mut().unwrap().remove("duration_secs");
    }
    assert_eq!(av, bv);
    let ga = GraphStore::open(enabled.path(), run.tenant_id).unwrap();
    let gb = GraphStore::open(disabled.path(), run.tenant_id).unwrap();
    assert_eq!(
        serde_json::to_value(ga.historical_records().unwrap()).unwrap(),
        serde_json::to_value(gb.historical_records().unwrap()).unwrap()
    );
    let captures = read_archive(archive.path()).unwrap();
    assert_eq!(captures.len(), 6);
    assert!(!captures[0]
        .states
        .iter()
        .any(|s| s.claim.kind == kwaai_provenance::ClaimKind::Relation));
    assert!(captures[1]
        .states
        .iter()
        .any(|s| s.claim.kind == kwaai_provenance::ClaimKind::Relation));
    assert!(
        completeness_report(archive.path(), &plan("Completed"))
            .unwrap()
            .eligible_for_factual_support_trajectory_analysis
    );
}

#[tokio::test]
async fn assessment_sources_are_checked_against_retained_historical_bytes_and_document() {
    let f = Fixture::new();
    let mut o = f.observer();
    o.cycle_started(&f.graph, &f.meta);
    let e = export(f.archive.path()).unwrap().remove(0);
    let result = verify_state(f.archive.path(), &e, &ScriptedBackend, 10_000)
        .await
        .unwrap();
    validate_assessment(&e, &result).unwrap();
    for mode in 0..6 {
        let mut bad = result.clone();
        match mode {
            0 => bad.assessment.sources[0].passage.text.push('x'),
            1 => bad.assessment.sources[0].passage.document_id = "wrong document".into(),
            2 => bad.assessment.sources[0].retrieved_hash = "wrong hash".into(),
            3 => bad.assessment.sources[0].reference.chunk_id = Some(999),
            4 => bad.assessment.sources.clear(),
            _ => bad
                .assessment
                .sources
                .push(bad.assessment.sources[0].clone()),
        }
        assert!(validate_assessment(&e, &bad).is_err(), "mode {mode}");
    }
}
#[test]
fn replaced_open_store_file_identity_is_rejected() {
    for graph in [false, true] {
        let f = Fixture::new();
        let mut o = f.observer();
        o.cycle_started(&f.graph, &f.meta);
        let name = if graph {
            format!("graph-{}.db", f.run.tenant_id)
        } else {
            format!("{}.db", f.run.tenant_id)
        };
        let path = f.data.path().join(name);
        std::fs::rename(&path, path.with_extension("retired")).unwrap();
        std::fs::write(&path, b"replacement, not the opened database").unwrap();
        o.stage_finished(DreamStage::Completion, &f.graph);
        assert!(o.failure().is_some());
        assert!(!f.read()[1].complete);
    }
}
#[test]
fn completeness_cli_emits_json_and_refuses_ineligible_prefix() {
    let f = Fixture::new();
    let mut o = f.observer();
    let expectation = f.archive.path().join("expectation.json");
    std::fs::write(
        &expectation,
        serde_json::to_vec(&plan("NothingToDo")).unwrap(),
    )
    .unwrap();
    o.cycle_started(&f.graph, &f.meta);
    let invoke = || {
        std::process::Command::new(env!("CARGO_BIN_EXE_provenance-history"))
            .arg("completeness")
            .arg(f.archive.path())
            .arg(&expectation)
            .output()
            .unwrap()
    };
    let prefix = invoke();
    assert!(!prefix.status.success());
    let report: CompletenessReport = serde_json::from_slice(&prefix.stdout).unwrap();
    assert!(
        report.archive_structurally_valid
            && !report.eligible_for_factual_support_trajectory_analysis
    );
    o.cycle_finished(&f.graph, CycleEnd::NothingToDo);
    let done = invoke();
    assert!(
        done.status.success(),
        "{}",
        String::from_utf8_lossy(&done.stderr)
    );
    assert!(
        serde_json::from_slice::<CompletenessReport>(&done.stdout)
            .unwrap()
            .eligible_for_factual_support_trajectory_analysis
    );
}
