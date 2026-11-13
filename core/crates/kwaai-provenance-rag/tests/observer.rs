//! Integration tests on a real temporary knowledge base.
//!
//! The dream cycle's LLM steps cannot run offline, so these tests drive the same
//! graph operations the cycle performs (relation upserts, merge, delete,
//! sanitize) and call the observer hooks at the same points `run_dream_cycle`
//! does.

use kwaai_provenance::persist::{self, TailPolicy};
use kwaai_provenance_rag::facts::{self, FactKey};
use kwaai_provenance_rag::{CycleSummary, ProvenanceObserver, LINEAGE_FILE};
use kwaai_rag::dream::{CycleEnd, DreamObserver, DreamStage};
use kwaai_rag::graph::{entity_id, EntityNode, FieldValue, GraphStore};
use kwaai_rag::meta_store::{ChunkMeta, MetaStore};
use std::collections::HashMap;
use std::path::Path;
use uuid::Uuid;

const ALICE: (&str, &str) = ("Alice Smith", "Person");
const ALICE_ALIAS: (&str, &str) = ("Mrs Alice Smith", "Person");
const ACME: (&str, &str) = ("Acme", "Organization");
const PARIS: (&str, &str) = ("Paris", "Place");
const BOB: (&str, &str) = ("Bob", "Person");

fn id(e: (&str, &str)) -> i64 {
    entity_id(e.0, e.1)
}

fn node(e: (&str, &str), fields: &[(&str, &str, &[i64])]) -> EntityNode {
    EntityNode {
        id: id(e),
        name: e.0.to_string(),
        entity_type: e.1.to_string(),
        description: format!("{} test entity", e.0),
        embedding: vec![0.1, 0.2, 0.3],
        mention_count: 2,
        first_chunk_id: 1,
        aliases: vec![],
        schema_type: None,
        gender: None,
        evidence: vec![],
        fields: fields
            .iter()
            .map(|(k, v, ev)| {
                (
                    k.to_string(),
                    FieldValue {
                        value: v.to_string(),
                        evidence_chunk_ids: ev.to_vec(),
                        confidence: 0.5,
                    },
                )
            })
            .collect::<HashMap<_, _>>(),
        confidence: 0.5,
        extraction_confidence: 1.0,
    }
}

fn chunk(i: u32, text: &str) -> ChunkMeta {
    ChunkMeta {
        doc_name: "doc.txt".into(),
        chunk_index: i,
        text: text.into(),
        surrounding: String::new(),
        page_num: None,
        ingested_at: "2026-11-12T00:00:00Z".into(),
        section_name: None,
        skip_extraction: false,
        section_note: None,
        section_type: Default::default(),
    }
}

/// Chunks 1–3 exist. Facts: Alice works_at Acme [1]; alias lived_in Paris [2];
/// Alice born=1901 [3].
fn build_kb(dir: &Path, tenant: Uuid) {
    let meta = MetaStore::open(dir, tenant).unwrap();
    meta.put_chunks(
        &[
            chunk(0, "Alice works at Acme."),
            chunk(1, "Mrs Alice Smith lived in Paris."),
            chunk(2, "Alice was born in 1901."),
        ],
        &[1, 2, 3],
    )
    .unwrap();
    drop(meta);
    let mut g = GraphStore::open(dir, tenant).unwrap();
    g.upsert_entity(node(ALICE, &[("born", "1901", &[3])]))
        .unwrap();
    g.upsert_entity(node(ALICE_ALIAS, &[])).unwrap();
    g.upsert_entity(node(ACME, &[])).unwrap();
    g.upsert_entity(node(PARIS, &[])).unwrap();
    g.upsert_entity(node(BOB, &[])).unwrap();
    g.upsert_relation(id(ALICE), id(ACME), "works_at", 1)
        .unwrap();
    g.upsert_relation(id(ALICE_ALIAS), id(PARIS), "lived_in", 2)
        .unwrap();
}

/// One simulated dream cycle. With `obs`, hooks are called where run_dream_cycle calls them.
fn run_cycle(dir: &Path, tenant: Uuid, mut obs: Option<&mut ProvenanceObserver>) {
    let meta = MetaStore::open(dir, tenant).unwrap();
    let mut g = GraphStore::open(dir, tenant).unwrap();
    if let Some(o) = obs.as_mut() {
        o.cycle_started(&g, &meta);
    }
    drop(meta);
    // Completion: a dream-added relation for an entity with no chunks → placeholder chunk 0.
    g.upsert_relation(id(BOB), id(ACME), "works_at", 0).unwrap();
    if let Some(o) = obs.as_mut() {
        o.stage_finished(DreamStage::Completion, &g);
    }
    let pairs = [(id(ALICE_ALIAS), id(ALICE))];
    for (a, c) in pairs {
        g.merge_entity_into(a, c).unwrap();
    }
    g.rebuild_in_memory().ok();
    if let Some(o) = obs.as_mut() {
        o.stage_finished(DreamStage::Merge { pairs: &pairs }, &g);
    }
    if let Some(o) = obs.as_mut() {
        o.stage_finished(DreamStage::Prune, &g);
    }
    g.sanitize_relations().unwrap();
    if let Some(o) = obs.as_mut() {
        o.stage_finished(DreamStage::Sanitize, &g);
        o.cycle_finished(&g, CycleEnd::Completed);
    }
}

fn graph_state(dir: &Path, tenant: Uuid) -> serde_json::Value {
    let g = GraphStore::open(dir, tenant).unwrap();
    let mut rels: Vec<serde_json::Value> = g
        .all_relation_records()
        .iter()
        .map(|r| serde_json::to_value(r).unwrap())
        .collect();
    rels.sort_by_key(|v| v.to_string());
    let mut ents: Vec<serde_json::Value> = g
        .all_entities()
        .map(|n| serde_json::to_value(n).unwrap())
        .collect();
    ents.sort_by_key(|v| v.to_string());
    serde_json::json!({ "relations": rels, "entities": ents })
}

fn summary(dir: &Path, cycle: u64) -> CycleSummary {
    serde_json::from_slice(&std::fs::read(dir.join(format!("cycle-{cycle:04}.json"))).unwrap())
        .unwrap()
}

fn claim_id(key: FactKey) -> String {
    key.claim().id
}

#[test]
fn observing_does_not_change_the_graph() {
    let tenant = Uuid::new_v4();
    let plain = tempfile::tempdir().unwrap();
    let observed = tempfile::tempdir().unwrap();
    let prov = tempfile::tempdir().unwrap();
    build_kb(plain.path(), tenant);
    build_kb(observed.path(), tenant);
    assert_eq!(
        graph_state(plain.path(), tenant),
        graph_state(observed.path(), tenant)
    );

    run_cycle(plain.path(), tenant, None);
    let mut obs = ProvenanceObserver::open(prov.path(), tenant).unwrap();
    run_cycle(observed.path(), tenant, Some(&mut obs));
    assert_eq!(obs.failure(), None);

    assert_eq!(
        graph_state(plain.path(), tenant),
        graph_state(observed.path(), tenant)
    );
}

#[test]
fn records_baseline_dream_changes_and_traceability() {
    let tenant = Uuid::new_v4();
    let kb = tempfile::tempdir().unwrap();
    let prov = tempfile::tempdir().unwrap();
    build_kb(kb.path(), tenant);
    let mut obs = ProvenanceObserver::open(prov.path(), tenant).unwrap();
    assert_eq!(obs.cycle(), Some(1));
    run_cycle(kb.path(), tenant, Some(&mut obs));
    assert_eq!(obs.failure(), None);

    let s = summary(prov.path(), 1);
    assert!(s.complete);
    assert_eq!(s.end, "completed");
    assert_eq!(s.source_chunks, 3);

    // Facts at the end equal the graph's facts.
    let g = GraphStore::open(kb.path(), tenant).unwrap();
    let live = facts::snapshot(&g);
    assert_eq!(s.facts, live.len());

    let log = persist::read_file(&prov.path().join(LINEAGE_FILE), TailPolicy::Strict)
        .unwrap()
        .replay()
        .unwrap();
    let traces = |k: FactKey| log.traces_to(&claim_id(k), facts::is_original);

    // Ingested fact on a real chunk: traceable.
    assert!(traces(FactKey::Relation {
        src: id(ALICE),
        relation: "works_at".into(),
        dst: id(ACME)
    }));
    // Field fact: traceable.
    assert!(traces(FactKey::Field {
        entity: id(ALICE),
        field: "born".into(),
        value: "1901".into()
    }));
    // Merged fact, rewritten from the alias: still traceable through the rewrite.
    let merged = FactKey::Relation {
        src: id(ALICE),
        relation: "lived_in".into(),
        dst: id(PARIS),
    };
    assert!(
        live.contains_key(&claim_id(merged.clone())),
        "merge should have moved the relation"
    );
    assert!(traces(merged));
    // Dream-added relation on the placeholder chunk 0: not traceable.
    assert!(!traces(FactKey::Relation {
        src: id(BOB),
        relation: "works_at".into(),
        dst: id(ACME)
    }));

    assert_eq!(s.traceable, s.facts - 1);
    assert_eq!(s.changes["baseline"].extracted, 3);
    assert_eq!(s.changes["dream.completion"].extracted, 1);
    assert_eq!(s.changes["dream.merge"].rewritten, 1);
    assert_eq!(s.by_origin["baseline"].facts, 2); // the rewritten fact's first producer is the merge
    assert_eq!(s.by_origin["dream.completion"].traceable, 0);
    // Observe mode: every introduced fact checked, stub verifier → all would block.
    assert_eq!(s.gate_checked, 2);
    assert_eq!(s.gate_would_block, 2);
    assert_eq!(s.lineage_digest, persist::lineage_digest(&log).unwrap());
}

#[test]
fn second_cycle_reconciles_unobserved_changes_as_drift() {
    let tenant = Uuid::new_v4();
    let kb = tempfile::tempdir().unwrap();
    let prov = tempfile::tempdir().unwrap();
    build_kb(kb.path(), tenant);
    let mut obs = ProvenanceObserver::open(prov.path(), tenant).unwrap();
    run_cycle(kb.path(), tenant, Some(&mut obs));
    drop(obs);

    // Changed outside any observed cycle.
    {
        let mut g = GraphStore::open(kb.path(), tenant).unwrap();
        g.upsert_relation(id(BOB), id(PARIS), "lived_in", 2)
            .unwrap();
    }
    let mut obs = ProvenanceObserver::open(prov.path(), tenant).unwrap();
    assert_eq!(obs.cycle(), Some(2));
    let meta = MetaStore::open(kb.path(), tenant).unwrap();
    let g = GraphStore::open(kb.path(), tenant).unwrap();
    obs.cycle_started(&g, &meta);
    obs.cycle_finished(&g, CycleEnd::NothingToDo);
    assert_eq!(obs.failure(), None);

    let s = summary(prov.path(), 2);
    assert_eq!(s.end, "nothing_to_do");
    assert_eq!(s.changes["untracked"].extracted, 1);
    assert!(!s.changes.contains_key("baseline"));
    assert_eq!(s.facts, facts::snapshot(&g).len());
}

#[test]
fn a_cycle_with_no_changes_still_advances_the_cycle_number() {
    let tenant = Uuid::new_v4();
    let kb = tempfile::tempdir().unwrap();
    let prov = tempfile::tempdir().unwrap();
    build_kb(kb.path(), tenant);
    for expected in 1..=3 {
        let mut obs = ProvenanceObserver::open(prov.path(), tenant).unwrap();
        assert_eq!(obs.cycle(), Some(expected));
        let meta = MetaStore::open(kb.path(), tenant).unwrap();
        let g = GraphStore::open(kb.path(), tenant).unwrap();
        obs.cycle_started(&g, &meta);
        obs.cycle_finished(&g, CycleEnd::NothingToDo);
    }
    assert!(prov.path().join("cycle-0003.json").exists());
}

#[test]
fn refuses_another_kbs_directory() {
    let prov = tempfile::tempdir().unwrap();
    ProvenanceObserver::open(prov.path(), Uuid::new_v4()).unwrap();
    let err = ProvenanceObserver::open(prov.path(), Uuid::new_v4())
        .err()
        .unwrap();
    assert!(err.contains("separate directory per KB"), "{err}");
}

#[test]
fn refuses_a_corrupted_lineage_file_and_env_falls_back_to_noop() {
    let tenant = Uuid::new_v4();
    let kb = tempfile::tempdir().unwrap();
    let prov = tempfile::tempdir().unwrap();
    build_kb(kb.path(), tenant);
    let mut obs = ProvenanceObserver::open(prov.path(), tenant).unwrap();
    run_cycle(kb.path(), tenant, Some(&mut obs));
    drop(obs);

    let path = prov.path().join(LINEAGE_FILE);
    let mut text = std::fs::read_to_string(&path).unwrap();
    text = text.replacen("\"chunk:1\"", "\"chunk:9\"", 1);
    std::fs::write(&path, text).unwrap();
    let err = ProvenanceObserver::open(prov.path(), tenant).err().unwrap();
    assert!(err.contains("IdMismatch"), "{err}");

    std::env::set_var(kwaai_provenance_rag::ENV_DIR, prov.path());
    let (_obs, status) = kwaai_provenance_rag::observer_from_env(tenant);
    std::env::remove_var(kwaai_provenance_rag::ENV_DIR);
    assert!(status.unwrap().starts_with("NOT recording"));
}

#[test]
fn report_tool_prints_cycles_and_detects_a_changed_log() {
    let tenant = Uuid::new_v4();
    let kb = tempfile::tempdir().unwrap();
    let prov = tempfile::tempdir().unwrap();
    build_kb(kb.path(), tenant);
    let mut obs = ProvenanceObserver::open(prov.path(), tenant).unwrap();
    run_cycle(kb.path(), tenant, Some(&mut obs));
    drop(obs);

    let bin = env!("CARGO_BIN_EXE_provenance-report");
    let out = std::process::Command::new(bin)
        .arg(prov.path())
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success(),
        "{stdout}\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(stdout.contains("matches the latest summary"), "{stdout}");
    assert!(stdout.contains("75.0%"), "{stdout}");

    let csv = std::process::Command::new(bin)
        .arg(prov.path())
        .arg("--csv")
        .output()
        .unwrap();
    let csv = String::from_utf8_lossy(&csv.stdout);
    assert!(csv
        .lines()
        .next()
        .unwrap()
        .starts_with("cycle,end,complete,facts"));
    assert_eq!(csv.lines().count(), 2);

    // Append a valid record after the summary: history no longer matches it.
    {
        let mut log = persist::read_file(&prov.path().join(LINEAGE_FILE), TailPolicy::Strict)
            .unwrap()
            .replay()
            .unwrap();
        let mut w =
            persist::JsonlWriter::append_to(&prov.path().join(LINEAGE_FILE), log.entries().len())
                .unwrap();
        let ev = kwaai_provenance::lineage::LineageEvent::new(
            kwaai_provenance::lineage::LineageEventKind::Extract,
            ["chunk:1"],
            ["late"],
            "test",
            Some(9),
            999,
        )
        .unwrap();
        log.propose(ev).unwrap();
        w.sync_lineage(&log).unwrap();
    }
    let out = std::process::Command::new(bin)
        .arg(prov.path())
        .output()
        .unwrap();
    assert!(!out.status.success());
}
