//! [`ProvenanceObserver`]: records dream-cycle changes as lineage.
//!
//! Files written in the provenance directory:
//!
//! | File | Contents |
//! |------|----------|
//! | `kb.json` | The knowledge base (tenant) this directory belongs to. A directory is never shared between KBs. |
//! | `lineage.jsonl` | Every lineage entry and gate outcome, appended across cycles (`kwaai_provenance::persist` format). |
//! | `cycle-NNNN.json` | One summary per observed cycle: fact counts, traceability, what each stage changed. |
//!
//! The first observed cycle records every fact already in the graph as a
//! `baseline` extraction from its evidence chunks. Later cycles first reconcile
//! the log with the graph: anything that changed between cycles without being
//! observed (re-ingestion, a cycle run without observation, manual edits) is
//! recorded under the actor `untracked` and counted as drift, so the numbers
//! never silently mix observed and unobserved history.
//!
//! Every event is recorded after the graph write it describes, so all events are
//! committed. Writes that failed leave no trace in the graph and none in lineage;
//! they appear only in the dream report's `cycle_errors`.

use crate::facts::{
    self, chunk_artifact, is_original, parse_chunk_artifact, Snapshot, NO_EVIDENCE,
};
use crate::plan::{plan, Step};
use kwaai_provenance::gate::{Gate, GateMode, UnresolvedVerifier};
use kwaai_provenance::lineage::{
    LineageError, LineageEvent, LineageEventKind, LineageLog, LogEntry,
};
use kwaai_provenance::persist::{self, JsonlWriter, TailPolicy};
use kwaai_provenance::{EvidenceKind, EvidenceRef, SCHEMA_VERSION};
use kwaai_rag::dream::{CycleEnd, DreamObserver, DreamStage};
use kwaai_rag::graph::GraphStore;
use kwaai_rag::meta_store::MetaStore;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::{Path, PathBuf};
use uuid::Uuid;

/// Environment variable naming the provenance directory. Unset or empty: off.
pub const ENV_DIR: &str = "KWAAI_PROVENANCE_DIR";

/// Lineage file name inside the provenance directory.
pub const LINEAGE_FILE: &str = "lineage.jsonl";

/// Actor recorded for facts already in the graph when observation starts.
pub const ACTOR_BASELINE: &str = "baseline";
/// Actor recorded for changes that happened between observed cycles.
pub const ACTOR_UNTRACKED: &str = "untracked";
/// Actor recorded for changes found at cycle end that no stage accounted for.
pub const ACTOR_UNATTRIBUTED: &str = "dream.unattributed";

/// What one stage changed.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct StageCounts {
    /// New facts.
    pub extracted: usize,
    /// Facts that gained evidence chunks.
    pub evidence_added: usize,
    /// Facts rewritten into another (merge, field-value change).
    pub rewritten: usize,
    /// Facts removed.
    pub removed: usize,
}

/// Traceability of facts grouped by how they entered the graph.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct OriginCounts {
    /// Facts.
    pub facts: usize,
    /// Of which trace to an original chunk.
    pub traceable: usize,
}

/// Summary of one observed cycle, written as `cycle-NNNN.json`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CycleSummary {
    /// `kwaai-provenance` schema version.
    pub schema_version: u32,
    /// Cycle number (1 = first observed cycle).
    pub cycle: u64,
    /// Knowledge base.
    pub tenant_id: Uuid,
    /// How the cycle ended (`completed`, `nothing_to_do`, `skipped_destructive_stages`),
    /// or `observer_failed`.
    pub end: String,
    /// False if the observer stopped part-way; numbers then cover only what was seen.
    pub complete: bool,
    /// Why the observer stopped, if it did.
    pub error: Option<String>,
    /// Facts in the graph at cycle end.
    pub facts: usize,
    /// Of which relations.
    pub relation_facts: usize,
    /// Of which entity field values.
    pub field_facts: usize,
    /// Facts whose lineage reaches at least one original chunk.
    pub traceable: usize,
    /// `traceable / facts`; `None` when there are no facts.
    pub traceability_rate: Option<f64>,
    /// Breakdown by the actor that first produced each fact.
    pub by_origin: BTreeMap<String, OriginCounts>,
    /// What each actor changed during this cycle (includes `baseline` / `untracked`).
    pub changes: BTreeMap<String, StageCounts>,
    /// Lineage commits refused because they would have created a cycle.
    pub lineage_conflicts: usize,
    /// Gate checks run in Observe mode on facts this cycle introduced.
    pub gate_checked: usize,
    /// Of which Enforce mode would have rejected or quarantined.
    pub gate_would_block: usize,
    /// Chunks in the chunk store at cycle start.
    pub source_chunks: usize,
    /// Lineage entries in the log after this cycle.
    pub lineage_entries: usize,
    /// `kwaai_provenance::persist::lineage_digest` of the log after this cycle.
    pub lineage_digest: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct KbFile {
    schema_version: u32,
    tenant_id: Uuid,
}

struct Inner {
    dir: PathBuf,
    tenant: Uuid,
    log: LineageLog,
    writer: JsonlWriter<File>,
    gate: Gate,
    cycle: u64,
    next_seq: u64,
    real_chunks: BTreeSet<i64>,
    chunk_hashes: BTreeMap<i64, String>,
    snapshot: Snapshot,
    changes: BTreeMap<String, StageCounts>,
    conflicts: usize,
    gate_checked: usize,
    gate_would_block: usize,
}

/// Records a knowledge base's dream cycles as lineage. See the module docs.
pub struct ProvenanceObserver {
    inner: Option<Inner>,
    failure: Option<String>,
}

impl ProvenanceObserver {
    /// Open (or start) the provenance directory `dir` for KB `tenant`. Creates
    /// the directory if needed. Refuses a directory that belongs to another KB or
    /// whose lineage file does not replay cleanly.
    pub fn open(dir: &Path, tenant: Uuid) -> Result<Self, String> {
        fs::create_dir_all(dir).map_err(|e| format!("create {}: {e}", dir.display()))?;
        let kb_path = dir.join("kb.json");
        if kb_path.exists() {
            let kb: KbFile = serde_json::from_slice(
                &fs::read(&kb_path).map_err(|e| format!("read {}: {e}", kb_path.display()))?,
            )
            .map_err(|e| format!("parse {}: {e}", kb_path.display()))?;
            if kb.tenant_id != tenant {
                return Err(format!(
                    "{} belongs to KB {}, not {tenant}; use a separate directory per KB",
                    dir.display(),
                    kb.tenant_id
                ));
            }
        } else {
            let kb = KbFile {
                schema_version: SCHEMA_VERSION,
                tenant_id: tenant,
            };
            write_atomic(
                &kb_path,
                &serde_json::to_vec_pretty(&kb).map_err(|e| e.to_string())?,
            )?;
        }

        let path = dir.join(LINEAGE_FILE);
        let log = if path.exists() {
            persist::read_file(&path, TailPolicy::Strict)
                .and_then(|r| r.replay())
                .map_err(|e| {
                    format!(
                        "{}: {e}. If the last line is torn from an interrupted write, \
                         kwaai_provenance::persist::truncate_torn_tail repairs it.",
                        path.display()
                    )
                })?
        } else {
            LineageLog::new()
        };
        let writer = JsonlWriter::append_to(&path, log.entries().len())
            .map_err(|e| format!("open {}: {e}", path.display()))?;
        let (max_cycle, proposed) = log_position(&log);
        let max_cycle = max_cycle.max(last_summary_cycle(dir));
        Ok(ProvenanceObserver {
            inner: Some(Inner {
                dir: dir.to_path_buf(),
                tenant,
                log,
                writer,
                gate: Gate::new(GateMode::Observe),
                cycle: max_cycle + 1,
                next_seq: proposed,
                real_chunks: BTreeSet::new(),
                chunk_hashes: BTreeMap::new(),
                snapshot: Snapshot::new(),
                changes: BTreeMap::new(),
                conflicts: 0,
                gate_checked: 0,
                gate_would_block: 0,
            }),
            failure: None,
        })
    }

    /// The cycle number this observer will record (1 for a new directory).
    pub fn cycle(&self) -> Option<u64> {
        self.inner.as_ref().map(|i| i.cycle)
    }

    /// The error that stopped observation, if any.
    pub fn failure(&self) -> Option<&str> {
        self.failure.as_deref()
    }

    /// Run `f` on the inner state; on error or panic, stop observing and keep the
    /// reason. Never lets either escape into the dream cycle.
    fn guarded(&mut self, what: &str, f: impl FnOnce(&mut Inner) -> Result<(), String>) {
        if self.failure.is_some() {
            return;
        }
        let Some(inner) = self.inner.as_mut() else {
            return;
        };
        let outcome = catch_unwind(AssertUnwindSafe(|| f(inner)));
        let err = match outcome {
            Ok(Ok(())) => return,
            Ok(Err(e)) => e,
            Err(_) => "observer panicked".to_string(),
        };
        let msg = format!("{what}: {err}");
        tracing::warn!("provenance observer stopped; the dream cycle is unaffected: {msg}");
        self.failure = Some(msg);
    }
}

impl DreamObserver for ProvenanceObserver {
    fn cycle_started(&mut self, graph: &GraphStore, meta: &MetaStore) {
        self.guarded("cycle start", |inner| {
            let chunks = meta
                .all_chunks()
                .map_err(|e| format!("read chunk store: {e}"))?;
            inner.real_chunks = chunks.iter().map(|(id, _)| *id).collect();
            inner.chunk_hashes = chunks
                .iter()
                .map(|(id, c)| (*id, hex::encode(Sha256::digest(c.text.as_bytes()))))
                .collect();
            let current = facts::snapshot(graph);
            inner.reconcile(&current)?;
            inner.snapshot = current;
            inner.flush()
        });
    }

    fn stage_finished(&mut self, stage: DreamStage<'_>, graph: &GraphStore) {
        let (actor, pairs): (&str, &[(i64, i64)]) = match stage {
            DreamStage::Completion => ("dream.completion", &[]),
            DreamStage::Merge { pairs } => ("dream.merge", pairs),
            DreamStage::Prune => ("dream.prune", &[]),
            DreamStage::Sanitize => ("dream.sanitize", &[]),
        };
        self.guarded(actor, |inner| {
            let next = facts::snapshot(graph);
            inner.record(actor, &next, pairs, true)?;
            inner.snapshot = next;
            inner.flush()
        });
    }

    fn cycle_finished(&mut self, graph: &GraphStore, end: CycleEnd) {
        let end = match end {
            CycleEnd::Completed => "completed",
            CycleEnd::NothingToDo => "nothing_to_do",
            CycleEnd::SkippedDestructiveStages => "skipped_destructive_stages",
        };
        self.guarded("cycle end", |inner| {
            let last = facts::snapshot(graph);
            inner.record(ACTOR_UNATTRIBUTED, &last, &[], true)?;
            inner.snapshot = last;
            inner.flush()
        });
        let failure = self.failure.clone();
        if let Some(inner) = self.inner.as_mut() {
            let summary = inner.summary(end, failure);
            let path = inner.dir.join(format!("cycle-{:04}.json", inner.cycle));
            let written = serde_json::to_vec_pretty(&summary)
                .map_err(|e| e.to_string())
                .and_then(|bytes| write_atomic(&path, &bytes));
            match written {
                Ok(()) => tracing::info!(
                    "provenance: cycle {}: {}/{} facts trace to source{}",
                    summary.cycle,
                    summary.traceable,
                    summary.facts,
                    if summary.complete {
                        ""
                    } else {
                        " (incomplete)"
                    }
                ),
                Err(e) => tracing::warn!("provenance: could not write {}: {e}", path.display()),
            }
            let _ = inner.writer.sync_all();
        }
    }
}

impl Inner {
    fn flush(&mut self) -> Result<(), String> {
        self.writer
            .sync_lineage(&self.log)
            .map_err(|e| e.to_string())?;
        self.writer.flush().map_err(|e| e.to_string())
    }

    /// Bring the log in line with the graph before the cycle starts.
    fn reconcile(&mut self, current: &Snapshot) -> Result<(), String> {
        if !has_committed(&self.log) {
            for fact in current.values() {
                let chunks: Vec<i64> = fact.evidence.iter().copied().collect();
                self.extract(ACTOR_BASELINE, Some(0), &fact.claim.id, &chunks, false)?;
            }
            return Ok(());
        }
        let live = live_claims(&self.log);
        let cycle = Some(self.cycle);
        for id in live.iter().filter(|id| !current.contains_key(*id)) {
            self.commit(
                ACTOR_UNTRACKED,
                cycle,
                LineageEventKind::Prune,
                vec![id.clone()],
                vec![],
                "removed",
            )?;
        }
        for (id, fact) in current {
            let chunks: Vec<i64> = fact.evidence.iter().copied().collect();
            if !live.contains(id) {
                self.extract(ACTOR_UNTRACKED, cycle, id, &chunks, false)?;
                continue;
            }
            let known: BTreeSet<i64> = self
                .log
                .ancestors(id)
                .iter()
                .filter_map(|a| parse_chunk_artifact(a))
                .collect();
            let gained: Vec<i64> = fact.evidence.difference(&known).copied().collect();
            if !gained.is_empty() {
                self.extract(ACTOR_UNTRACKED, cycle, id, &gained, true)?;
            }
        }
        Ok(())
    }

    /// Record what changed between `self.snapshot` and `next` under `actor`.
    fn record(
        &mut self,
        actor: &str,
        next: &Snapshot,
        pairs: &[(i64, i64)],
        gate: bool,
    ) -> Result<(), String> {
        let cycle = Some(self.cycle);
        let mut introduced: Vec<String> = Vec::new();
        for step in plan(&self.snapshot, next, pairs) {
            match step {
                Step::Extract { to, chunks } => {
                    self.extract(actor, cycle, &to, &chunks, false)?;
                    introduced.push(to);
                }
                Step::EvidenceAdded { to, chunks } => {
                    self.extract(actor, cycle, &to, &chunks, true)?;
                }
                Step::Rewrite { from, to } => {
                    self.commit(
                        actor,
                        cycle,
                        LineageEventKind::ClaimRewrite,
                        vec![from],
                        vec![to.clone()],
                        "rewritten",
                    )?;
                    introduced.push(to);
                }
                Step::Remove { claim } => {
                    self.commit(
                        actor,
                        cycle,
                        LineageEventKind::Prune,
                        vec![claim],
                        vec![],
                        "removed",
                    )?;
                }
            }
        }
        if gate {
            for id in introduced {
                if let Some(fact) = next.get(&id) {
                    self.gate_check(fact)?;
                }
            }
        }
        Ok(())
    }

    fn extract(
        &mut self,
        actor: &str,
        cycle: Option<u64>,
        claim: &str,
        chunks: &[i64],
        evidence_only: bool,
    ) -> Result<(), String> {
        let inputs: Vec<String> = if chunks.is_empty() {
            vec![NO_EVIDENCE.to_string()]
        } else {
            chunks
                .iter()
                .map(|c| chunk_artifact(*c, &self.real_chunks))
                .collect()
        };
        let counter = if evidence_only {
            "evidence_added"
        } else {
            "extracted"
        };
        self.commit(
            actor,
            cycle,
            LineageEventKind::Extract,
            inputs,
            vec![claim.to_string()],
            counter,
        )
    }

    fn commit(
        &mut self,
        actor: &str,
        cycle: Option<u64>,
        kind: LineageEventKind,
        inputs: Vec<String>,
        outputs: Vec<String>,
        counter: &str,
    ) -> Result<(), String> {
        let seq = self.next_seq;
        self.next_seq += 1;
        let event = LineageEvent::new(kind, inputs, outputs, actor, cycle, seq)
            .map_err(|e| e.to_string())?;
        let id = self.log.propose(event).map_err(|e| e.to_string())?;
        match self.log.commit(&id) {
            Ok(()) => {
                let c = self.changes.entry(actor.to_string()).or_default();
                match counter {
                    "extracted" => c.extracted += 1,
                    "evidence_added" => c.evidence_added += 1,
                    "rewritten" => c.rewritten += 1,
                    _ => c.removed += 1,
                }
                Ok(())
            }
            Err(LineageError::CycleDetected { .. }) => {
                self.conflicts += 1;
                self.log
                    .fail(&id, "would create a lineage cycle")
                    .map_err(|e| e.to_string())
            }
            Err(e) => Err(e.to_string()),
        }
    }

    fn gate_check(&mut self, fact: &facts::Fact) -> Result<(), String> {
        let source = format!("kb:{}", self.tenant);
        let evidence: Vec<EvidenceRef> = fact
            .evidence
            .iter()
            .map(|c| match self.chunk_hashes.get(c) {
                Some(h) => EvidenceRef::new(
                    EvidenceKind::Original,
                    source.as_str(),
                    h.as_str(),
                    Some(*c),
                    vec![],
                ),
                None => {
                    EvidenceRef::new(EvidenceKind::Derived, source.as_str(), "", Some(*c), vec![])
                }
            })
            .collect();
        let outcome = self
            .gate
            .check(&fact.claim, &evidence, &UnresolvedVerifier)
            .map_err(|e| e.to_string())?;
        self.gate_checked += 1;
        if outcome.would_block() {
            self.gate_would_block += 1;
        }
        self.writer.write_gate(&outcome).map_err(|e| e.to_string())
    }

    fn summary(&self, end: &str, failure: Option<String>) -> CycleSummary {
        let first_actor = first_producers(&self.log);
        let mut by_origin: BTreeMap<String, OriginCounts> = BTreeMap::new();
        let (mut traceable, mut relations) = (0usize, 0usize);
        for (id, fact) in &self.snapshot {
            let ok = self.log.traces_to(id, is_original);
            if ok {
                traceable += 1;
            }
            if fact.key.is_relation() {
                relations += 1;
            }
            let origin = first_actor
                .get(id.as_str())
                .copied()
                .unwrap_or("unrecorded");
            let o = by_origin.entry(origin.to_string()).or_default();
            o.facts += 1;
            if ok {
                o.traceable += 1;
            }
        }
        let total = self.snapshot.len();
        CycleSummary {
            schema_version: SCHEMA_VERSION,
            cycle: self.cycle,
            tenant_id: self.tenant,
            end: if failure.is_some() {
                "observer_failed".to_string()
            } else {
                end.to_string()
            },
            complete: failure.is_none(),
            error: failure,
            facts: total,
            relation_facts: relations,
            field_facts: total - relations,
            traceable,
            traceability_rate: (total > 0).then(|| traceable as f64 / total as f64),
            by_origin,
            changes: self.changes.clone(),
            lineage_conflicts: self.conflicts,
            gate_checked: self.gate_checked,
            gate_would_block: self.gate_would_block,
            source_chunks: self.real_chunks.len(),
            lineage_entries: self.log.entries().len(),
            lineage_digest: persist::lineage_digest(&self.log).unwrap_or_default(),
        }
    }
}

/// Committed events in commit order.
fn committed_events(log: &LineageLog) -> Vec<&LineageEvent> {
    log.entries()
        .iter()
        .filter_map(|e| match e {
            LogEntry::Committed { event_id } => log.event(event_id),
            _ => None,
        })
        .collect()
}

fn has_committed(log: &LineageLog) -> bool {
    log.entries()
        .iter()
        .any(|e| matches!(e, LogEntry::Committed { .. }))
}

/// Highest cycle number recorded, and the number of proposed events (next seq).
fn log_position(log: &LineageLog) -> (u64, u64) {
    let mut max_cycle = 0;
    let mut proposed = 0;
    for e in log.entries() {
        if let LogEntry::Proposed { event } = e {
            proposed += 1;
            max_cycle = max_cycle.max(event.cycle.unwrap_or(0));
        }
    }
    (max_cycle, proposed)
}

/// Claims the log says are currently in the graph.
fn live_claims(log: &LineageLog) -> BTreeSet<String> {
    let mut live = BTreeSet::new();
    for ev in committed_events(log) {
        match ev.kind {
            LineageEventKind::Prune => {
                for i in &ev.inputs {
                    live.remove(i);
                }
            }
            LineageEventKind::ClaimRewrite => {
                for i in &ev.inputs {
                    live.remove(i);
                }
                live.extend(ev.outputs.iter().cloned());
            }
            _ => live.extend(ev.outputs.iter().cloned()),
        }
    }
    live
}

/// For each claim, the actor of the first committed event that produced it.
fn first_producers(log: &LineageLog) -> BTreeMap<&str, &str> {
    let mut out = BTreeMap::new();
    for ev in committed_events(log) {
        for o in &ev.outputs {
            out.entry(o.as_str()).or_insert(ev.actor.as_str());
        }
    }
    out
}

/// Highest `NNNN` among `cycle-NNNN.json` files in `dir`, or 0.
fn last_summary_cycle(dir: &Path) -> u64 {
    fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| {
            let name = e.file_name().into_string().ok()?;
            name.strip_prefix("cycle-")?
                .strip_suffix(".json")?
                .parse::<u64>()
                .ok()
        })
        .max()
        .unwrap_or(0)
}

fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let mut tmp = path.as_os_str().to_os_string();
    tmp.push(".tmp");
    let tmp = PathBuf::from(tmp);
    fs::write(&tmp, bytes).map_err(|e| format!("write {}: {e}", tmp.display()))?;
    fs::rename(&tmp, path).map_err(|e| format!("rename to {}: {e}", path.display()))
}
