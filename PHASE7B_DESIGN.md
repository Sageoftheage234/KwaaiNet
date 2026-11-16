# Phase 7B historical state capture (experimental v2, audit remediation)

Implemented in `kwaai-provenance-rag::history`, with an additive strict persisted
reader in `kwaai-rag::graph::GraphStore` and a separate
`kwaai-provenance-eval::history::evaluate_state` API. No enforcement, graph write
hooks, native-ID changes, lineage format changes, or Phase 7A artifact changes.
The default DreamRAG and CLI observer selection paths are unchanged.

## Architecture audit and observable boundaries

`run_dream_cycle_observed` calls synchronous shared-reference `DreamObserver`
methods inline on its async task. `run_dream_cycle` still uses `NoopObserver`.

| Path | Observable boundaries |
| --- | --- |
| Successful normal cycle | start; completion; merge (attempted pairs, including failed merges); prune; sanitize; finish Completed |
| Empty work list | start; finish NothingToDo; no stages |
| All completions failed and nothing completed | start; completion; finish SkippedDestructiveStages; no merge/prune/sanitize |
| Individual write/merge/delete fails but cycle continues | stage callback still runs; captures the actual persisted state, not attempted changes; individual errors only in DreamReport |
| Graph/meta open fails before start | no callbacks, no historical observation |
| Scoring/chunk reads or graph opens return early with `?` after start | observed prefix only; no error or finish callback |
| Task cancellation, panic, process termination | prefix only, possibly no start; no guaranteed interruption callback |

Completion is called before the destructive-stage guard. Merge/prune/sanitize
callbacks run even when they change nothing. Final scoring occurs before finish;
finish is retained even when identical to sanitize (or start/completion).
`same_state_as` explicitly links byte-identical structured graph states, while
occurrence IDs remain boundary-specific. These are observations, not proposals
and not evidence of causal effects. Operation-level mutation hooks are excluded.

Existing `facts::snapshot` uses cached entities and a relation reader that drops
SQL/JSON errors. It unions duplicate claim evidence, excludes empty fields,
descriptions, types and embeddings. Existing `ProvenanceObserver` records additive
ancestry, not complete historical evidence removals or claim text. Its baseline,
drift reconciliation, gate outcomes, `kb.json`, strict `lineage.jsonl` and cycle
summaries are preserved. Historical capture retains the same claim domain and additionally retains relation extraction-window contexts. It reads
both entity and relation rows directly from SQLite in one read transaction and
propagates every row/decode failure. Cache contents and intended mutations are
never used to reconstruct a historical state.

Existing `OriginalEvidenceResolver` reads current MetaStore content by chunk ID;
it cannot recover old bytes after re-ingestion. Phase 7B preserves bytes when the
boundary is observed, then resolves only immutable blobs. Phase 5 shadow inputs
retain native requests, assessments and explicit timing; historical exports are
not silently passed off as genuine pre-mutation proposals or Phase 5 bundles.
Phase 6's `evaluate` selects independent annotations solely by native claim ID,
including across input cells. A distinct cell alone would not isolate labels.

## Opt in and resume

Rust callers construct `Run::new(tenant_id, corpus: Option<String>)`, persist the
returned descriptor, and call:

```rust,ignore
let mut observer = HistoricalObserver::open(archive_dir, data_dir, run)?;
run_dream_cycle_observed(/* existing arguments */, &mut observer).await?;
// Check observer.failure() and retain DreamReport.cycle_errors separately.
```

Supply the same descriptor to resume; do not generate a new UUID for an existing
archive. One observer may span multiple cycles; cycle numbering starts at 1 and
increments at each observed finish. Reopen advances beyond the last observed
cycle. An unfinished prefix causes an explicit `resume_incomplete` record in that
previous cycle with unknown cause, before the next start. No historical state is
invented for the missing suffix. Use a separate archive per run. This is an API
opt-in, not a new default CLI environment switch. To retain lineage concurrently,
a caller may forward the three existing callbacks to both observers.

## Versioned schema and encoding

Authoritative strict serde types: `Run`, `Capture`, `State`, `Evidence`, `ExtractionContext`, `Export`
in `history.rs`. Unknown fields are rejected. Versions are independent of the
unchanged provenance/verification/lineage v1 schemas.

* `run.json`: schema_version 1, UUID run_id, tenant UUID, optional caller-supplied
  corpus version/fingerprint. Run UUID is generated locally by `Run::new`; neither
  supplied metadata nor a UUID authenticates execution identity.
* `records/00000000000000000001.json`: envelope `{sha256, capture}`. Hash is SHA-256
  of compact serde serialization of typed capture. Capture schema version 2 has full run
  descriptor, cycle, stage, boundary, monotonic sequence, RFC3339 wall-clock capture
  timestamp, complete/error, optional cycle_end, optional same_state_as and states.
* State: occurrence_id, exact native Claim (including native ID), contemporaneous
  entity-ref/name bindings, sorted complete direct evidence set and separately sorted extraction contexts. Missing bindings
  are absent, never inferred. Duplicate graph rows with the same structured claim
  are unioned. Empty/whitespace-only entity fields are outside the prior fact domain.
* ExtractionContext: the persisted relation evidence_window, that row's sorted direct
  centre IDs, expanded passage Evidence, unresolved centre IDs and missing
  document/index positions. Distinct duplicate-row windows are retained separately.
  Context uses document/index adjacency, filters section zones, and clips to the
  observed document extent. Internal gaps are explicit missing positions, never
  invented chunk IDs. This is the declared boundary-time indexed-window protocol;
  it does not claim to reconstruct an old batch prompt or an earlier ingestion.
* Evidence: full native EvidenceRef, optional source document identity, chunk
  index and section type, optional immutable blob path, explicit `available`/`missing` retrieval.
  Chunk ID remains in EvidenceRef; document identity is the stored document name,
  not an authenticated document identifier. Available original identities bind
  `kb:<tenant>`, chunk ID and SHA-256. Missing chunks use explicit derived refs
  with empty hash, not invented original hashes or ancestry substitutions.
* `blobs/<lowercase sha256>`: exact MetaStore passage UTF-8 bytes, without newline,
  whitespace or Unicode normalization. These are stored passages, not the original
  pre-ingestion PDF/file bytes. No whole-document hash is fabricated.

State occurrence identity:

```
"state:" + hex(SHA256(
  UTF8("kwaai.history.state-occurrence.v2") || 0x00 ||
  compact_serde_json((run, cycle, stage, boundary, sequence,
                     store_binding, native_claim, entity_bindings,
                     direct_evidence_vector, extraction_contexts))
))
```

Tuple framing, fixed struct field order, sorted BTreeMap bindings, chunk-ID sorted
unique evidence and claim-ID sorted states define v2 deterministically. Strings
are byte-exact. Evidence document/retrieval/blob metadata is also bound. Timestamps
are intentionally excluded from occurrence identity; envelope hashes cover them.
Changing evidence, extraction-window metadata/context, names, relation/field content,
store binding, run or boundary changes identity. Capture and Export use schema 2;
Run and HistoricalAssessment retain schema 1. V1 capture archives are rejected
explicitly: their missing context cannot be recovered through migration.
Native claim/evidence ID algorithms are untouched. Hashes provide consistency,
not signatures, trusted associations or authenticated lifecycle execution.

## Persistence and completeness

Files are published by writing a unique create-new temporary file, syncing it,
then hard-linking into place without replacement and syncing the parent directory.
Evidence deduplicates only identical bytes. Snapshot sequences refuse duplicates;
record readers reject duplicate claim/evidence IDs, bad ordering, unsupported
versions, invalid IDs, hashes, missing/modified blobs and truncated JSON. Resume
checks full run identity and all records before recording. Legal lifecycle paths are exactly start/completion/merge/prune/sanitize/Completed,
start/NothingToDo, and start/completion/SkippedDestructiveStages. An unfinished
legal prefix is readable; resume adds only an unknown-cause interruption marker.
Missing, duplicate, out-of-order and contradictory callbacks set failure and retain
the attempted callback as an incomplete record. Strict read/export/resume reject
illegal transitions; completeness reporting can inspect structurally sound records
with illegal transitions and reports them ineligible. No interruption callback is
invented. Unknown/inconsistent boundaries and cycle/stage order are rejected. Retained same-state references are
validated against their predecessor's content. Files ending in `.tmp` are ignored
as unpublished artifacts; they are not accepted as observations.

A create-new `writer.lock` prevents cooperating concurrent writers. Orderly drop
removes it. A crash may leave the lock: the operator must verify no writer remains
and remove only the stale lock before resume. Automatic stale-lock takeover is
unsupported. No automatic corrupt-record repair is performed. Deleting a complete
trailing suffix cannot be detected without an external expected head/hash; use the externally retained ArchiveHead described below.

`complete` means the structured graph read and evidence-status collection finished,
not that all evidence exists, every stage write succeeded, the cycle finished, or
factual support is established. Missing originals are explicit evidence entries in
a complete graph observation. Store-binding/SQL/blob/retrieval failures clear partial claims and
write an incomplete record. Later callbacks preserve the failure as incomplete
records rather than falsely reporting success. `failure()` reports errors even
when storage itself prevents writing any failure record. No crash-atomic graph /
journal correspondence is claimed. A graph mutation can commit before its capture
is durable. There is no cross-database graph/MetaStore transaction: concurrent
external re-ingestion can change evidence between reads. Experiments should
serialize writers; historical completeness cannot certify external quiescence.

## Store binding and experiment completeness

At cycle start, both callback stores must resolve to the expected canonical files
in the configured data directory. Opt-in store accessors use SQLite's read-only
SQLITE_FCNTL_HAS_MOVED VFS check to bind the path to the actual open connection;
unsupported VFS/platform identity fails explicitly. Every capture rechecks the graph
and opened read-only MetaStore and compares canonical paths, device and inode to
the run's retained binding, before and after collection. Same tenant/chunk IDs or
matching content cannot authorize a different store. Resume retains this binding;
relocating/copying the live stores requires a new run/archive. Archive reconstruction
uses recorded identities only, without reopening these paths. VFS checks run only
when historical capture requests them; default graph writes and transactions are
unchanged. Uncoordinated external path replacement/concurrent writes are unsupported,
not made atomic by identity checks.

Four separate questions are represented: archive structural validity (hashes,
schema, identities, blobs), individual capture success, legal/finished cycle
lifecycle, and coverage against an explicit experiment expectation. Use:

```text
cargo run -p kwaai-provenance-rag --bin provenance-history -- completeness ARCHIVE EXPECTATION.json
```

An expectation has schema_version 1, nonempty consecutive cycles from 1 and an
optional externally retained final head, for example:

```json
{
  "schema_version": 1,
  "cycles": [{"cycle": 1, "cycle_end": "Completed"}],
  "expected_archive_head": null
}
```

Declare terminal paths before analysis. A completed path expects all six boundaries;
the two legitimate skips expect only their actual callbacks. The JSON report lists
expected/observed cycles and boundaries, missing boundaries, incomplete captures,
missing originals (including empty evidence associations and expanded-window gaps), unexpected/contradictory events,
structural and lifecycle validity, observed/expected archive heads and eligibility.
CLI exit status is nonzero for an ineligible experiment, while stdout still contains
the machine-readable report. If structural corruption prevents reading, observed
coverage is unavailable; structural_error explains this and eligibility is false.

ArchiveHead hashes the run bytes and every published JSON record name/content in
sorted order with domain kwaai.history.archive-head.v2. It includes record_count.
Retain the full object outside the archive after the intended final capture and
supply it as expected_archive_head to detect removal of even a complete trailing
cycle. A fresh head computed from a damaged archive provides no such guarantee.
Temporary unpublished files are excluded. Blob corruption is checked separately.
Run reporting with the writer quiescent; there is no atomic concurrent-reader head.

Eligibility requires structural validity, legal transitions, exact planned cycle
and boundary coverage, successful captures, no missing originals and no head
mismatch. Parsing alone is insufficient. This certifies input-history coverage
under the declared protocol, not factual support, successful individual graph
operations, independent annotation, execution authenticity or general behavioral
equivalence. Retain DreamReport and independent annotations separately.

## Verification export and evaluation

From `core/`:

```
cargo run -p kwaai-provenance-rag --bin provenance-history -- validate ARCHIVE
cargo run -p kwaai-provenance-rag --bin provenance-history -- export ARCHIVE
```

Export emits JSONL `Export` envelopes, one per complete state occurrence, retaining
full capture/run/corpus metadata in the Export envelope and compact, canonical
Phase 4 VerificationRequest structs. Requests contain the sorted union of direct
and expanded passage references; the archive keeps those roles separate. Missing
indexed positions add explicit Derived availability markers with no chunk ID and
no original hash. Both raw Phase 4 verification and verify_state therefore fail
closed with MissingEvidence. Full capture metadata is excluded from per-request
budgets; unchanged claims/evidence keep the same request size as unrelated graph
content grows. Configured budgets are never raised.
No verdict is synthesized. Use `ArchivedResolver::for_state(archive, occurrence)`
and Phase 4 `verify(&export.request, &resolver, &backend, budget).await`. Prefer
`history::verify_state(archive, &export, &backend, budget).await`: it validates the
export against the archive and returns `HistoricalAssessment` binding the Phase 4
result to the occurrence ID and full request SHA-256. The scoped
resolver refuses evidence outside that occurrence, checks hashes again on resolve,
and never reads the live KB. Whole-archive resolver is available but rejects
ambiguous document bindings. Missing/empty evidence yields Phase 4 Unresolved.
Unbound entity names retain Phase 4's existing InvalidIdentity behavior.

`kwaai_provenance_eval::history::evaluate_state` is a separate additive API, not an
extension of the strict Phase 5 bundle parser or normal runner configuration.
Pass one Export, its retained HistoricalAssessment, independent state-scoped
annotations, a Phase 6 Manifest and policy. It shares validate_capture/validate_export with archive reading, rejects
contradictory completeness/errors, malformed timestamps, versions, duplicate or
invalid native IDs, invalid contexts and occurrence identities. It checks retained
source bytes/hashes, document identity, reference prefix order and failure/status
consistency as well as occurrence/request/assessment identity (including the assessment's occurrence and request hash) and requires the explicit corpus version to match that manifest. Native
Claim IDs remain in requests/results and retained exports. Only the normalized
Phase 6 observation/annotation key is the state-occurrence ID; annotation content
is exact compact JSON of the native Claim. Native-ID or other-state labels are
rejected, including labels for unchanged native claims with changed evidence.
Verifier-origin annotations remain excluded by Phase 6. No verifier judgment is
independent ground truth. Missing, unresolved and conflicting annotations retain
Phase 6's existing protocol. Document hashes cannot be derived from passage hashes;
normalized traceability/authentication/timing are unknown and normalized document
evidence is empty. Full original evidence is retained in Export; no invented
manifest mapping is supplied. This API has no checkpoint runner; callers must
retain inputs/results and use the existing implementation fingerprint (now also
covering both history modules, facts, persisted graph/meta readers, store identity,
section-zone logic and the Ollama verifier) for their experimental protocol.

## Observation effects and unsupported conditions

Capture runs synchronously and blocks the async worker until reads, hashing,
serialization, blob publication and fsync complete. Full graph scans acquire a
SQLite read transaction; they may contend with writers/checkpointing. MetaStore is
opened read-only per boundary and strictly inventories chunks, so malformed rows
fail capture instead of becoming missing references. Shared chunks incur repeated
existing-blob comparisons. No graph write return
value or transaction boundary is changed, but scheduling and duration can change.
Memory includes graph rows, chunk text, each capture, and all prior records for
resume/ordering; it grows with total captured history. Export duplicates a full
capture in each per-claim envelope and can be expensive on large graphs. Disk
writes include every snapshot, unique passage blobs, temporary files, lock and
fsyncs; identical boundaries retain full snapshots. This favors reviewable research
artifacts over throughput. No zero-overhead or general behavioral-equivalence
claim is made. Validation reports both the empty-cycle comparison and one populated successful
cycle with fixed HTTP fixture responses. Neither measures general equivalence.

Unsupported: operation-level history, pre-start failures, reliable cancellation or
process-death callbacks, identifying which write failed from observer hooks alone,
recovering evidence missing or overwritten before first capture, exact earlier
extraction prompts (batch size, filters and original section inputs are not stored
by production), historical whole-source-file
hashes, externally concurrent-writer atomicity, authenticated run/corpus identity,
crash-atomic graph/journal linkage, automatic stale lock/corrupt tail repair, and
causal attribution. Persist DreamReport separately when stage error details matter.
