//! Offline, observe-only evaluation. No graph or inference dependencies.
mod identity;
pub mod phase5;
pub use identity::{implementation_identity, EvaluatorIdentity};

use anyhow::{bail, ensure, Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::{Read, Write},
    path::{Component, Path},
    process::Command,
    time::Instant,
};
pub const SCHEMA: u32 = 1;
/// Run/checkpoint v3 retains imported experimental attribution; data schemas stay v1.
pub const RUN_SCHEMA: u32 = 3;
pub fn hash(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}
pub fn object_hash<T: Serialize>(value: &T) -> Result<String> {
    Ok(hash(&serde_json::to_vec(value)?))
}
fn nonempty(s: &str) -> bool {
    !s.trim().is_empty()
}
fn digest(s: &str) -> bool {
    s.len() == 64
        && s.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
fn safe_path(s: &str) -> bool {
    !s.is_empty()
        && Path::new(s)
            .components()
            .all(|c| matches!(c, Component::Normal(_)))
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Document {
    pub id: String,
    pub path: String,
    pub sha256: String,
    pub bytes: u64,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub schema_version: u32,
    pub corpus_version: String,
    pub documents: Vec<Document>,
}
impl Manifest {
    pub fn inventory(root: &Path) -> Result<Self> {
        fn walk(root: &Path, dir: &Path, docs: &mut Vec<Document>) -> Result<()> {
            for entry in fs::read_dir(dir)? {
                let entry = entry?;
                let kind = entry.file_type()?;
                ensure!(!kind.is_symlink(), "symlinks are not corpus documents");
                if kind.is_dir() {
                    walk(root, &entry.path(), docs)?;
                } else if kind.is_file() {
                    let path = entry
                        .path()
                        .strip_prefix(root)?
                        .to_str()
                        .context("non UTF-8 path")?
                        .replace('\\', "/");
                    #[cfg(unix)]
                    ensure!(
                        entry
                            .file_name()
                            .to_str()
                            .is_some_and(|s| !s.contains('\\')),
                        "literal backslash in corpus path"
                    );
                    ensure!(safe_path(&path), "unsafe document path");
                    let mut file = fs::File::open(entry.path())?;
                    let mut hasher = Sha256::new();
                    let mut buffer = [0u8; 65536];
                    let mut bytes = 0u64;
                    loop {
                        let n = file.read(&mut buffer)?;
                        if n == 0 {
                            break;
                        }
                        hasher.update(&buffer[..n]);
                        bytes += n as u64;
                    }
                    docs.push(Document {
                        id: format!("doc:{}", hash(path.as_bytes())),
                        path,
                        sha256: hex::encode(hasher.finalize()),
                        bytes,
                    });
                } else {
                    bail!("nonregular corpus entry");
                }
            }
            Ok(())
        }
        ensure!(
            !fs::symlink_metadata(root)?.file_type().is_symlink(),
            "symlink corpus root"
        );
        let mut documents = Vec::new();
        walk(root, root, &mut documents)?;
        documents.sort_by(|a, b| a.path.cmp(&b.path));
        let corpus_version = object_hash(&documents)?;
        Ok(Self {
            schema_version: SCHEMA,
            corpus_version,
            documents,
        })
    }
    pub fn validate(&self) -> Result<()> {
        ensure!(self.schema_version == SCHEMA, "unsupported manifest schema");
        ensure!(
            object_hash(&self.documents)? == self.corpus_version,
            "corpus version mismatch"
        );
        let mut paths = BTreeSet::new();
        for d in &self.documents {
            ensure!(safe_path(&d.path) && digest(&d.sha256), "invalid document");
            ensure!(
                d.id == format!("doc:{}", hash(d.path.as_bytes())),
                "invalid document identity"
            );
            ensure!(paths.insert(&d.path), "duplicate document");
        }
        ensure!(
            self.documents.windows(2).all(|w| w[0].path < w[1].path),
            "manifest must be sorted"
        );
        Ok(())
    }
    pub fn check(&self, root: &Path) -> Result<()> {
        self.validate()?;
        ensure!(Self::inventory(root)? == *self, "corpus integrity mismatch");
        Ok(())
    }
    /// Hash-ranked sampling, versioned by schema; independent of platform RNG implementations.
    pub fn sample(&self, seed: u64, count: usize) -> Result<Vec<String>> {
        self.validate()?;
        ensure!(count <= self.documents.len(), "sample exceeds corpus");
        let mut ranked: Vec<_> = self
            .documents
            .iter()
            .map(|d| {
                (
                    hash(format!("sample-v1:{seed}:{}", d.id).as_bytes()),
                    d.id.clone(),
                )
            })
            .collect();
        ranked.sort();
        Ok(ranked.into_iter().take(count).map(|(_, id)| id).collect())
    }
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Label {
    Supported,
    Unsupported,
    Unresolved,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Evidence {
    pub document_id: String,
    pub sha256: String,
    pub locator: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Annotation {
    pub schema_version: u32,
    pub annotation_id: String,
    pub claim_id: String,
    pub claim_content: String,
    pub evidence: Vec<Evidence>,
    pub label: Label,
    pub rationale: String,
    pub annotator_id: String,
    pub timestamp: DateTime<Utc>,
    pub corpus_version: String,
    pub origin: AnnotationOrigin,
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AnnotationOrigin {
    Independent,
    Verifier,
}
pub fn validate_annotations(annotations: &[Annotation], manifest: &Manifest) -> Result<()> {
    manifest.validate()?;
    let mut ids = BTreeSet::new();
    let mut claims = BTreeMap::new();
    for a in annotations {
        ensure!(
            a.schema_version == SCHEMA && a.corpus_version == manifest.corpus_version,
            "annotation schema/corpus mismatch"
        );
        ensure!(
            [
                &a.annotation_id,
                &a.claim_id,
                &a.claim_content,
                &a.rationale,
                &a.annotator_id
            ]
            .iter()
            .all(|s| nonempty(s)),
            "empty annotation field"
        );
        ensure!(
            ids.insert(&a.annotation_id),
            "duplicate annotation id; append disagreements with distinct IDs"
        );
        if let Some(content) = claims.insert(&a.claim_id, &a.claim_content) {
            ensure!(
                content == &a.claim_content,
                "claim identity/content conflict"
            );
        }
        for e in &a.evidence {
            validate_evidence(e, manifest)?;
        }
        ensure!(
            a.label != Label::Supported || !a.evidence.is_empty(),
            "supported annotation requires evidence"
        );
    }
    Ok(())
}
fn validate_evidence(e: &Evidence, m: &Manifest) -> Result<()> {
    ensure!(nonempty(&e.locator), "empty evidence locator");
    ensure!(
        m.documents
            .iter()
            .any(|d| d.id == e.document_id && d.sha256 == e.sha256),
        "invalid evidence reference/hash"
    );
    Ok(())
}
/// Phase 6 normalized contract; actual Phase 5 imports use `phase5::Bundle`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DecisionImport {
    pub adapter_version: u32,
    pub producer_schema: String,
    pub corpus_version: String,
    pub model_identifier: String,
    pub seed: u64,
    pub cycle: u32,
    pub repetition: u32,
    pub records: Vec<Observation>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Observation {
    pub claim_id: String,
    pub claim_content: String,
    pub evidence: Vec<Evidence>,
    pub judgment: Option<Label>,
    pub traceable: Option<bool>,
    pub authenticated: Option<bool>,
    /// Auditable method and retained record identity supplied by the adapter.
    pub authentication_basis: Option<String>,
    pub verification_ms: Option<u64>,
    pub baseline_ms: Option<u64>,
}
pub trait DecisionAdapter {
    fn normalize(&self, raw: &[u8]) -> Result<DecisionImport>;
}
pub struct FixtureAdapter;
impl DecisionAdapter for FixtureAdapter {
    fn normalize(&self, raw: &[u8]) -> Result<DecisionImport> {
        let v: DecisionImport = serde_json::from_slice(raw)?;
        ensure!(
            v.adapter_version == SCHEMA && nonempty(&v.producer_schema),
            "unsupported adapter contract"
        );
        Ok(v)
    }
}
/// Hypothetical analysis only. Never exported to a production gate.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum HypotheticalPolicy {
    Baseline,
    Allow,
    RejectUnsupported,
    RequireSupported,
}
impl HypotheticalPolicy {
    pub fn accepts(self, label: Option<Label>) -> bool {
        match self {
            Self::Baseline | Self::Allow => true,
            Self::RejectUnsupported => label != Some(Label::Unsupported),
            Self::RequireSupported => label == Some(Label::Supported),
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Rate {
    pub numerator: u64,
    pub denominator: u64,
    pub estimate: f64,
    pub wilson_95: [f64; 2],
}
fn rate(k: u64, n: u64) -> Option<Rate> {
    if n == 0 {
        return None;
    }
    let p = k as f64 / n as f64;
    let z = 1.959963984540054;
    let n_f = n as f64;
    let den = 1.0 + z * z / n_f;
    let center = (p + z * z / (2.0 * n_f)) / den;
    let half = z * ((p * (1.0 - p) / n_f + z * z / (4.0 * n_f * n_f)).sqrt()) / den;
    Some(Rate {
        numerator: k,
        denominator: n,
        estimate: p,
        wilson_95: [(center - half).max(0.0), (center + half).min(1.0)],
    })
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Metrics {
    pub observations: usize,
    pub missing_ground_truth: usize,
    pub conflicting_ground_truth: usize,
    #[serde(deserialize_with = "required_option")]
    pub supported_fact_rate: Option<Rate>,
    #[serde(deserialize_with = "required_option")]
    pub unsupported_fact_rate: Option<Rate>,
    #[serde(deserialize_with = "required_option")]
    pub source_traceability: Option<Rate>,
    #[serde(deserialize_with = "required_option")]
    pub provenance_authentication_rate: Option<Rate>,
    #[serde(deserialize_with = "required_option")]
    pub false_rejection_rate: Option<Rate>,
    #[serde(deserialize_with = "required_option")]
    pub false_acceptance_rate: Option<Rate>,
    #[serde(deserialize_with = "required_option")]
    pub precision: Option<Rate>,
    #[serde(deserialize_with = "required_option")]
    pub recall: Option<Rate>,
    #[serde(deserialize_with = "required_option")]
    pub runtime_overhead_ratio: Option<f64>,
}
pub fn evaluate(
    obs: &[Observation],
    annotations: &[Annotation],
    m: &Manifest,
    policy: HypotheticalPolicy,
) -> Result<Metrics> {
    validate_annotations(annotations, m)?;
    let mut seen = BTreeSet::new();
    let (mut missing, mut conflicts, mut s, mut u, mut tp, mut fp, mut tn, mut fn_) =
        (0, 0, 0, 0, 0, 0, 0, 0);
    let (mut trace, mut traced, mut auth, mut authed) = (0, 0, 0, 0);
    let (mut overhead, mut base, mut timed) = (0u128, 0u128, 0usize);
    for o in obs {
        ensure!(
            nonempty(&o.claim_id) && nonempty(&o.claim_content) && seen.insert(&o.claim_id),
            "empty/duplicate observation"
        );
        let valid =
            !o.evidence.is_empty() && o.evidence.iter().all(|e| validate_evidence(e, m).is_ok());
        ensure!(
            o.traceable != Some(true) || valid,
            "traceability assertion with invalid evidence"
        );
        ensure!(
            o.authenticated != Some(true) || valid,
            "authentication assertion with invalid provenance"
        );
        if let Some(v) = o.traceable {
            trace += 1;
            traced += u64::from(v);
        }
        ensure!(
            o.authenticated.is_none()
                || o.authentication_basis.as_ref().is_some_and(|s| nonempty(s)),
            "authentication observation requires an audit basis"
        );
        if let Some(v) = o.authenticated {
            auth += 1;
            authed += u64::from(v);
        }
        if let (Some(v), Some(b)) = (o.verification_ms, o.baseline_ms) {
            overhead += v as u128;
            base += b as u128;
            timed += 1;
        }
        let labels: Vec<_> = annotations
            .iter()
            .filter(|a| a.claim_id == o.claim_id && a.origin == AnnotationOrigin::Independent)
            .collect();
        for a in &labels {
            ensure!(
                a.claim_content == o.claim_content,
                "annotation/observation content mismatch"
            );
        }
        if labels.is_empty() {
            missing += 1;
            continue;
        }
        if labels.iter().any(|a| a.label != labels[0].label) {
            conflicts += 1;
            continue;
        }
        let accepted = policy.accepts(o.judgment);
        match labels[0].label {
            Label::Supported => {
                s += 1;
                if accepted {
                    tp += 1
                } else {
                    fn_ += 1
                }
            }
            Label::Unsupported => {
                u += 1;
                if accepted {
                    fp += 1
                } else {
                    tn += 1
                }
            }
            Label::Unresolved => {
                missing += 1;
            }
        }
    }
    Ok(Metrics {
        observations: obs.len(),
        missing_ground_truth: missing,
        conflicting_ground_truth: conflicts,
        supported_fact_rate: rate(s, s + u),
        unsupported_fact_rate: rate(u, s + u),
        source_traceability: rate(traced, trace),
        provenance_authentication_rate: rate(authed, auth),
        false_rejection_rate: rate(fn_, tp + fn_),
        false_acceptance_rate: rate(fp, fp + tn),
        precision: rate(tp, tp + fp),
        recall: rate(tp, tp + fn_),
        runtime_overhead_ratio: if timed == obs.len() && base > 0 {
            Some(overhead as f64 / base as f64)
        } else {
            None
        },
    })
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModelConfig {
    pub identifier: String,
    pub digest: Option<String>,
    pub parameters: BTreeMap<String, serde_json::Value>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InputCell {
    pub model: String,
    pub repetition: u32,
    pub cycle: u32,
    pub path: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub schema_version: u32,
    pub run_id: String,
    pub models: Vec<ModelConfig>,
    pub manifest: String,
    pub corpus_root: String,
    pub sample_count: usize,
    pub seeds: Vec<u64>,
    pub cycles: u32,
    pub repetitions: u32,
    pub verification: BTreeMap<String, serde_json::Value>,
    pub policies: Vec<HypotheticalPolicy>,
    pub annotations: String,
    pub inputs: Vec<InputCell>,
    pub output_dir: String,
}
pub fn read_json<T: serde::de::DeserializeOwned>(p: impl AsRef<Path>) -> Result<T> {
    Ok(serde_json::from_slice(&fs::read(p)?)?)
}
/// Exclusive creation: never replaces an existing result.
pub fn write_new(p: impl AsRef<Path>, value: &impl Serialize) -> Result<()> {
    let bytes = serde_json::to_vec_pretty(value)?;
    let mut f = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(p)?;
    f.write_all(&bytes)?;
    f.write_all(b"\n")?;
    f.sync_all()?;
    Ok(())
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RunMetadata {
    pub schema_version: u32,
    pub evaluator: EvaluatorIdentity,
    pub run_id: String,
    pub config_hash: String,
    pub manifest_hash: String,
    pub annotations_hash: String,
    pub input_hashes: BTreeMap<String, String>,
    pub git_commit: Option<String>,
    pub git_status: Option<String>,
    pub started_at: DateTime<Utc>,
    pub hardware: BTreeMap<String, String>,
    pub config: Config,
    pub manifest: Manifest,
    pub selected_documents: Vec<Vec<String>>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CellResult {
    pub phase5_attribution: Option<phase5::Attribution>,
    pub schema_version: u32,
    pub evaluator: EvaluatorIdentity,
    pub config_hash: String,
    pub input_hash: String,
    pub model: String,
    pub repetition: u32,
    pub cycle: u32,
    #[serde(deserialize_with = "unique_policy_metrics")]
    pub metrics: BTreeMap<HypotheticalPolicy, Metrics>,
    pub elapsed_ms: u128,
}
fn git(args: &[&str]) -> Option<String> {
    let o = Command::new("git").args(args).output().ok()?;
    o.status
        .success()
        .then(|| String::from_utf8_lossy(&o.stdout).trim().to_string())
}
/// Replays retained cycle observations; does not execute consolidation or models.
pub fn run(config: Config, resume: bool) -> Result<()> {
    ensure!(
        config.schema_version == SCHEMA
            && safe_path(&config.run_id)
            && !config.run_id.contains('/'),
        "invalid run/schema"
    );
    ensure!(
        config.cycles > 0
            && config.repetitions > 0
            && config.seeds.len() == config.repetitions as usize,
        "invalid trial dimensions"
    );
    ensure!(
        !config.models.is_empty() && !config.policies.is_empty(),
        "missing models/policies"
    );
    let mut names = BTreeSet::new();
    for model in &config.models {
        ensure!(
            nonempty(&model.identifier) && names.insert(&model.identifier),
            "duplicate/empty model"
        );
    }
    ensure!(
        config.policies.iter().collect::<BTreeSet<_>>().len() == config.policies.len(),
        "duplicate policies"
    );
    let m: Manifest = read_json(&config.manifest)?;
    m.check(Path::new(&config.corpus_root))?;
    let annotations: Vec<Annotation> = read_json(&config.annotations)?;
    validate_annotations(&annotations, &m)?;
    let selections: Vec<_> = config
        .seeds
        .iter()
        .map(|s| m.sample(*s, config.sample_count))
        .collect::<Result<_>>()?;
    let mut keys = BTreeSet::new();
    let mut hashes = BTreeMap::new();
    for cell in &config.inputs {
        ensure!(
            names.contains(&cell.model)
                && cell.repetition < config.repetitions
                && (1..=config.cycles).contains(&cell.cycle),
            "invalid input cell"
        );
        ensure!(
            keys.insert((&cell.model, cell.repetition, cell.cycle)),
            "duplicate input cell"
        );
        hashes.insert(cell.path.clone(), hash(&fs::read(&cell.path)?));
    }
    ensure!(
        keys.len() == config.models.len() * config.repetitions as usize * config.cycles as usize,
        "incomplete input matrix"
    );
    let config_hash = object_hash(&config)?;
    let evaluator = implementation_identity();
    let dir = Path::new(&config.output_dir).join(&config.run_id);
    if !resume {
        fs::create_dir_all(&config.output_dir)?;
        fs::create_dir(&dir)?;
    }
    let meta_path = dir.join("run.json");
    if resume {
        let old: RunMetadata = read_json(&meta_path).context(
            "invalid or legacy run metadata; retain this directory and start a new run ID",
        )?;
        ensure!(old.evaluator == evaluator, "evaluator implementation fingerprint mismatch: saved {}, current {}; retain this run and start a new run ID with this implementation", old.evaluator.sha256, evaluator.sha256);
        ensure!(
            old.schema_version == RUN_SCHEMA
                && old.config_hash == config_hash
                && old.manifest_hash == object_hash(&m)?
                && old.annotations_hash == object_hash(&annotations)?
                && old.input_hashes == hashes,
            "resume identity mismatch"
        );
    } else {
        let mut hardware = BTreeMap::new();
        hardware.insert("os".into(), std::env::consts::OS.into());
        hardware.insert("arch".into(), std::env::consts::ARCH.into());
        if let Ok(n) = std::thread::available_parallelism() {
            hardware.insert("logical_cpus".into(), n.to_string());
        }
        if let Ok(cpu) = fs::read_to_string("/proc/cpuinfo") {
            hardware.insert("cpuinfo".into(), cpu);
        }
        write_new(
            &meta_path,
            &RunMetadata {
                schema_version: RUN_SCHEMA,
                evaluator: evaluator.clone(),
                run_id: config.run_id.clone(),
                config_hash: config_hash.clone(),
                manifest_hash: object_hash(&m)?,
                annotations_hash: object_hash(&annotations)?,
                input_hashes: hashes.clone(),
                git_commit: git(&["rev-parse", "HEAD"]),
                git_status: git(&["status", "--porcelain=v1", "--untracked-files=all"]),
                started_at: Utc::now(),
                hardware,
                config: config.clone(),
                manifest: m.clone(),
                selected_documents: selections.clone(),
            },
        )?;
    }
    let mut failures = Vec::new();
    for (index, cell) in config.inputs.iter().enumerate() {
        let output = dir.join(format!("cell-{index:06}.json"));
        let expected = &hashes[&cell.path];
        if output.exists() {
            let saved: CellResult = read_json(&output).with_context(|| format!("{}: partial/corrupt completed cell; retain/quarantine it explicitly before resuming", output.display()))?;
            ensure!(
                resume
                    && saved.schema_version == RUN_SCHEMA
                    && saved.evaluator == evaluator
                    && saved.config_hash == config_hash
                    && &saved.input_hash == expected
                    && saved.model == cell.model
                    && saved.repetition == cell.repetition
                    && saved.cycle == cell.cycle,
                "{}: incompatible completed cell; retain/quarantine it or use a new run ID",
                output.display()
            );
            let policies: BTreeSet<_> = config.policies.iter().copied().collect();
            ensure!(saved.metrics.keys().copied().collect::<BTreeSet<_>>() == policies,
                "{}: saved policy coverage differs from config; retain/quarantine the cell before resuming", output.display());
            let recomputed = cell_metrics(
                &config,
                cell,
                expected,
                &annotations,
                &m,
                &selections[cell.repetition as usize],
            )
            .with_context(|| {
                format!(
                    "{}: cannot revalidate retained observations; no saved cell was accepted",
                    output.display()
                )
            })?;
            ensure!(
                saved.phase5_attribution == recomputed.1,
                "saved Phase 5 attribution mismatch"
            );
            for (policy, expected_metrics) in &recomputed.0 {
                validate_saved_metrics(&saved.metrics[policy], expected_metrics)
                    .with_context(|| format!("{}: invalid saved metrics for {policy:?}; retain/quarantine this cell before resuming", output.display()))?;
            }
            continue;
        }
        let start = Instant::now();
        let result = (|| -> Result<CellResult> {
            let (metrics, phase5_attribution) = cell_metrics(
                &config,
                cell,
                expected,
                &annotations,
                &m,
                &selections[cell.repetition as usize],
            )?;
            Ok(CellResult {
                phase5_attribution,
                schema_version: RUN_SCHEMA,
                evaluator: evaluator.clone(),
                config_hash: config_hash.clone(),
                input_hash: expected.clone(),
                model: cell.model.clone(),
                repetition: cell.repetition,
                cycle: cell.cycle,
                metrics,
                elapsed_ms: start.elapsed().as_millis(),
            })
        })();
        match result {
            Ok(v) => write_new(&output, &v)?,
            Err(e) => failures.push(format!("cell-{index:06}: {e:#}")),
        }
    }
    let report = serde_json::json!({"schema_version":RUN_SCHEMA,"evaluator":evaluator,"timestamp":Utc::now(),"complete":failures.is_empty(),"failures":failures,"mode":"hypothetical_replay"});
    // Each attempt has a distinct report; previous failures remain available for audit.
    let mut attempt = 0;
    while dir.join(format!("attempt-{attempt:06}.json")).exists() {
        attempt += 1;
    }
    write_new(dir.join(format!("attempt-{attempt:06}.json")), &report)?;
    ensure!(
        failures.is_empty(),
        "experiment has failed cells; see attempt report"
    );
    Ok(())
}

type CellMetrics = (
    BTreeMap<HypotheticalPolicy, Metrics>,
    Option<phase5::Attribution>,
);

/// Re-read and hash retained observations for fresh evaluation AND every resumed cell.
fn cell_metrics(
    config: &Config,
    cell: &InputCell,
    expected: &str,
    annotations: &[Annotation],
    m: &Manifest,
    selected: &[String],
) -> Result<CellMetrics> {
    let raw = fs::read(&cell.path)?;
    ensure!(hash(&raw) == expected, "input changed during run");
    let mut attribution = None;
    let imported = if serde_json::from_slice::<serde_json::Value>(&raw)?
        .get("phase5_adapter_version")
        .is_some()
    {
        let result = phase5::import(&raw)?;
        ensure!(
            object_hash(&result.original.sidecar.corpus)? == object_hash(m)?,
            "sidecar corpus mismatch"
        );
        let model = config
            .models
            .iter()
            .find(|m| m.identifier == cell.model)
            .context("missing model")?;
        ensure!(
            object_hash(model)? == object_hash(&result.original.sidecar.model)?,
            "sidecar model mismatch"
        );
        attribution = Some(result.attribution()?);
        result.normalized
    } else {
        FixtureAdapter.normalize(&raw)?
    };
    ensure!(
        imported.corpus_version == m.corpus_version
            && imported.model_identifier == cell.model
            && imported.seed == config.seeds[cell.repetition as usize]
            && imported.cycle == cell.cycle
            && imported.repetition == cell.repetition,
        "import/cell identity mismatch"
    );
    for o in &imported.records {
        ensure!(
            o.evidence.iter().all(|e| selected.contains(&e.document_id)),
            "observation outside selected corpus"
        );
    }
    let claim_ids: BTreeSet<_> = imported.records.iter().map(|o| &o.claim_id).collect();
    for a in annotations
        .iter()
        .filter(|a| a.origin == AnnotationOrigin::Independent && claim_ids.contains(&a.claim_id))
    {
        ensure!(
            a.evidence.iter().all(|e| selected.contains(&e.document_id)),
            "ground truth evidence outside selected corpus"
        );
    }
    let mut metrics = BTreeMap::new();
    for p in &config.policies {
        metrics.insert(*p, evaluate(&imported.records, annotations, m, *p)?);
    }
    Ok((metrics, attribution))
}

// Explicit deserialize_with makes even nullable fields required: missing != null.
fn required_option<'de, D, T>(deserializer: D) -> std::result::Result<Option<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(deserializer)
}

fn unique_policy_metrics<'de, D>(
    deserializer: D,
) -> std::result::Result<BTreeMap<HypotheticalPolicy, Metrics>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    struct PolicyVisitor;
    impl<'de> serde::de::Visitor<'de> for PolicyVisitor {
        type Value = BTreeMap<HypotheticalPolicy, Metrics>;
        fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("a policy-to-metrics object with unique policy keys")
        }
        fn visit_map<A: serde::de::MapAccess<'de>>(
            self,
            mut entries: A,
        ) -> std::result::Result<Self::Value, A::Error> {
            let mut metrics = BTreeMap::new();
            while let Some((policy, value)) = entries.next_entry::<HypotheticalPolicy, Metrics>()? {
                if metrics.insert(policy, value).is_some() {
                    return Err(serde::de::Error::custom(format!(
                        "duplicate saved policy result: {policy:?}"
                    )));
                }
            }
            Ok(metrics)
        }
    }
    deserializer.deserialize_map(PolicyVisitor)
}

fn same_float(saved: f64, expected: f64) -> bool {
    saved.is_finite() && expected.is_finite() && saved == expected
}

/// Retained observations are mandatory. Exact count/denominator/nullability
/// comparisons enforce confusion-matrix consistency. Compare against the same
/// JSON roundtrip used by persisted metrics, without an acceptance tolerance.
fn validate_saved_metrics(saved: &Metrics, expected: &Metrics) -> Result<()> {
    let expected: Metrics = serde_json::from_slice(&serde_json::to_vec(expected)?)?;
    ensure!(
        saved.observations == expected.observations,
        "observation count mismatch"
    );
    ensure!(
        saved.missing_ground_truth == expected.missing_ground_truth,
        "missing-ground-truth count mismatch"
    );
    ensure!(
        saved.conflicting_ground_truth == expected.conflicting_ground_truth,
        "conflicting-ground-truth count mismatch"
    );
    for (name, actual, correct) in [
        (
            "supported_fact_rate",
            &saved.supported_fact_rate,
            &expected.supported_fact_rate,
        ),
        (
            "unsupported_fact_rate",
            &saved.unsupported_fact_rate,
            &expected.unsupported_fact_rate,
        ),
        (
            "source_traceability",
            &saved.source_traceability,
            &expected.source_traceability,
        ),
        (
            "provenance_authentication_rate",
            &saved.provenance_authentication_rate,
            &expected.provenance_authentication_rate,
        ),
        (
            "false_rejection_rate",
            &saved.false_rejection_rate,
            &expected.false_rejection_rate,
        ),
        (
            "false_acceptance_rate",
            &saved.false_acceptance_rate,
            &expected.false_acceptance_rate,
        ),
        ("precision", &saved.precision, &expected.precision),
        ("recall", &saved.recall, &expected.recall),
    ] {
        match (actual, correct) {
            (None, None) => {},
            (Some(a), Some(e)) => {
                ensure!(a.numerator == e.numerator && a.denominator == e.denominator, "{name}: numerator/denominator disagrees with retained observations (confusion-matrix mismatch)");
                ensure!((0.0..=1.0).contains(&a.estimate) && same_float(a.estimate, e.estimate), "{name}: invalid estimate");
                ensure!(a.wilson_95[0] <= a.wilson_95[1] && a.wilson_95.iter().all(|v| (0.0..=1.0).contains(v)) && a.wilson_95.iter().zip(e.wilson_95).all(|(a,e)| same_float(*a,e)), "{name}: invalid Wilson interval");
            },
            _ => bail!("{name}: missing or unexpected metric; explicit null is valid only with no eligible denominator"),
        }
    }
    match (
        saved.runtime_overhead_ratio,
        expected.runtime_overhead_ratio,
    ) {
        (None, None) => {}
        (Some(a), Some(e)) => ensure!(
            a >= 0.0 && same_float(a, e),
            "runtime overhead disagrees with retained timings"
        ),
        _ => bail!("runtime overhead availability disagrees with retained timings"),
    }
    Ok(())
}
