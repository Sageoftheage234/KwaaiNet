//! Experimental historical capture schema v2. Single writer, separate from strict lineage.
//! Retains boundary-time claims, direct evidence and indexed extraction-window context,
//! with archived passage bytes, store binding checks and lifecycle validation.
#![allow(missing_docs)]
use crate::facts::{entity_ref, Fact, FactKey, Snapshot};
use kwaai_provenance::verification::{EvidenceResolver, SourcePassage, VerificationRequest};
use kwaai_provenance::{Claim, EvidenceKind, EvidenceRef};
use kwaai_rag::dream::{CycleEnd, DreamObserver, DreamStage};
use kwaai_rag::{graph::GraphStore, meta_store::MetaStore};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::{Path, PathBuf};
use uuid::Uuid;

type Result<T> = std::result::Result<T, String>;
fn hash(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}
fn json<T: Serialize>(v: &T) -> Result<Vec<u8>> {
    serde_json::to_vec(v).map_err(|e| e.to_string())
}
fn io(e: std::io::Error) -> String {
    e.to_string()
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Run {
    pub schema_version: u32,
    pub run_id: Uuid,
    pub tenant_id: Uuid,
    /// Caller assertion, never authenticated execution identity.
    pub corpus: Option<String>,
}
impl Run {
    pub fn new(tenant_id: Uuid, corpus: Option<String>) -> Self {
        Self {
            schema_version: 1,
            run_id: Uuid::new_v4(),
            tenant_id,
            corpus,
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Evidence {
    pub reference: EvidenceRef,
    pub document_id: Option<String>,
    pub chunk_index: Option<u32>,
    pub section_type: Option<kwaai_rag::doc_schema::SectionType>,
    /// `blobs/<sha256>` relative to the archive. None means unavailable.
    pub blob: Option<String>,
    pub retrieval: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExtractionContext {
    pub evidence_window: u8,
    pub direct_chunk_ids: Vec<i64>,
    /// Boundary-time passages in document/section-zone filtered centre ± window.
    pub expanded: Vec<Evidence>,
    /// Direct centres absent from the boundary-time store; no neighbours inferred.
    pub unresolved_centres: Vec<i64>,
    /// Gaps inside the observed document extent, never fabricated chunk IDs.
    pub missing_positions: Vec<MissingPosition>,
}
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MissingPosition {
    pub document_id: String,
    pub chunk_index: u32,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct State {
    pub occurrence_id: String,
    pub claim: Claim,
    pub bindings: BTreeMap<String, String>,
    /// Sorted by chunk ID; union for duplicate graph claims, never lineage ancestry.
    pub evidence: Vec<Evidence>,
    /// Kept separately from direct graph associations; empty for entity fields.
    pub extraction_contexts: Vec<ExtractionContext>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StoreBinding {
    pub graph: FileIdentity,
    pub evidence: FileIdentity,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FileIdentity {
    pub canonical_path: PathBuf,
    pub device: u64,
    pub inode: u64,
}
fn file_identity(path: &Path) -> Result<FileIdentity> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let m = fs::metadata(path).map_err(io)?;
        Ok(FileIdentity {
            canonical_path: path.canonicalize().map_err(io)?,
            device: m.dev(),
            inode: m.ino(),
        })
    }
    #[cfg(not(unix))]
    {
        let _ = path;
        Err("authoritative store identity unavailable on this platform".into())
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Capture {
    pub schema_version: u32,
    pub run: Run,
    pub cycle: u64,
    pub stage: String,
    pub boundary: String,
    pub sequence: u64,
    pub captured_at: String,
    pub complete: bool,
    pub error: Option<String>,
    pub cycle_end: Option<String>,
    /// Previous complete capture with identical claim/binding/evidence state.
    pub same_state_as: Option<u64>,
    pub states: Vec<State>,
    pub store_binding: Option<StoreBinding>,
}
/// SHA-256(domain || compact serde JSON tuple), using the NUL-terminated domain
/// `kwaai.history.state-occurrence.v2`. Struct order and BTreeMap order are fixed
/// by v2; exact UTF-8 is retained without normalization.
pub fn occurrence_id(c: &Capture, s: &State) -> Result<String> {
    let mut bytes = b"kwaai.history.state-occurrence.v2\0".to_vec();
    bytes.extend(json(&(
        &c.run,
        c.cycle,
        &c.stage,
        &c.boundary,
        c.sequence,
        &c.store_binding,
        &s.claim,
        &s.bindings,
        &s.evidence,
        &s.extraction_contexts,
    ))?);
    Ok(format!("state:{}", hash(&bytes)))
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Envelope {
    sha256: String,
    capture: Capture,
}

/// Publish complete bytes without replacing an existing artifact. A unique
/// temporary file is synced then hard-linked atomically on the same filesystem.
fn immutable(path: &Path, bytes: &[u8]) -> Result<()> {
    if path.exists() {
        return if fs::read(path).map_err(io)? == bytes {
            Ok(())
        } else {
            Err(format!("conflicting artifact {}", path.display()))
        };
    }
    let tmp = path.with_extension(format!("{}.tmp", Uuid::new_v4()));
    let result = (|| {
        let mut f = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&tmp)
            .map_err(io)?;
        f.write_all(bytes).map_err(io)?;
        f.sync_all().map_err(io)?;
        fs::hard_link(&tmp, path).map_err(io)?;
        File::open(path.parent().ok_or("missing parent")?)
            .map_err(io)?
            .sync_all()
            .map_err(io)
    })();
    let _ = fs::remove_file(tmp);
    result
}

/// Validate all committed records and blobs before resume/export. No torn-tail repair.
pub fn read_archive(dir: &Path) -> Result<Vec<Capture>> {
    let records = read_structural_archive(dir)?;
    for (i, c) in records.iter().enumerate() {
        validate_transition(c, i.checked_sub(1).map(|j| &records[j]))?;
    }
    Ok(records)
}
fn read_structural_archive(dir: &Path) -> Result<Vec<Capture>> {
    let run: Run = serde_json::from_slice(&fs::read(dir.join("run.json")).map_err(io)?)
        .map_err(|e| e.to_string())?;
    if run.schema_version != 1 {
        return Err("unsupported run version".into());
    }
    let mut paths = fs::read_dir(dir.join("records"))
        .map_err(io)?
        .map(|e| e.map(|e| e.path()).map_err(io))
        .collect::<Result<Vec<_>>>()?;
    paths.retain(|p| p.extension().is_some_and(|e| e == "json"));
    paths.sort();
    let mut out = Vec::new();
    for (i, path) in paths.iter().enumerate() {
        let e: Envelope = serde_json::from_slice(&fs::read(path).map_err(io)?)
            .map_err(|e| format!("{}: {e}", path.display()))?;
        let c = e.capture;
        if e.sha256 != hash(&json(&c)?)
            || c.schema_version != 2
            || c.run != run
            || c.sequence != i as u64 + 1
            || path.file_name().and_then(|s| s.to_str())
                != Some(&format!("{:020}.json", c.sequence))
        {
            return Err("record hash/version/run/order mismatch".into());
        }
        validate_capture(&c)?;
        if let Some(previous) = out
            .iter()
            .rev()
            .find_map(|p: &Capture| p.store_binding.as_ref())
        {
            if c.store_binding.as_ref().is_some_and(|b| b != previous) {
                return Err("archive store binding changed".into());
            }
        }
        for s in &c.states {
            for ev in all_evidence(s) {
                if let Some(blob) = &ev.blob {
                    if hash(&fs::read(dir.join(blob)).map_err(io)?) != ev.reference.content_hash {
                        return Err("invalid evidence blob hash".into());
                    }
                }
            }
        }
        if let Some(seq) = c.same_state_as {
            let prev: &Capture = out
                .get(seq.checked_sub(1).ok_or("invalid duplicate reference")? as usize)
                .ok_or("invalid duplicate reference")?;
            if !c.complete
                || !prev.complete
                || state_bytes(&prev.states)? != state_bytes(&c.states)?
            {
                return Err("invalid same-state reference".into());
            }
        }
        out.push(c);
    }
    Ok(out)
}
/// Validate one artifact without live stores or archive dependencies.
/// Lifecycle coverage and on-disk blob integrity are separate checks.
pub fn validate_capture(c: &Capture) -> Result<()> {
    if c.complete != c.error.is_none() || (!c.complete && !c.states.is_empty()) {
        return Err("invalid completeness".into());
    }
    if c.schema_version != 2
        || c.run.schema_version != 1
        || c.sequence == 0
        || c.cycle == 0
        || chrono_timestamp_invalid(&c.captured_at)
    {
        return Err("invalid cycle/timestamp".into());
    }
    if c.store_binding.as_ref().is_some_and(|b| {
        !b.graph.canonical_path.is_absolute()
            || !b.evidence.canonical_path.is_absolute()
            || b.graph == b.evidence
    }) {
        return Err("invalid store binding".into());
    }
    if c.complete && c.store_binding.is_none() {
        return Err("complete capture has no store binding".into());
    }
    if !matches!(
        (
            c.boundary.as_str(),
            c.stage.as_str(),
            c.cycle_end.as_deref()
        ),
        ("cycle_start", "start", None)
            | (
                "post_stage",
                "completion" | "merge" | "prune" | "sanitize",
                None
            )
            | (
                "cycle_finish",
                "finish",
                Some("Completed" | "NothingToDo" | "SkippedDestructiveStages")
            )
            | ("resume_incomplete", "unknown", Some("incomplete"))
    ) {
        return Err("invalid capture boundary".into());
    }
    if c.same_state_as
        .is_some_and(|seq| !c.complete || seq == 0 || seq >= c.sequence)
    {
        return Err("invalid same-state reference".into());
    }
    if c.boundary == "resume_incomplete" && c.complete {
        return Err("complete interruption marker".into());
    }
    let mut last: Option<&str> = None;
    for s in &c.states {
        if last.is_some_and(|id| id >= s.claim.id.as_str())
            || occurrence_id(c, s)? != s.occurrence_id
            || kwaai_provenance::claim_id(
                s.claim.kind.clone(),
                &s.claim.subject,
                &s.claim.predicate,
                &s.claim.object,
            ) != s.claim.id
        {
            return Err("invalid/duplicate state".into());
        }
        last = Some(&s.claim.id);
        validate_contexts(s)?;
        let mut identities = BTreeMap::new();
        for ev in all_evidence(s) {
            if identities
                .insert(ev.reference.chunk_id, ev)
                .is_some_and(|old| old != ev)
            {
                return Err("conflicting evidence for historical chunk".into());
            }
        }
        for evidence in
            std::iter::once(&s.evidence).chain(s.extraction_contexts.iter().map(|x| &x.expanded))
        {
            let mut previous = None;
            for ev in evidence {
                let r = &ev.reference;
                if previous.is_some_and(|id| Some(id) >= r.chunk_id)
                    || r.chunk_id.is_none()
                    || r.source_id != format!("kb:{}", c.run.tenant_id)
                    || !r.parent_ids.is_empty()
                    || kwaai_provenance::evidence_id(
                        r.kind.clone(),
                        &r.source_id,
                        &r.content_hash,
                        r.chunk_id,
                    ) != r.id
                {
                    return Err("invalid evidence identity/order".into());
                }
                previous = r.chunk_id;
                match &ev.blob {
                    Some(blob) => {
                        if r.content_hash.len() != 64
                            || !r.content_hash.bytes().all(|b| b.is_ascii_hexdigit())
                            || *blob != format!("blobs/{}", r.content_hash)
                            || ev.retrieval != "available"
                            || r.kind != EvidenceKind::Original
                            || ev.document_id.is_none()
                            || ev.chunk_index.is_none()
                            || ev.section_type.is_none()
                        {
                            return Err("invalid evidence blob".into());
                        }
                    }
                    None => {
                        if ev.retrieval != "missing"
                            || r.kind != EvidenceKind::Derived
                            || !r.content_hash.is_empty()
                            || ev.document_id.is_some()
                            || ev.chunk_index.is_some()
                            || ev.section_type.is_some()
                        {
                            return Err("invalid missing evidence".into());
                        }
                    }
                }
            }
        }
    }
    Ok(())
}
fn chrono_timestamp_invalid(value: &str) -> bool {
    chrono::DateTime::parse_from_rfc3339(value).is_err()
}
fn validate_transition(c: &Capture, previous: Option<&Capture>) -> Result<()> {
    match c.boundary.as_str() {
        "cycle_start" if c.stage == "start" && c.cycle_end.is_none() => {
            if previous.is_some_and(|p| p.cycle_end.is_none() || c.cycle != p.cycle + 1)
                || (previous.is_none() && c.cycle != 1)
            {
                return Err("invalid cycle start order".into());
            }
        }
        "post_stage" if stage_rank(&c.stage).is_some() && c.cycle_end.is_none() => {
            if previous.is_none_or(|p| {
                p.cycle != c.cycle
                    || p.cycle_end.is_some()
                    || stage_rank(&p.stage).unwrap_or(0) + 1 != stage_rank(&c.stage).unwrap_or(0)
            }) {
                return Err("invalid stage order".into());
            }
        }
        "cycle_finish"
            if c.stage == "finish"
                && c.cycle_end.as_deref().is_some_and(|end| {
                    matches!(
                        end,
                        "Completed" | "NothingToDo" | "SkippedDestructiveStages"
                    )
                }) =>
        {
            if previous.is_none_or(|p| {
                p.cycle != c.cycle
                    || p.cycle_end.is_some()
                    || match c.cycle_end.as_deref() {
                        Some("Completed") => p.stage != "sanitize",
                        Some("NothingToDo") => p.stage != "start",
                        Some("SkippedDestructiveStages") => p.stage != "completion",
                        _ => true,
                    }
            }) {
                return Err("invalid cycle finish order".into());
            }
        }
        "resume_incomplete"
            if !c.complete
                && c.stage == "unknown"
                && c.cycle_end.as_deref() == Some("incomplete") =>
        {
            if previous.is_none_or(|p| p.cycle != c.cycle || p.cycle_end.is_some()) {
                return Err("invalid incomplete marker".into());
            }
        }
        _ => return Err("unknown/invalid boundary".into()),
    }
    Ok(())
}
fn all_evidence(s: &State) -> impl Iterator<Item = &Evidence> {
    s.evidence
        .iter()
        .chain(s.extraction_contexts.iter().flat_map(|x| x.expanded.iter()))
}
fn validate_contexts(s: &State) -> Result<()> {
    if s.claim.kind != kwaai_provenance::ClaimKind::Relation && !s.extraction_contexts.is_empty() {
        return Err("field has relation extraction context".into());
    }
    if s.claim.kind == kwaai_provenance::ClaimKind::Relation && s.extraction_contexts.is_empty() {
        return Err("relation missing extraction metadata".into());
    }
    let direct: BTreeSet<_> = s
        .evidence
        .iter()
        .filter_map(|e| e.reference.chunk_id)
        .collect();
    let mut covered = BTreeSet::new();
    let mut previous = None;
    for x in &s.extraction_contexts {
        let key = (x.evidence_window, x.direct_chunk_ids.clone());
        if previous.as_ref().is_some_and(|p| p >= &key)
            || x.direct_chunk_ids.windows(2).any(|w| w[0] >= w[1])
            || x.unresolved_centres.windows(2).any(|w| w[0] >= w[1])
            || x.missing_positions.windows(2).any(|w| w[0] >= w[1])
        {
            return Err("invalid extraction context order".into());
        }
        previous = Some(key);
        for id in &x.direct_chunk_ids {
            if !direct.contains(id) {
                return Err("context centre outside direct evidence".into());
            }
            covered.insert(*id);
            let ev = s
                .evidence
                .iter()
                .find(|e| e.reference.chunk_id == Some(*id))
                .ok_or("missing centre")?;
            if (ev.retrieval == "missing") != x.unresolved_centres.contains(id) {
                return Err("context centre retrieval mismatch".into());
            }
            if ev.retrieval == "available" && !x.expanded.contains(ev) {
                return Err("expanded context missing centre".into());
            }
        }
        for e in &x.expanded {
            if !x.direct_chunk_ids.iter().any(|id| {
                let Some(centre) = s
                    .evidence
                    .iter()
                    .find(|c| c.reference.chunk_id == Some(*id))
                else {
                    return false;
                };
                centre.document_id == e.document_id
                    && centre
                        .chunk_index
                        .zip(e.chunk_index)
                        .is_some_and(|(a, b)| a.abs_diff(b) <= x.evidence_window as u32)
                    && (centre.reference.chunk_id == e.reference.chunk_id
                        || centre
                            .section_type
                            .as_ref()
                            .zip(e.section_type.as_ref())
                            .is_some_and(|(a, b)| a.same_window_zone(b)))
            }) {
                return Err("expanded passage outside retained window/zone".into());
            }
        }
        for p in &x.missing_positions {
            if x.expanded.iter().any(|e| {
                e.document_id.as_deref() == Some(p.document_id.as_str())
                    && e.chunk_index == Some(p.chunk_index)
            }) || !s.evidence.iter().any(|c| {
                c.reference
                    .chunk_id
                    .is_some_and(|id| x.direct_chunk_ids.contains(&id))
                    && c.document_id.as_deref() == Some(p.document_id.as_str())
                    && c.chunk_index
                        .is_some_and(|i| i.abs_diff(p.chunk_index) <= x.evidence_window as u32)
            }) {
                return Err("invalid missing window position".into());
            }
        }
        if x.unresolved_centres
            .iter()
            .any(|id| !x.direct_chunk_ids.contains(id))
        {
            return Err("unknown unresolved centre".into());
        }
    }
    if s.claim.kind == kwaai_provenance::ClaimKind::Relation && direct != covered {
        return Err("incomplete direct/context association".into());
    }
    Ok(())
}
fn stage_rank(stage: &str) -> Option<u8> {
    match stage {
        "completion" => Some(1),
        "merge" => Some(2),
        "prune" => Some(3),
        "sanitize" => Some(4),
        _ => None,
    }
}
fn state_bytes(states: &[State]) -> Result<Vec<u8>> {
    json(
        &states
            .iter()
            .map(|s| (&s.claim, &s.bindings, &s.evidence, &s.extraction_contexts))
            .collect::<Vec<_>>(),
    )
}

/// Opt in by constructing this observer and passing it to
/// `run_dream_cycle_observed`. Archive directory belongs to exactly one run.
pub struct HistoricalObserver {
    dir: PathBuf,
    data_dir: PathBuf,
    run: Run,
    records: Vec<Capture>,
    cycle: u64,
    active: bool,
    failure: Option<String>,
    _lock: File,
    binding: Option<StoreBinding>,
}
impl HistoricalObserver {
    pub fn open(dir: &Path, data_dir: &Path, run: Run) -> Result<Self> {
        if run.schema_version != 1 {
            return Err("unsupported run version".into());
        }
        fs::create_dir_all(dir.join("records")).map_err(io)?;
        fs::create_dir_all(dir.join("blobs")).map_err(io)?;
        let lock = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(dir.join("writer.lock"))
            .map_err(io)?;
        let result = (|| {
            immutable(&dir.join("run.json"), &json(&run)?)?;
            read_archive(dir)
        })();
        let records = match result {
            Ok(r) => r,
            Err(e) => {
                let _ = fs::remove_file(dir.join("writer.lock"));
                return Err(e);
            }
        };
        let cycle = records.last().map_or(1, |c| c.cycle + 1);
        let unfinished = records
            .last()
            .filter(|c| c.cycle_end.is_none())
            .map(|c| c.cycle);
        let binding = records.iter().rev().find_map(|c| c.store_binding.clone());
        let mut obs = Self {
            dir: dir.into(),
            data_dir: data_dir.into(),
            run,
            records,
            cycle,
            active: false,
            failure: None,
            _lock: lock,
            binding,
        };
        if let Some(cycle) = unfinished {
            let mut c = obs.empty("unknown", "resume_incomplete");
            c.cycle = cycle;
            c.complete = false;
            c.error = Some("previous cycle has no observed finish; cause unknown".into());
            c.cycle_end = Some("incomplete".into());
            obs.append(c)?;
        }
        Ok(obs)
    }
    pub fn failure(&self) -> Option<&str> {
        self.failure.as_deref()
    }
    fn empty(&self, stage: &str, boundary: &str) -> Capture {
        Capture {
            schema_version: 2,
            run: self.run.clone(),
            cycle: self.cycle,
            stage: stage.into(),
            boundary: boundary.into(),
            sequence: self.records.len() as u64 + 1,
            captured_at: MetaStore::now_rfc3339(),
            complete: true,
            error: None,
            cycle_end: None,
            same_state_as: None,
            states: vec![],
            store_binding: self.binding.clone(),
        }
    }
    fn append(&mut self, c: Capture) -> Result<()> {
        let bytes = json(&Envelope {
            sha256: hash(&json(&c)?),
            capture: c.clone(),
        })?;
        let path = self
            .dir
            .join("records")
            .join(format!("{:020}.json", c.sequence));
        if path.exists() {
            return Err("duplicate snapshot sequence".into());
        }
        immutable(&path, &bytes)?;
        self.records.push(c);
        Ok(())
    }
    fn capture(&mut self, graph: &GraphStore, stage: &str, boundary: &str, end: Option<String>) {
        let mut c = self.empty(stage, boundary);
        c.cycle_end = end;
        if let Err(e) = validate_transition(&c, self.records.last()) {
            self.failure = Some(e);
        }
        let prior_failure = self.failure.clone();
        let result = catch_unwind(AssertUnwindSafe(|| {
            if let Some(error) = prior_failure {
                Err(error)
            } else {
                self.collect(graph, &mut c)
            }
        }));
        let error = match result {
            Ok(Ok(())) => None,
            Ok(Err(e)) => Some(e),
            Err(_) => Some("capture panicked".into()),
        };
        if let Some(e) = error {
            c.complete = false;
            c.error = Some(e.clone());
            c.states.clear();
            self.failure = Some(e);
        }
        if c.complete {
            if let Some(prev) = self.records.iter().rev().find(|p| p.complete) {
                if state_bytes(&prev.states).ok() == state_bytes(&c.states).ok() {
                    c.same_state_as = Some(prev.sequence);
                }
            }
        }
        if let Err(e) = self.append(c) {
            self.failure = Some(e);
        }
        if let Some(e) = &self.failure {
            tracing::warn!("historical capture incomplete: {e}");
        }
    }
    fn check_binding(&mut self, graph: &GraphStore, meta: &MetaStore) -> Result<()> {
        let gp = graph.historical_store_path().map_err(|e| e.to_string())?;
        let mp = meta.historical_store_path().map_err(|e| e.to_string())?;
        let expected_graph = self
            .data_dir
            .join(format!("graph-{}.db", self.run.tenant_id))
            .canonicalize()
            .map_err(io)?;
        let expected_meta = self
            .data_dir
            .join(format!("{}.db", self.run.tenant_id))
            .canonicalize()
            .map_err(io)?;
        if meta.tenant_id() != self.run.tenant_id || gp != expected_graph || mp != expected_meta {
            return Err("graph/evidence store binding mismatch".into());
        }
        let binding = StoreBinding {
            graph: file_identity(&gp)?,
            evidence: file_identity(&mp)?,
        };
        if self.binding.as_ref().is_some_and(|b| b != &binding) {
            return Err("bound store identity changed".into());
        }
        self.binding = Some(binding);
        Ok(())
    }
    fn collect(&mut self, graph: &GraphStore, c: &mut Capture) -> Result<()> {
        let meta = MetaStore::open_read_only(&self.data_dir, self.run.tenant_id)
            .map_err(|e| e.to_string())?;
        self.check_binding(graph, &meta)?;
        c.store_binding = self.binding.clone();
        let (entities, relations) = graph.historical_records().map_err(|e| e.to_string())?;
        // all_chunks is strict: malformed JSON fails instead of becoming missing.
        let chunks: BTreeMap<_, _> = meta
            .all_chunks()
            .map_err(|e| e.to_string())?
            .into_iter()
            .collect();
        let mut positions = BTreeMap::new();
        for (&id, chunk) in &chunks {
            if positions
                .insert((chunk.doc_name.clone(), chunk.chunk_index), id)
                .is_some()
            {
                return Err("ambiguous document chunk position".into());
            }
        }
        let names: BTreeMap<_, _> = entities
            .iter()
            .map(|n| (entity_ref(n.id), n.name.clone()))
            .collect();
        let mut facts = Snapshot::new();
        let mut insert = |f: Fact| {
            facts
                .entry(f.claim.id.clone())
                .and_modify(|v| v.evidence.extend(&f.evidence))
                .or_insert(f);
        };
        for n in entities {
            for (field, value) in n.fields {
                if !value.value.trim().is_empty() {
                    insert(Fact::new(
                        FactKey::Field {
                            entity: n.id,
                            field,
                            value: value.value,
                        },
                        value.evidence_chunk_ids,
                    ));
                }
            }
        }
        let mut contexts: BTreeMap<String, BTreeSet<(u8, Vec<i64>)>> = BTreeMap::new();
        for r in relations {
            let f = Fact::new(
                FactKey::Relation {
                    src: r.src_id,
                    relation: r.relation_type,
                    dst: r.dst_id,
                },
                r.evidence_chunk_ids,
            );
            contexts
                .entry(f.claim.id.clone())
                .or_default()
                .insert((r.evidence_window, f.evidence.iter().copied().collect()));
            insert(f);
        }
        for f in facts.into_values() {
            let mut s = State {
                occurrence_id: String::new(),
                claim: f.claim,
                bindings: BTreeMap::new(),
                evidence: vec![],
                extraction_contexts: vec![],
            };
            for key in [&s.claim.subject, &s.claim.object] {
                if let Some(name) = names.get(key) {
                    s.bindings.insert(key.clone(), name.clone());
                }
            }
            for id in &f.evidence {
                s.evidence.push(self.retain_evidence(*id, chunks.get(id))?);
            }
            for (window, direct) in contexts.remove(&s.claim.id).unwrap_or_default() {
                let mut expanded = BTreeSet::new();
                let mut missing_positions = BTreeSet::new();
                let mut unresolved_centres = Vec::new();
                for id in &direct {
                    let Some(centre) = chunks.get(id) else {
                        unresolved_centres.push(*id);
                        continue;
                    };
                    expanded.insert(*id);
                    // Retained indexed window, clipped to observed document extent.
                    // Unknown pre-capture deletions and batch prompts are not inferred.
                    let max = positions
                        .keys()
                        .filter(|(d, _)| d == &centre.doc_name)
                        .map(|(_, i)| *i)
                        .max()
                        .unwrap_or(centre.chunk_index);
                    for index in centre.chunk_index.saturating_sub(window as u32)
                        ..=centre.chunk_index.saturating_add(window as u32).min(max)
                    {
                        match positions.get(&(centre.doc_name.clone(), index)) {
                            Some(other)
                                if centre
                                    .section_type
                                    .same_window_zone(&chunks[other].section_type) =>
                            {
                                expanded.insert(*other);
                            }
                            Some(_) => {}
                            None => {
                                missing_positions.insert(MissingPosition {
                                    document_id: centre.doc_name.clone(),
                                    chunk_index: index,
                                });
                            }
                        }
                    }
                }
                s.extraction_contexts.push(ExtractionContext {
                    evidence_window: window,
                    direct_chunk_ids: direct,
                    expanded: expanded
                        .into_iter()
                        .map(|id| self.retain_evidence(id, chunks.get(&id)))
                        .collect::<Result<_>>()?,
                    unresolved_centres,
                    missing_positions: missing_positions.into_iter().collect(),
                });
            }
            s.occurrence_id = occurrence_id(c, &s)?;
            c.states.push(s);
        }
        self.check_binding(graph, &meta)?;
        validate_capture(c)
    }
    fn retain_evidence(
        &self,
        id: i64,
        chunk: Option<&kwaai_rag::meta_store::ChunkMeta>,
    ) -> Result<Evidence> {
        Ok(if let Some(chunk) = chunk {
            let h = hash(chunk.text.as_bytes());
            immutable(&self.dir.join("blobs").join(&h), chunk.text.as_bytes())?;
            Evidence {
                reference: EvidenceRef::new(
                    EvidenceKind::Original,
                    format!("kb:{}", self.run.tenant_id),
                    &h,
                    Some(id),
                    vec![],
                ),
                document_id: Some(chunk.doc_name.clone()),
                chunk_index: Some(chunk.chunk_index),
                section_type: Some(chunk.section_type.clone()),
                blob: Some(format!("blobs/{h}")),
                retrieval: "available".into(),
            }
        } else {
            Evidence {
                reference: EvidenceRef::new(
                    EvidenceKind::Derived,
                    format!("kb:{}", self.run.tenant_id),
                    "",
                    Some(id),
                    vec![],
                ),
                document_id: None,
                chunk_index: None,
                section_type: None,
                blob: None,
                retrieval: "missing".into(),
            }
        })
    }
}
impl DreamObserver for HistoricalObserver {
    fn cycle_started(&mut self, g: &GraphStore, meta: &MetaStore) {
        if self.active {
            self.failure = Some("duplicate cycle start".into());
        }
        if let Err(e) = self.check_binding(g, meta) {
            self.failure = Some(e);
        }
        self.active = true;
        self.capture(g, "start", "cycle_start", None);
    }
    fn stage_finished(&mut self, stage: DreamStage<'_>, g: &GraphStore) {
        if !self.active {
            self.failure = Some("stage outside cycle".into());
        }
        let name = match stage {
            DreamStage::Completion => "completion",
            DreamStage::Merge { .. } => "merge",
            DreamStage::Prune => "prune",
            DreamStage::Sanitize => "sanitize",
        };
        self.capture(g, name, "post_stage", None);
    }
    fn cycle_finished(&mut self, g: &GraphStore, end: CycleEnd) {
        if !self.active {
            self.failure = Some("finish outside cycle".into());
        }
        self.capture(g, "finish", "cycle_finish", Some(format!("{end:?}")));
        self.active = false;
        self.cycle += 1;
    }
}
impl Drop for HistoricalObserver {
    fn drop(&mut self) {
        let _ = fs::remove_file(self.dir.join("writer.lock"));
    }
}

/// One state per Phase 6 evaluation scope. Keep this envelope for attribution.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Export {
    pub schema_version: u32,
    pub capture: Capture,
    pub occurrence_id: String,
    pub request: VerificationRequest,
}
/// Only exports validated, complete records; preserves Phase 4 native IDs.
pub fn export(dir: &Path) -> Result<Vec<Export>> {
    let mut out = vec![];
    for c in read_archive(dir)? {
        if c.complete {
            for s in &c.states {
                out.push(Export {
                    schema_version: 2,
                    capture: c.clone(),
                    occurrence_id: s.occurrence_id.clone(),
                    request: verification_request(&c, s),
                });
            }
        }
    }
    Ok(out)
}
/// Canonical per-state request. Attribution size is independent of other states.
pub fn verification_request(c: &Capture, s: &State) -> VerificationRequest {
    let evidence: BTreeMap<_, _> = all_evidence(s)
        .map(|e| (e.reference.chunk_id, e.reference.clone()))
        .collect();
    let mut evidence: Vec<_> = evidence.into_values().collect();
    // Explicit availability markers, not invented passage identities. These
    // make raw Phase 4 verification fail closed too, without a new verifier rule.
    let missing: BTreeSet<_> = s
        .extraction_contexts
        .iter()
        .flat_map(|x| &x.missing_positions)
        .collect();
    for position in missing {
        let source = format!(
            "missing-window:{}",
            serde_json::to_string(&(c.run.tenant_id, &position.document_id, position.chunk_index))
                .expect("serializable position")
        );
        evidence.push(EvidenceRef::new(
            EvidenceKind::Derived,
            source,
            "",
            None,
            vec![],
        ));
    }
    VerificationRequest {
        claim: s.claim.clone(),
        entity_bindings: s.bindings.clone(),
        evidence,
        metadata: BTreeMap::from([
            ("state_occurrence_id".into(), s.occurrence_id.clone()),
            (
                "evidence_scope".into(),
                "direct_and_indexed_extraction_window".into(),
            ),
            ("execution_identity_authenticated".into(), "false".into()),
        ]),
    }
}
/// Validate a standalone export's full capture and canonical request.
/// Does not establish on-disk blob integrity or experiment completeness.
pub fn validate_export(export: &Export) -> Result<&State> {
    validate_capture(&export.capture)?;
    if export.schema_version != 2 || !export.capture.complete {
        return Err("unsupported/incomplete export".into());
    }
    let state = export
        .capture
        .states
        .iter()
        .find(|s| s.occurrence_id == export.occurrence_id)
        .ok_or("state missing from capture")?;
    if json(&export.request)? != json(&verification_request(&export.capture, state))? {
        return Err("request/state mismatch".into());
    }
    Ok(state)
}
/// Validate retained source bytes, identities, prefix order and failure semantics.
pub fn validate_assessment(export: &Export, retained: &HistoricalAssessment) -> Result<()> {
    let state = validate_export(export)?;
    let a = &retained.assessment;
    if retained.schema_version != 1
        || retained.occurrence_id != export.occurrence_id
        || retained.request_sha256 != request_hash(&export.request)?
        || a.verification_schema_version != 1
        || a.result.claim_id != state.claim.id
        || a.result.evidence_ids
            != export
                .request
                .evidence
                .iter()
                .map(|r| r.id.clone())
                .collect::<Vec<_>>()
    {
        return Err("assessment occurrence/request mismatch".into());
    }
    if a.failure.is_some() && a.result.status != kwaai_provenance::SupportStatus::Unresolved {
        return Err("failed assessment has resolved judgment".into());
    }
    if a.sources.len() > export.request.evidence.len() {
        return Err("extra assessment sources".into());
    }
    for (source, reference) in a.sources.iter().zip(&export.request.evidence) {
        let ev = all_evidence(state)
            .find(|e| &e.reference == reference)
            .ok_or("source outside state")?;
        if &source.reference != reference
            || reference.kind != EvidenceKind::Original
            || source.passage.source_id != reference.source_id
            || Some(&source.passage.document_id) != ev.document_id.as_ref()
            || source.passage.text.trim().is_empty()
            || hash(source.passage.text.as_bytes()) != reference.content_hash
            || source.retrieved_hash != reference.content_hash
        {
            return Err("assessment source mismatch".into());
        }
    }
    if a.failure.is_none()
        && (a.sources.len() != export.request.evidence.len()
            || a.sources.is_empty()
            || has_missing_context(state))
    {
        return Err("successful verification without complete original evidence".into());
    }
    Ok(())
}
fn has_missing_context(state: &State) -> bool {
    state
        .extraction_contexts
        .iter()
        .any(|x| !x.unresolved_centres.is_empty() || !x.missing_positions.is_empty())
}
/// Resolver reads only archived bytes; never falls back to the live graph/store.
pub struct ArchivedResolver {
    dir: PathBuf,
    evidence: BTreeMap<String, Evidence>,
}
impl ArchivedResolver {
    /// Scope resolution to a single validated historical state, including its
    /// contemporaneous document identity even if a later state reuses bytes.
    pub fn for_state(dir: &Path, occurrence: &str) -> Result<Self> {
        for c in read_archive(dir)? {
            if let Some(s) = c.states.into_iter().find(|s| s.occurrence_id == occurrence) {
                return Ok(Self {
                    dir: dir.into(),
                    evidence: all_evidence(&s)
                        .cloned()
                        .map(|e| (e.reference.id.clone(), e))
                        .collect(),
                });
            }
        }
        Err("unknown historical state".into())
    }
    /// Whole-archive resolver; refuses ambiguous document bindings. Prefer for_state.
    pub fn open(dir: &Path) -> Result<Self> {
        let mut evidence = BTreeMap::new();
        for c in read_archive(dir)? {
            for s in c.states {
                for e in all_evidence(&s).cloned() {
                    if let Some(old) = evidence.insert(e.reference.id.clone(), e.clone()) {
                        if old != e {
                            return Err("ambiguous archived evidence identity".into());
                        }
                    }
                }
            }
        }
        Ok(Self {
            dir: dir.into(),
            evidence,
        })
    }
}
impl EvidenceResolver for ArchivedResolver {
    fn resolve(&self, r: &EvidenceRef) -> Result<Option<SourcePassage>> {
        let Some(e) = self.evidence.get(&r.id) else {
            return Ok(None);
        };
        if e.reference != *r {
            return Err("reference mismatch".into());
        }
        let Some(blob) = &e.blob else {
            return Ok(None);
        };
        let bytes = fs::read(self.dir.join(blob)).map_err(io)?;
        if hash(&bytes) != r.content_hash {
            return Err("blob hash mismatch".into());
        }
        Ok(Some(SourcePassage {
            source_id: r.source_id.clone(),
            document_id: e.document_id.clone().ok_or("missing document")?,
            text: String::from_utf8(bytes).map_err(|e| e.to_string())?,
        }))
    }
}

/// A retained Phase 4 judgment bound to the exact exported occurrence/request.
/// Hash binding is integrity, not authentication or independent ground truth.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HistoricalAssessment {
    pub schema_version: u32,
    pub occurrence_id: String,
    pub request_sha256: String,
    pub assessment: kwaai_provenance::verification::EvidenceVerification,
}
/// Stable compact-JSON fingerprint of the complete Phase 4 request.
pub fn request_hash(request: &VerificationRequest) -> Result<String> {
    Ok(hash(&json(request)?))
}
/// Verify an exported request using only preserved evidence and bind the result
/// to this occurrence. No proposals, enforcement, or graph mutations.
pub async fn verify_state(
    dir: &Path,
    export: &Export,
    backend: &dyn kwaai_provenance::verification::EvidenceBackend,
    max_input_bytes: usize,
) -> Result<HistoricalAssessment> {
    let retained = self::export(dir)?
        .into_iter()
        .find(|e| e.occurrence_id == export.occurrence_id)
        .ok_or("unknown historical export")?;
    if json(&retained)? != json(export)? {
        return Err("export/archive mismatch".into());
    }
    let resolver = ArchivedResolver::for_state(dir, &export.occurrence_id)?;
    validate_export(export)?;
    let assessment = kwaai_provenance::verification::verify(
        &export.request,
        &resolver,
        backend,
        max_input_bytes,
    )
    .await;
    Ok(HistoricalAssessment {
        schema_version: 1,
        occurrence_id: export.occurrence_id.clone(),
        request_sha256: request_hash(&export.request)?,
        assessment,
    })
}

/// Externally retain this identity to detect loss of a complete trailing suffix.
/// Binds run bytes and every published record name/content, in sorted order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ArchiveHead {
    pub record_count: u64,
    pub sha256: String,
}
pub fn archive_head(dir: &Path) -> Result<ArchiveHead> {
    let mut framed = b"kwaai.history.archive-head.v2\0".to_vec();
    framed.extend(json(&hash(&fs::read(dir.join("run.json")).map_err(io)?))?);
    let mut records = fs::read_dir(dir.join("records"))
        .map_err(io)?
        .map(|p| p.map(|p| p.path()).map_err(io))
        .collect::<Result<Vec<_>>>()?;
    records.retain(|p| p.extension().is_some_and(|e| e == "json"));
    records.sort();
    for path in &records {
        let name = path
            .file_name()
            .and_then(|s| s.to_str())
            .ok_or("non-UTF8 record name")?;
        framed.extend(json(&(name, hash(&fs::read(path).map_err(io)?)))?);
    }
    Ok(ArchiveHead {
        record_count: records.len() as u64,
        sha256: hash(&framed),
    })
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CycleExpectation {
    pub cycle: u64,
    /// Completed, NothingToDo, or SkippedDestructiveStages; declared before analysis.
    pub cycle_end: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExperimentExpectation {
    pub schema_version: u32,
    pub cycles: Vec<CycleExpectation>,
    pub expected_archive_head: Option<ArchiveHead>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CycleCompleteness {
    pub cycle: u64,
    pub expected_boundaries: Vec<String>,
    pub observed_boundaries: Vec<String>,
    pub missing_boundaries: Vec<String>,
    pub lifecycle_complete: bool,
    pub captures_complete: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MissingOriginal {
    pub reason: String,
    pub sequence: u64,
    pub occurrence_id: String,
    pub chunk_id: Option<i64>,
    pub document_id: Option<String>,
    pub chunk_index: Option<u32>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CompletenessReport {
    pub schema_version: u32,
    pub archive_structurally_valid: bool,
    pub structural_error: Option<String>,
    pub lifecycle_valid: bool,
    pub expected_cycles: Vec<u64>,
    pub observed_cycles: Vec<u64>,
    pub cycles: Vec<CycleCompleteness>,
    pub incomplete_captures: Vec<u64>,
    pub missing_original_evidence: Vec<MissingOriginal>,
    pub unexpected_or_contradictory_events: Vec<String>,
    pub archive_head: Option<ArchiveHead>,
    pub expected_archive_head: Option<ArchiveHead>,
    pub archive_head_matches: Option<bool>,
    pub eligible_for_factual_support_trajectory_analysis: bool,
}
fn expected_boundaries(end: &str) -> Result<Vec<String>> {
    let stages: &[&str] = match end {
        "Completed" => &[
            "start",
            "completion",
            "merge",
            "prune",
            "sanitize",
            "finish:Completed",
        ],
        "NothingToDo" => &["start", "finish:NothingToDo"],
        "SkippedDestructiveStages" => &["start", "completion", "finish:SkippedDestructiveStages"],
        _ => return Err("invalid expected terminal reason".into()),
    };
    Ok(stages.iter().map(|s| s.to_string()).collect())
}
fn boundary_label(c: &Capture) -> String {
    match c.cycle_end.as_deref() {
        Some(end) => format!("{}:{end}", c.stage),
        None => c.stage.clone(),
    }
}
/// Parses a caller-supplied experiment plan; structural validity alone never
/// implies completeness. Missing originals fail eligibility conservatively.
/// Run with the writer quiescent: this is not an atomic concurrent-writer audit.
pub fn completeness_report(
    dir: &Path,
    expected: &ExperimentExpectation,
) -> Result<CompletenessReport> {
    if expected.schema_version != 1
        || expected.cycles.is_empty()
        || expected
            .cycles
            .iter()
            .enumerate()
            .any(|(i, c)| c.cycle != i as u64 + 1)
    {
        return Err("expected cycles must be nonempty and consecutive from 1".into());
    }
    for c in &expected.cycles {
        expected_boundaries(&c.cycle_end)?;
    }
    let head = archive_head(dir).ok();
    let head_matches = expected
        .expected_archive_head
        .as_ref()
        .map(|e| Some(e) == head.as_ref());
    let mut report = CompletenessReport {
        schema_version: 1,
        archive_structurally_valid: false,
        structural_error: None,
        lifecycle_valid: false,
        expected_cycles: expected.cycles.iter().map(|c| c.cycle).collect(),
        observed_cycles: vec![],
        cycles: vec![],
        incomplete_captures: vec![],
        missing_original_evidence: vec![],
        unexpected_or_contradictory_events: vec![],
        archive_head: head,
        expected_archive_head: expected.expected_archive_head.clone(),
        archive_head_matches: head_matches,
        eligible_for_factual_support_trajectory_analysis: false,
    };
    let records = match read_structural_archive(dir) {
        Ok(records) => {
            report.archive_structurally_valid = true;
            records
        }
        Err(error) => {
            report.structural_error = Some(error);
            for c in &expected.cycles {
                let boundaries = expected_boundaries(&c.cycle_end)?;
                report.cycles.push(CycleCompleteness {
                    cycle: c.cycle,
                    expected_boundaries: boundaries.clone(),
                    observed_boundaries: vec![],
                    missing_boundaries: boundaries,
                    lifecycle_complete: false,
                    captures_complete: false,
                });
            }
            return Ok(report);
        }
    };
    report.lifecycle_valid = true;
    for (i, c) in records.iter().enumerate() {
        if let Err(e) = validate_transition(c, i.checked_sub(1).map(|j| &records[j])) {
            report.lifecycle_valid = false;
            report
                .unexpected_or_contradictory_events
                .push(format!("sequence {} cycle {}: {e}", c.sequence, c.cycle));
        }
        if !c.complete {
            report.incomplete_captures.push(c.sequence);
        }
        for s in &c.states {
            if all_evidence(s).next().is_none() {
                report.missing_original_evidence.push(MissingOriginal {
                    reason: "no_evidence_associations".into(),
                    sequence: c.sequence,
                    occurrence_id: s.occurrence_id.clone(),
                    chunk_id: None,
                    document_id: None,
                    chunk_index: None,
                });
            }
            let mut seen = BTreeSet::new();
            for e in all_evidence(s) {
                if e.retrieval == "missing" && seen.insert(e.reference.id.clone()) {
                    report.missing_original_evidence.push(MissingOriginal {
                        reason: "missing_direct_reference".into(),
                        sequence: c.sequence,
                        occurrence_id: s.occurrence_id.clone(),
                        chunk_id: e.reference.chunk_id,
                        document_id: e.document_id.clone(),
                        chunk_index: e.chunk_index,
                    });
                }
            }
            for p in s
                .extraction_contexts
                .iter()
                .flat_map(|x| &x.missing_positions)
                .collect::<BTreeSet<_>>()
            {
                report.missing_original_evidence.push(MissingOriginal {
                    reason: "missing_window_position".into(),
                    sequence: c.sequence,
                    occurrence_id: s.occurrence_id.clone(),
                    chunk_id: None,
                    document_id: Some(p.document_id.clone()),
                    chunk_index: Some(p.chunk_index),
                });
            }
        }
    }
    let observed: BTreeSet<_> = records.iter().map(|c| c.cycle).collect();
    report.observed_cycles = observed.iter().copied().collect();
    for cycle in observed
        .iter()
        .copied()
        .chain(report.expected_cycles.iter().copied())
        .collect::<BTreeSet<_>>()
    {
        let plan = expected.cycles.iter().find(|c| c.cycle == cycle);
        let boundaries = plan
            .map(|p| expected_boundaries(&p.cycle_end))
            .transpose()?
            .unwrap_or_default();
        let captures: Vec<_> = records.iter().filter(|c| c.cycle == cycle).collect();
        let actual: Vec<_> = captures.iter().map(|c| boundary_label(c)).collect();
        let missing = boundaries
            .iter()
            .filter(|b| !actual.contains(b))
            .cloned()
            .collect();
        let lifecycle_complete = plan.is_some() && boundaries == actual;
        if !lifecycle_complete {
            report.unexpected_or_contradictory_events.push(format!(
                "cycle {cycle}: expected {boundaries:?}, observed {actual:?}"
            ));
        }
        report.cycles.push(CycleCompleteness {
            cycle,
            expected_boundaries: boundaries,
            observed_boundaries: actual,
            missing_boundaries: missing,
            lifecycle_complete,
            captures_complete: !captures.is_empty() && captures.iter().all(|c| c.complete),
        });
    }
    if head_matches == Some(false) {
        report
            .unexpected_or_contradictory_events
            .push("archive head differs from externally retained expected head".into());
    }
    report.eligible_for_factual_support_trajectory_analysis = report.archive_structurally_valid
        && report.lifecycle_valid
        && report
            .cycles
            .iter()
            .all(|c| c.lifecycle_complete && c.captures_complete)
        && report.missing_original_evidence.is_empty()
        && report.unexpected_or_contradictory_events.is_empty()
        && report.archive_head.is_some()
        && head_matches != Some(false);
    Ok(report)
}
