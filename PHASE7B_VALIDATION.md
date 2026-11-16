# Phase 7B audit remediation validation

Repository `/home/kwaai/KwaaiNet-Phase7B`, branch `feat/provenance-phase7b`,
baseline `4c460d698a32d1378d5a9ca94f5b0451e98e1c4a`, 2026-11-16.
All original Phase 7B work is preserved. No commits, pushes, merges, other-worktree
changes, extended corpus, or model inference were used. The original 424-test
report is retained below as historical evidence, not the current test total.

## Reproduction before implementation changes

The actual implementations and production interfaces were inspected first. Five
regressions were added before implementation remediation. Both commands exited
101 with the intended assertions failing:

```text
cargo test -p kwaai-provenance-rag -p kwaai-provenance-eval --test history audit_
cargo test -p kwaai-provenance-rag --test history audit_
```

The first command stopped at the evaluation regression: 0 passed, 1 failed.
The second ran the four capture regressions: 0 passed, 4 failed. Logs:
`/tmp/phase7b-audit-red.log`, `/tmp/phase7b-audit-rag-red.log`.

| Finding | Regression and original failure | Corrected behavior and additional coverage |
| --- | --- | --- |
| 1: window evidence | `audit_window_context_is_retained`: expected window 1, got absent/null | Window metadata and direct/expanded roles retained. `window_metadata_changes_state_content_and_expanded_bytes_survive_without_live_store` holds occurrence coordinates equal, changes only context, checks exact CRLF/NUL bytes after deleting the live document, tests document/section filtering and missing expanded-only blob rejection. Missing centres/positions remain explicit and raw Phase 4 verification fails closed. |
| 2: lifecycle | `audit_completed_requires_all_stages`: observer accepted start/Completed without stages | Explicit consecutive transitions and terminal-path validation. Eight missing/duplicate/out-of-order/contradictory cases fail recording/strict reading; machine reports retain structural validity separately from lifecycle validity. Legitimate skips, prefixes and resume markers are covered. |
| 3: store binding | `audit_store_binding_rejects_other_directory_same_tenant`: observer accepted another directory's passage | Callback graph and MetaStore must match canonical configured files and retained device/inode identity; SQLite VFS verifies the path belongs to the open connection. Wrong callback stores, a mid-cycle graph switch and replaced open files fail incomplete. |
| 4: budget | `audit_unrelated_graph_growth_does_not_consume_verification_budget`: unchanged claim failed InputTooLarge after 40 unrelated 500-character field values | Same claim/evidence succeeds at the identical 10,000-byte limit before/after growth. Full capture stays in Export; per-request attribution is compact. |
| 5: evaluation | `audit_contradictory_completeness_is_rejected`: complete=true/error=Some was accepted | Shared structural validation rejects contradictions, versions, timestamps, duplicate/invalid IDs, invalid boundaries, absent store binding and self-referencing same-state links. Two evidence states of one native claim accept independently supplied opposing labels and reject cross-state labels. Verifier-origin labels still yield missing ground truth. Retained source text/document/hash/reference tampering and missing/extra sources are rejected. |

## Final validation

Commands run from `core/` unless noted. No inference server is used: HTTP fixtures
return fixed JSON and verification backends are scripted.

| Exact command | Result |
| --- | --- |
| `cargo test -p kwaai-provenance-rag -p kwaai-provenance-eval --test history` | 29 passed, 0 failed (24 capture + 5 evaluation), loopback-capable execution |
| `cargo test -p kwaai-provenance -p kwaai-provenance-rag -p kwaai-provenance-eval --all-features` | 145 passed, 0 failed, 1 ignored |
| `cargo test -p kwaai-rag --lib` | 296 passed, 0 failed, 1 ignored |
| `cargo fmt --all -- --check` | Pass |
| `cargo clippy -p kwaai-rag -p kwaai-provenance -p kwaai-provenance-rag -p kwaai-provenance-eval --all-targets --all-features -- -D warnings` | Pass |
| `cargo check --workspace` | Pass; default workspace features |
| `git diff --check` (repository root) | Pass |

Current disjoint suite total: **441 passed, 0 failed, 2 ignored**. Focused reruns
are not added again to this total. Historical tests increased from 12 to 29;
the original strict persisted graph read test remains in the 296-test suite.

Logs: `/tmp/phase7b-remediation-history-approved.log`,
`/tmp/phase7b-remediation-provenance-final.log`,
`/tmp/phase7b-remediation-rag.log`, `/tmp/phase7b-remediation-fmt.log`,
`/tmp/phase7b-remediation-clippy.log`, `/tmp/phase7b-remediation-workspace.log`.

The initial sandbox focused run failed only at the populated fixture's loopback
bind; approved execution resolved the environment restriction. During development,
fixture checks caught a Rust borrow, a Rate-vs-float assertion, and a supported
annotation missing its independently supplied manifest reference. The populated
fixture initially omitted GraphStore chunk links, correctly yielding placeholder
missing evidence; it was corrected to use real link_chunk before final execution.
All intended original-failure evidence predates implementation changes. These
fixture-development failures do not replace the five original red regressions.

## Populated behavioral comparison

`populated_scripted_cycle_matches_capture_disabled_without_model_inference` runs
actual observed and unobserved DreamRAG cycles on separate identically seeded
stores, with two entities, fields, original passage bytes and chunk links. A local
fixed HTTP fixture returns a schema:Person type and associated_with relation for
one work item; no model or embedding inference runs. Both executions perform one
type completion and one relation addition, then run merge/prune/sanitize. It checks:

* Complete report equality except timestamp/duration.
* Exact persisted entity/relation value equality via strict SQLite reads.
* Six captured boundaries and a relation absent at start/present at completion.
* Experiment completeness eligibility against a declared Completed cycle.

This demonstrates one successful populated deterministic path, not general
behavioral equivalence, concurrency invariance or a performance estimate. It does
not force per-operation SQL failures, exercise real generation/embedding, or prove
all merge/prune/sanitize mutation paths equivalent. The earlier empty-cycle
comparison remains only an additional plumbing check.

## Completeness reporting

API `completeness_report` and CLI
`provenance-history completeness ARCHIVE EXPECTATION.json` separately report
structural validity, individual capture completeness, cycle lifecycle completeness
and experiment coverage. Tests cover empty archives, unfinished prefixes, normal
and skipped cycles, missing originals/empty evidence associations, capture errors,
unexpected cycles, invalid events, resume markers and removal of a whole valid
trailing cycle. An externally retained ArchiveHead detects suffix loss even if the
shorter archive parses and a shorter plan would otherwise accept it. CLI tests
parse JSON and check failure/success exit statuses. Reports cannot prove an
externally declared plan or execution identity is truthful.

## Files changed for remediation

* `PHASE7B_DESIGN.md`, `PHASE7B_VALIDATION.md`.
* `core/crates/kwaai-provenance-rag/{Cargo.toml,README.md,src/history.rs,src/bin/provenance-history.rs,tests/history.rs}`.
* `core/crates/kwaai-provenance-eval/{README.md,src/history.rs,src/identity.rs,tests/history.rs}`.
* `core/crates/kwaai-rag/src/{lib.rs,graph.rs,meta_store.rs,store_identity.rs}`.

Other pre-existing Phase 7B working-tree changes remain preserved. Graph mutation
methods, DreamRAG stage code, transaction boundaries, default observer selection,
and enforced gating were not changed. SQLite identity checks run only when called
by capture; unsupported platform/VFS identity fails explicitly.

## Scientific readiness and remaining limits

Ready for small, serialized development-corpus experiments under the documented
boundary-time indexed-window protocol, after declaring expected cycles, retaining
the final archive head externally, checking eligibility and collecting independent
state-scoped annotations. Missing evidence and capture/lifecycle failures make the
report ineligible rather than being silently dropped from the experiment.

Not established: exact prompts/bytes from extraction before the first capture,
whole-source-file identity, authenticated execution/annotation origin, cross-store
atomicity, external writer quiescence, crash-atomic graph/journal correspondence,
process-death callbacks, per-operation failure identification, generalized behavior
preservation, causal effects, large-corpus overhead or model factual accuracy.
Production stores retain window size but not old batch size/filter inputs or exact
prompt membership. Expanded context is explicitly the boundary-time indexed window,
not an assertion that the historical extraction prompt is recoverable. V1 capture
archives are rejected, not silently upgraded. Disk-full/fsync fault injection and
process-kill experiments remain unperformed. No hardware feature matrix or entire
workspace test suite was run.

---

# Initial implementation validation (retained historical record)

Executed 2026-11-16 in `/home/kwaai/KwaaiNet-Phase7B`, branch
`feat/provenance-phase7b`, starting commit
`4c460d698a32d1378d5a9ca94f5b0451e98e1c4a`. Changes remain uncommitted.
Only this worktree and `/tmp` were written; no Phase 7A artifacts were changed.

## Commands and final results

Commands below run from `core/` unless stated otherwise. Test totals include
binary/doc suites where applicable.

| Exact command | Final result |
| --- | --- |
| `bash patches/fetch-patches.sh` | Pass with approved network; generates this worktree's ignored dependency patch directories |
| `cargo test -p kwaai-provenance -p kwaai-provenance-rag -p kwaai-provenance-eval --all-features` | **128 passed, 0 failed, 1 ignored**, approved loopback-capable execution |
| `cargo test -p kwaai-rag --lib` | **296 passed, 0 failed, 1 ignored**, approved loopback-capable execution |
| `cargo fmt --manifest-path core/Cargo.toml --all -- --check` (worktree root) | Pass, no output |
| `cargo clippy --manifest-path core/Cargo.toml -p kwaai-rag -p kwaai-provenance -p kwaai-provenance-rag -p kwaai-provenance-eval --all-targets --all-features -- -D warnings` (root) | Pass, no diagnostics |
| `cargo check --workspace` | Pass, approved network-capable execution for existing pinned protoc download |
| `cargo test -p kwaai-provenance-rag --test history real_empty_cycle_matches_uninstrumented -- --nocapture` | Pass; 1 passed, 9 filtered; timing below |
| `git diff --check` (root) | Pass |

Final logs: `/tmp/phase7b-provenance-final.log`,
`/tmp/phase7b-rag-approved.log`, `/tmp/phase7b-fmt.log`,
`/tmp/phase7b-clippy-final.log`, `/tmp/phase7b-workspace-final-approved.log`,
`/tmp/phase7b-timing-core.log`.

Environmental failures and corrected development failures:

* Initial `cargo check -p kwaai-provenance-rag` could not resolve the missing
  generated `core/patches/cudarc/Cargo.toml`. Patch fetching first failed sandbox
  DNS, then succeeded with approval. No dependency patch source was committed.
* Sandbox all-feature provenance tests could not bind the existing mock HTTP
  listeners. Approved reruns passed. Sandbox `kwaai-rag --lib` had 287 passes,
  9 loopback permission failures and 1 ignored; approved rerun passed all 296.
* Workspace check first failed when the existing p2p-daemon build script attempted
  a pinned protoc download under sandbox network restrictions. Approved retry
  passed. Workspace check uses default features; the hardware feature matrix and
  entire workspace test suite were not run.
* A first historical field-change fixture incorrectly assumed `upsert_entity`
  replaces populated fields. Inspection confirmed additive union behavior. The
  test now deletes/reinserts through real GraphStore methods to replace a field.
* Development checks caught a moved fixture string, the evaluator fingerprint
  test's old file-count expectation, one unnecessary test clone, and an assessment
  fixture field access after adding the historical wrapper. All were corrected;
  final suites/Clippy are clean.

## Coverage

New tests: ten `kwaai-provenance-rag/tests/history.rs`, two
`kwaai-provenance-eval/tests/history.rs`, one strict persisted-read unit test in
`kwaai-rag/src/graph.rs` (13 new tests total). Existing suites remain intact.

| Requirement | Evidence |
| --- | --- |
| Deterministic occurrence identity | Repeated computation plus separate changes to run/cycle/stage/boundary/sequence/content/predicate/names/evidence |
| Same native claim, changed evidence | Real persisted field evidence union and removal; same native ID, different occurrence IDs and complete per-boundary sets |
| Same claim across boundaries | Identical sanitize/finish states retain distinct occurrence IDs and validated same-state references |
| Entity field and binding changes | Real delete/reinsert changes field content; separate name-only replacement preserves claim/evidence while changing bindings |
| Relations, creation, deletion | Real upsert/delete/replacement operations; relation and entity disappearance visible in subsequent full snapshots |
| Evidence addition/removal | Multiple available sources plus missing chunk; exported resolver cannot resolve removed evidence for the new occurrence |
| Missing originals | Explicit missing entry; whole-store retrieval failure emits incomplete records; missing originals are never replaced with current text |
| Original preservation and export | Exact CRLF/NUL passage bytes and document identities survive live MetaStore overwrite; actual Phase 4 verifier runs with a scripted backend |
| Skipped stages | Callback-driven SkippedDestructiveStages captures start/completion/finish only; actual empty DreamRAG cycle captures start/finish only |
| Failed capture | Invalid original-store path, incomplete records at start and finish, failure API, unchanged graph |
| Duplicate snapshots | Cooperating writer refusal, duplicated record sequence rejected; identical graph-state captures explicitly marked |
| Corruption/truncation | Truncated record, altered capture, modified blob, copied record sequence all refuse validation/resume; resolver rechecks blob bytes after construction |
| Strict graph read | Malformed persisted relation row causes historical read error instead of silently dropping it |
| Resume | Full replay, unchanged run descriptor, cycle advancement, explicit unfinished-prefix marker; changed metadata/corrupt records rejected |
| Phase 4 export | Native IDs retained, metadata retained, archive/request mismatch rejected, exact source texts and hashes supplied to actual `verify` |
| Annotation isolation | Native-ID and another-boundary annotations rejected; missing independent labels remain missing; verifier labels excluded |
| Assessment isolation | Historical occurrence/request wrapper checked, request and assessment evidence mismatches rejected |
| Non-instrumented preservation | Actual observed/unobserved empty cycles compare score-before/after and cycle errors; graph equality on capture failure; existing observer tests still pass |

The scripted backend is explicitly a fixture, not a factual-support measurement.
No verifier verdict is promoted to independent ground truth. No live model or
private corpus was used. A populated successful LLM consolidation cycle and forced
per-operation write failures were not exercised end to end; populated histories
use actual graph APIs with the existing callbacks driven at stage boundaries.
The existing lifecycle source establishes where callbacks occur under those
conditions. No cancellation/process-kill durability experiment, stale-lock crash
recovery, disk-full fault injection, external concurrency study or large-corpus
benchmark was performed. Corrupt/truncated files are injected directly. A resumed
unfinished prefix records unknown cause; it does not prove that a cycle crashed.

## Observation-effect measurement

`real_empty_cycle_matches_uninstrumented` uses separate fresh temporary stores,
the same tenant, default DreamConfig, empty inference endpoint list and an unused
loopback embed URL. Enabled runs first, disabled second. Both produce equal
score-before/after and cycle-error values. Final targeted run timing is recorded
in `/tmp/phase7b-timing-core.log` below; it includes store setup inside the cycle,
inline capture reads/serialization/writes/fsync, and runtime scheduling. Observer
construction is outside the timed region. Each condition is measured once; no
confidence interval or causal overhead estimate is justified. Earlier runs varied
substantially, so this is only a small development observation, not a benchmark
of populated consolidation. Expected read/locking/memory/disk/async effects and
unsupported capture conditions are documented in `PHASE7B_DESIGN.md`.

The final approved targeted run from `core/` measured **40.175 ms enabled** and
**89.123 ms disabled**. An earlier sandbox measurement was **157.939 ms enabled**
and **86.494 ms disabled** (`/tmp/phase7b-timing.log`). A root invocation with
`--manifest-path core/Cargo.toml`, which does not load `core/.cargo/config.toml`,
measured **45.885 s enabled** and **12.317 s disabled** while other checks were
running (`/tmp/phase7b-timing-final.log`). The root workspace invocation also
required another approved retry for its separately built protoc helper. The final
workspace check uses the normal `core/` configuration and passes.

These single, sequential observations are strongly affected by setup, scheduling,
configuration, caching and concurrent development load. No warmup, randomized
ordering, replications, confidence interval or populated-corpus measurement was
performed. The faster enabled sample is not evidence of a speedup, just as the
slower samples do not estimate a stable instrumentation cost. Instrumentation adds
real reads, hashing, serialization and durable disk writes as detailed in the
design. No zero-overhead or general behavioral-equivalence claim follows.
