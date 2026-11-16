# Phase 6 controlled provenance evaluation (data schema 1, run schema 2)

This standalone Rust evaluation crate has no dependency on DreamRAG, Phase 5,
Python provenance scaffolding, model runtimes, or production gates. It reads
retained observations across a model × repetition × consolidation-cycle matrix.
It never performs consolidation, mutates a graph, downloads models, or runs GPU
inference. Future actual graph intervention requires a separately authorized
experiment and implementation. No scientific experimental results are included.

## Reproduce the synthetic smoke run

Run from `core/` (all config paths resolve against the process working directory):

```bash
cargo test -p kwaai-provenance-eval
cargo run -p kwaai-provenance-eval -- check crates/kwaai-provenance-eval/fixtures/corpus crates/kwaai-provenance-eval/fixtures/manifest.json
cargo run -p kwaai-provenance-eval -- sample crates/kwaai-provenance-eval/fixtures/manifest.json 42 1
cargo run -p kwaai-provenance-eval -- run crates/kwaai-provenance-eval/fixtures/config.json
cargo run -p kwaai-provenance-eval -- run crates/kwaai-provenance-eval/fixtures/config.json --resume
```

The committed fixture has two fictional documents, two scripted model identifiers,
two repetitions, two cycles and all four hypothetical policies. The annotation
labels were assigned directly from the fictional documents, independently of the
scripted judgments. These are software test inputs, not observed model results or
estimates of model accuracy. Authentication and timing are deliberately unknown.
Outputs go to `/tmp/kwaai-phase6-fixture-output/synthetic-smoke`. For another run,
copy the config and choose a new run ID and output directory. Existing runs are
never overwritten. Do not place outputs inside the immutable corpus directory.

## Corpus protocol

`inventory ROOT NEW_MANIFEST` walks regular files recursively without writing any
source. It rejects symbolic links and nonregular entries, records exact byte
SHA-256 and byte length, and sorts normalized relative UTF-8 paths. Document IDs
are `doc:` plus SHA-256 of the relative path: stable when a document's bytes
change, different when renamed. They are scoped to a corpus; use corpus version
plus ID as the global reference. Corpus version is SHA-256 of compact JSON for
the sorted document list (Rust struct field order: id, path, sha256, bytes).
Manifest validation checks schema, IDs, ordering, duplicates and corpus digest.
`check ROOT MANIFEST` detects added, removed, renamed and modified documents.

Use a frozen, independently retained corpus snapshot. A preflight integrity check
cannot prevent another process from editing files during a run. This harness
analyzes retained observations; it does not retrieve text for entailment checking.
The evidence locator identifies the source span for annotation/audit; it is opaque
to this evaluator. Files must have portable path names: use `/` as separator and
avoid literal backslashes. File modification times do not affect identity.

Sampling sorts `(SHA256("sample-v1:" + decimal_seed + ":" + document_id), id)`
and takes the first N IDs without replacement. Sample count cannot exceed corpus
size. The same seed/manifest/count yields the same order. Every imported evidence
reference and every used independent annotation reference must be inside that
repetition's selected corpus. Assign annotations against that evidence scope; do
not transfer a full-corpus support label to a subset without checking it. Selection alone does
not demonstrate that an upstream consolidator used only those documents; retain
its input snapshot and execution logs for that assertion.

## Annotation protocol

The serde `Annotation` struct is the authoritative strict schema; see the complete
JSON examples in `fixtures/annotations.json`. Required fields include schema,
annotation ID, claim ID and exact content, evidence document IDs, byte hashes and
locators, support label, rationale, annotator ID, RFC3339 timestamp, corpus version
and origin (`independent` or `verifier`). Labels are `supported`, `unsupported`,
`unresolved`. Supported labels require at least one valid evidence reference.
Unsupported labels may have empty evidence (absence of support); record the
search/contradiction rationale. Validation checks references against the manifest,
not semantic correctness of the human label or authorship.

Assign labels independently, preferably blinded to verifier output and policies.
Use distinct annotation IDs for every assessment and preserve the original list.
Duplicate IDs are rejected, never replaced. Multiple independent assessments must
agree exactly before contributing ground truth; any disagreement excludes that
claim and increments the conflict count. Unresolved consensus is ineligible for
binary metrics. Verifier-origin annotations never enter ground truth or resolve
independent conflicts. Adjudication should produce a new, documented annotation
set/corpus-bound evaluation, retaining the original disagreements for audit;
there is no implicit majority vote or latest-write-wins behavior.

## Configuration and recovery

`Config` in `src/lib.rs` and `fixtures/config.json` specify the run ID, models and
optional available digests/parameters, manifest and corpus root, sample size,
per-repetition seeds, cycle count, repetition count, verification settings,
policies, annotations, complete input matrix and output directory. Missing or
duplicate cells fail preflight. The harness records model and verification
settings; it does not execute or independently attest them. Imported records must
match corpus version, model ID, repetition, cycle and seed. Use a frozen export
from each actual upstream cycle; never reuse one snapshot and call it new data.

`run.json` records the experiment schema, Git HEAD and porcelain dirty state
(including untracked paths), UTC start time, full config, normalized config hash,
manifest and hash, annotation hash, exact input byte hashes, selected documents,
OS/architecture/CPU information when available. Config/annotation/manifest hashes
use typed compact serde JSON, so whitespace/key order in source JSON are ignored;
input hashes cover exact bytes. Model digests are caller-supplied when available,
otherwise null; no network discovery is attempted. Seeds do not guarantee
reproducible model outputs. Retain runtime versions, model weights and external
execution logs alongside actual observations.

Each completed cell is exclusively created and records identity, source hash,
per-policy metrics and evaluator elapsed time. Failed evaluation cells produce
no success cell and are listed in a timestamped `attempt-NNNNNN.json` with
`complete=false`; later cells are still attempted. Inputs unavailable at preflight
and structural checkpoint errors stop immediately with a CLI error. A run is
complete only with a complete attempt report and all expected valid cells.

`--resume` requires the existing run header, an identical evaluator implementation
fingerprint, and identical config, corpus, annotations and input hashes. It
validates completed cell identities and exact configured policy coverage, rejects
duplicate policy keys and unknown fields, and requires every metric field even
when its correct value is explicit null. It then rereads and hash-checks the
retained observations, repeats input/sampling/annotation validation, and recomputes
all configured policy metrics. Counts, denominators, coverage, confusion-matrix
relationships, estimates, Wilson intervals and runtime overhead must agree.
Floating values are compared exactly after applying the same serde JSON roundtrip
as the originally persisted metrics; there is no numerical acceptance tolerance.
Saved cells are preserved only after this validation; absent cells are recomputed.
Retained observations are mandatory: there is no identity-only or internally
consistent-metrics fallback when they are unavailable. It does not accept partial JSON as a completed
cell. After an interrupted write, explicitly move the corrupt cell to a retained
quarantine location before resuming; retain error logs. If the run header itself
is incomplete, start a new run ID and retain the failed directory. Changed inputs,
labels or config require a new run; do not repair a prior run in place. Reports
are append-only per attempt. Use one writer per run directory. Checkpoints are
recovery artifacts, not a tamper-proof archive.

## Evaluator implementation identity

Run metadata, every cell and every completed attempt report contain an `evaluator`
identity: algorithm version 1, aggregate SHA-256 and a sorted path-to-file-SHA-256
map. Run/checkpoint envelope schema is now **2**; corpus, annotation, configuration
and adapter schemas remain **1**. Old runs without an evaluator identity cannot
resume: retain them and start a new run ID. No automatic migration invents an
identity for historical results.

`src/identity.rs` embeds the exact compile-time bytes of these eight files:

- `core/crates/kwaai-provenance-eval/src/lib.rs` (including authoritative data,
  metric, configuration and run schemas, policy semantics and evaluation logic)
- `core/crates/kwaai-provenance-eval/src/main.rs`
- `core/crates/kwaai-provenance-eval/src/identity.rs` (identity schema/algorithm)
- `core/crates/kwaai-provenance-eval/Cargo.toml`
- `core/Cargo.toml`
- `core/Cargo.lock`
- `core/.cargo/config.toml`
- `rust-toolchain.toml`

The aggregate hashes the literal bytes
`kwaai-provenance-eval-implementation-v1` followed by one NUL byte, followed by
compact serde JSON for the BTreeMap of paths to lowercase SHA-256 hex digests.
The map uses lexicographic path ordering. Paths are fixed repository-relative
strings; checkout location, time, Git metadata and file modification times do not
enter the fingerprint. Any byte change in a listed file changes the fingerprint
after rebuilding. Adding source modules or external schema files requires
extending this explicit coverage list.

Embedding identifies the implementation in the running binary, rather than a
possibly edited checkout at runtime. Reusing an old binary reports its old
identity, honestly. Resume compares the full identity object before accepting any
cells; cells also must match it. Mismatches fail closed with saved/current hashes
and instructions to retain the run and start a new ID. Rejected resumes do not
write successful attempt reports.

This is source/dependency-resolution identity, not a binary signature or an
execution-environment attestation. It excludes documentation, integration tests,
fixtures (separately input-hashed), actual compiler executable, environment
variables, command-line compiler flags, dependency cache contents and hardware.
It does not replace retaining the built binary, source snapshot, execution logs
and actual runtime/model versions for empirical experiments. Git commit/status
remain supplementary metadata. Checkpoints are not a tamper-proof archive;
original evaluator elapsed time cannot be reconstructed from replay.

## Metrics and interpretation

All rates include numerator, denominator, estimate and a two-sided 95% Wilson
interval. A zero eligible denominator yields JSON null. Metrics are per cell,
without silently pooling dependent observations across cycles or repetitions.

| Metric | Numerator / eligible denominator |
| --- | --- |
| Supported-fact rate | independently supported / independently supported + unsupported claims |
| Unsupported-fact rate | independently unsupported / independently supported + unsupported claims |
| Source traceability | traceable observations / observations with a supplied traceability assessment |
| Provenance authentication | authenticated observations / observations with a supplied authentication assessment |
| False rejection | rejected supported claims / independently supported claims |
| False acceptance | accepted unsupported claims / independently unsupported claims |
| Precision | accepted supported claims / accepted independently binary-labeled claims |
| Recall | accepted supported claims / independently supported claims |

Supported/unsupported fact rates describe the entire observed claim set and stay
the same across hypothetical policies. Precision describes the hypothetical
accepted subset. Acceptance is the positive prediction; recall equals one minus
false rejection when defined. Each claim counts once per cell regardless of
annotation count. Missing/unresolved labels and disagreements are separately
counted and excluded from binary denominators. Always report these coverage counts
and denominators; a labeled subset may not represent the whole corpus.

Traceability and authentication are distinct imported observations, never proxies
for factual support. True assertions require nonempty references matching the
manifest. Any supplied authentication assessment also requires an audit basis
(method and retained record identity). The adapter is responsible for checking
lineage/signatures against its authoritative source; this evaluator does not
perform cryptographic authentication merely by hashing a document. Unknown
assessments remain null; false observations stay false. Source locators and
assessment audit bases must be audited before publishing empirical findings.

Runtime overhead is `sum(additional verification_ms) / sum(baseline_ms)` only when
every observation has both measurements and the baseline sum is positive. Supply
paired, nonoverlapping measurements; duplicating a cycle-level timing per claim
would bias this ratio. Cell elapsed_ms measures offline evaluation itself, not
model inference overhead. No overhead CI is invented from one aggregate pair.

Wilson intervals assume independent Bernoulli observations and are descriptive
within a cell. Correlated claims, repeated cycles and shared evidence violate
that assumption. Do not pool them as independent samples or infer significance
from these intervals. A real protocol should predeclare annotation sampling,
conflict handling, model digests, repetitions, timing boundaries and a clustered
or paired analysis for cross-policy comparisons. No shadow decision demonstrates
improved actual graph quality.

See [ADAPTER.md](ADAPTER.md) for the Phase 5 boundary and
[VALIDATION.md](VALIDATION.md) for executed checks and limitations.
