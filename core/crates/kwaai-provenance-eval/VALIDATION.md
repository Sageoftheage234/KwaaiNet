# Phase 5 → Phase 6 integration validation

Executed 2026-11-16 in `/home/kwaai/KwaaiNet-Integration`, branch
`feat/provenance-integration`, base HEAD
`bc00a14cf9b20a93b297417bf36b80b8e76a05c4`. Changes are uncommitted.
Neither original development worktree was modified. This section supersedes the
historical reports below for the integration worktree.

| Exact command (from core/) | Final result |
| --- | --- |
| `cargo test -p kwaai-provenance-eval` | **33 passed, 0 failed, 0 ignored**: 2 unit, 22 existing integration, 9 new integration; binary/doc suites have 0 tests |
| `cargo test -p kwaai-provenance -p kwaai-provenance-rag --all-features` | **83 passed, 0 failed, 1 ignored**, approved loopback-capable run |
| `cargo fmt --all -- --check` | Passed, exit 0 |
| `cargo clippy --workspace --all-targets -- -D warnings` | Passed, exit 0 |
| `cargo check --workspace --all-targets` | Passed, exit 0 |
| `git diff --check` | Passed |

Combined regression breakdown: provenance unit 45; evidence verification 4 passed,
1 ignored; provenance-rag unit 14; observer integration 9; shadow integration 7;
verification integration 3; provenance doc test 1. The ignored test requires a live
Ollama model. No GPU/model experiment ran. Workspace checks use default features,
not the hardware feature matrix; the entire workspace test suite was not run.

Environmental exceptions and recovered failures:

- Missing generated dependency patches initially prevented Cargo resolution.
  `bash patches/fetch-patches.sh` failed sandbox DNS, then passed with approved
  network access. Only this worktree's ignored dependency directories were generated.
- The sandbox Phase 4/5 run had 43 passes and 2 failures, 0 ignored in its first
  unit suite: existing mock HTTP fixtures could not bind loopback (`PermissionDenied`).
  Cargo stopped before later suites. The identical approved command passed as above;
  no test code was modified to bypass sockets.
- Initial workspace Clippy/check attempts failed existing build-script downloads
  of protoc v28.3. Approved network reruns downloaded the compiler successfully.
  Cached build-script informational warnings remain; final checks exited 0.
- Clippy found one new test's unnecessary clone; it was fixed with
  `std::slice::from_ref`. Final Clippy and tests pass without lint allowances.

Final local logs: `/tmp/phase6-test.log`, `/tmp/phase45-test-approved.log`,
`/tmp/phase6-fmt.log`, `/tmp/phase6-clippy-final.log`,
`/tmp/phase6-check-final.log`. Earlier environmental failures are retained in
`/tmp/phase45-test.log`, `/tmp/phase6-clippy.log`, `/tmp/phase6-check.log`.

## Contract and coverage

Integration adapter v1 consumes actual Phase 5 schema-1 serialized evaluations
inside the actual CLI framing. Fixtures call the producer's `evaluate` and serde
serialization, rather than inventing evaluation JSON; negative tests mutate that
serialization. The separate hash-bound sidecar supplies missing experiment metadata
as explicitly identified caller assertions. Output normalized schema remains 1;
run/checkpoint schema is 3, with persisted attribution checked during resume.

The nine new tests exercise the requested twenty scenarios (several use tables):

| Requested scenarios | Test coverage |
| --- | --- |
| 1–3 supported, unsupported, unresolved across policies | `actual_serialization_all_nine_policy_decisions` also checks Phase 6 mappings |
| 4–7 missing/duplicate/incorrect policies, unsupported versions | `rejects_malformed_policies_statuses_versions_and_identity` also covers unknown policy, reason and contradictory status |
| 8 missing identity | Same table rejects empty original run; sidecar test rejects empty model |
| 9–11 Invalid, Unverified, promotion | `invalid_and_unverified_remain_distinct`; malformed table rejects Authenticated |
| 12–13 post-stage, retained replay | Default genuine post-stage fixture; `retained_replay_is_an_unauthenticated_timing_assertion` |
| 14–16 missing model/corpus and changed sidecar hash | `required_sidecar_metadata_and_hash_binding` |
| 17 resume changed input | `end_to_end_evaluation_independent_annotations_and_resume_integrity` changes both sidecar and exact imported bytes, even with valid rebound hashes |
| 18 independent annotations | `producer_labels_are_retained_but_never_become_independent_annotations` covers missing, unresolved and conflicting labels |
| 19 repeated IDs | `repeated_ids_and_mixed_timing_are_rejected` |
| 20 successful import/evaluation | End-to-end fresh and resumed run with all three policies |

`evidence_references_are_preserved_and_missing_mappings_rejected` additionally
checks actual original evidence IDs/content hashes against the retained payload
and corpus mapping. The existing 22 tests retain resume metric corruption,
implementation fingerprint, sampling, annotations and checkpoint recovery coverage.

## Exact changed-file inventory

Relative to repository root:

- `core/Cargo.lock`: two path dependency names in the evaluation package entry.
- `core/crates/kwaai-provenance-eval/Cargo.toml`: actual producer dependencies.
- `core/crates/kwaai-provenance-eval/src/phase5.rs`: new versioned adapter and attribution.
- `core/crates/kwaai-provenance-eval/src/lib.rs`: runner dispatch, corpus/model checks,
  checkpoint v3 and attribution resume checks.
- `core/crates/kwaai-provenance-eval/src/identity.rs`: integration/producer fingerprint coverage.
- `core/crates/kwaai-provenance-eval/tests/phase5.rs`: new deterministic integration tests.
- `core/crates/kwaai-provenance-eval/ADAPTER.md`: authoritative contract documentation.
- `core/crates/kwaai-provenance-eval/README.md`: reproducibility and schema updates.
- `core/crates/kwaai-provenance-eval/VALIDATION.md`: this report.

No Phase 5 or Phase 4 source modifications were needed. Production graph behavior,
Python scaffold and five-cycle pilot are unchanged. No commit, push, merge or PR.

## Research limitations and later-phase readiness

1. There is no authoritative lineage association authentication. Invalid remains
   Invalid; Unverified remains unknown; no imported record is authenticated.
2. Model, corpus association, seed, stage and retained proposal timing are caller
   assertions. Hash binding proves integrity, not truth, authorship or capture time.
   Available producer metadata is retained; unknown weight digests remain unknown.
3. Post-stage observations have survivorship bias. Retained replay cannot establish
   genuine pre-mutation capture. Mixed timing/stage categories fail within a cell;
   no cross-cell pooled headline metric or causal intervention is produced.
4. Independent annotations require external collection and audit. Missing, conflicting
   and unresolved labels remain excluded from binary metrics. Producer labels never
   become ground truth automatically. No verifier accuracy was estimated.
5. Fingerprints identify covered source/lock bytes, not a signed binary or attested
   execution environment. Checkpoints are not a tamper-proof archive. Original
   retained bundles remain mandatory for resume and audit.
6. No live interception, enforced gating, historical snapshot exporter, graph
   intervention, retrieval-quality study or GPU experiment is included.
7. Producer policy arrays must remain in their documented stable order. An upstream
   schema change requires a new explicit compatibility contract. Repeated claims
   within a cell are rejected rather than arbitrarily deduplicated.

Phase 7/8 specifications are not present in the inspected provenance crate docs.
This integration is ready as an offline input boundary for subsequent controlled
research with retained records and independently audited annotations. It does not
establish readiness for live gating, authenticated proposal capture, production
rollout or causal hallucination-reduction claims. Those uses require trusted
lineage validation, audited capture boundaries, fixed corpus/model protocols,
independent labels and an authorized intervention design before proceeding.

---

# Historical Phase 6 reports (different worktree; not current integration results)

# Phase 6 validation report

Executed 2026-11-16 in `/home/kwaai/KwaaiNet-Experiments`, branch
`feat/provenance-experiments`, base/HEAD
`1baf356ad28cc0958903e31e8a7fe6e21a599776`. Changes are uncommitted.
The Phase 5 worktree `/home/kwaai/KwaaiNet` was not modified.

## Blocking-finding remediation validation (current)

The two audit blockers are remediated without changes to production crates,
Phase 5, the protected Python scaffold, original documents or the original pilot.
Run/checkpoint envelopes are schema 2; corpus/annotation/config/adapter schemas
remain 1. Historical schema-1 runs are preserved and intentionally cannot resume.

Checks rerun after the final remediation source change, from `core/`:

| Exact command | Current result |
| --- | --- |
| `cargo test -p kwaai-provenance-eval` | Passed: 2 unit + 22 integration = **24 passed**, 0 failed, 0 ignored |
| `cargo fmt --all -- --check` | Passed |
| `cargo clippy --workspace --all-targets -- -D warnings` | Passed |
| `cargo check --workspace --all-targets` | Passed |
| `cargo test -p kwaai-provenance -p kwaai-provenance-rag --all-features` | Passed outside sandbox: **76 passed, 1 ignored**, 0 failed |
| `git diff --check` | Passed |

Phase 4 breakdown: provenance unit tests 45; evidence-verification integration 4
passed/1 ignored; provenance-rag unit tests 14; observer integration 9;
verification integration 3; provenance doc test 1. The ignored test requires a
real local model. The initial sandbox invocation again produced 43 passes and 2
mock HTTP failures because loopback binds were denied. The approved identical
rerun passed; no live model or GPU experiment was invoked.

Additional CLI checks used a copy of the fixture configuration at
`/tmp/kwaai-phase6-remediation-hf8qtc54/config.json`, changing only run ID/output
location. Both commands passed:

```text
cargo run -p kwaai-provenance-eval -- run /tmp/kwaai-phase6-remediation-hf8qtc54/config.json
cargo run -p kwaai-provenance-eval -- run /tmp/kwaai-phase6-remediation-hf8qtc54/config.json --resume
```

Eight fresh cells, all four policy results per cell, and both completed attempt
reports have matching schema-2 evaluator identities. Independent Python SHA-256
checks against the eight source/build-input files and the documented aggregate
encoding matched the embedded fingerprint:

```text
aa2fa438f4696a6a18197eb37a19cf4c6f6af6468f0186f18c38854cf7504d49
```

The artifacts remain at
`/tmp/kwaai-phase6-remediation-hf8qtc54/validated-run-schema-2/` for local audit.
They are synthetic software-validation outputs, not empirical results. The
original schema-1 smoke directory was not changed or migrated.

New regression coverage checks empty/missing/unexpected/duplicate policy results;
every missing metric field including nullable fields; incorrect denominators;
individually plausible rates with an inconsistent confusion matrix; altered
estimates (including a 1e-13 change), Wilson intervals, counts, nullability and
timing ratios; unchanged valid resume; a changed evaluator source identity;
legacy metadata; and a cell from a different evaluator. Invalid resumes preserve
the offending file and do not emit a successful attempt report. Unit tests prove
fingerprint determinism/order independence and sensitivity to source, schema and
lockfile changes. Existing corpus/input/annotation/recovery tests still pass.

Remediation changed-file inventory (relative to repository root):

- `core/crates/kwaai-provenance-eval/src/lib.rs`
- `core/crates/kwaai-provenance-eval/src/identity.rs` (new)
- `core/crates/kwaai-provenance-eval/tests/evaluation.rs`
- `core/crates/kwaai-provenance-eval/README.md`
- `core/crates/kwaai-provenance-eval/ADAPTER.md`
- `core/crates/kwaai-provenance-eval/VALIDATION.md`

The existing workspace/lockfile changes remain exactly the original member entry
and 13-line package entry. No new dependency or patch changes were needed.
The complete proposed Phase 6 inventory is now two tracked modifications and
21 untracked files in the new crate. Generated build artifacts under
`core/target/` were updated; the already generated patched dependency directories
remain ignored. No commit, push, merge or PR was performed.

Remaining limits: interrupted JSON writes still require explicit quarantine;
retained observations are mandatory for resume; fingerprints attest listed source
bytes/dependency resolution, not external toolchains, binaries, environment or
hardware. Original evaluator elapsed time is not recoverable by replay. Actual
Phase 5 mapping and cryptographic authentication remain separate integration
work. Cross-cycle correlation and empirical annotation/model provenance limits
remain as documented in README.md. The complete workspace test suite and GPU
feature matrix were not executed.

## Original implementation validation (historical)

The following table and smoke artifacts describe the initial implementation,
before the remediation above; their schema-1 outputs are not resumable by the
current implementation.

### Original checks

Commands below ran from `core/` except the explicitly repository-relative patch
bootstrap and Git checks. Workspace checks use the default feature combination;
`--all-targets` covers build targets, not every hardware feature combination.

| Command | Result |
| --- | --- |
| `cargo test -p kwaai-provenance-eval` | Passed: 14 integration tests; no ignored tests |
| `cargo fmt --all -- --check` | Passed for the workspace |
| `cargo clippy --workspace --all-targets -- -D warnings` | Passed |
| `cargo check --workspace --all-targets` | Passed |
| `cargo test -p kwaai-provenance -p kwaai-provenance-rag --all-features` | Passed: 76 tests including one doc test; one pre-existing local-model smoke test ignored |
| `cargo run -p kwaai-provenance-eval -- run crates/kwaai-provenance-eval/fixtures/config.json` | Passed: eight synthetic cells, four hypothetical policies per cell |
| Same `run` command with `--resume` | Passed; retained completed cells |
| `cargo run -p kwaai-provenance-eval -- inventory crates/kwaai-provenance-eval/fixtures/corpus /tmp/kwaai-phase6-validation-20261116-manifest.json` | Passed, exclusive output creation |
| `cargo run -p kwaai-provenance-eval -- check crates/kwaai-provenance-eval/fixtures/corpus /tmp/kwaai-phase6-validation-20261116-manifest.json` | Passed |
| `cargo run -p kwaai-provenance-eval -- sample crates/kwaai-provenance-eval/fixtures/manifest.json 42 1` | Passed |
| `git diff --check` (repository root) | Passed |

The final Phase 6 suite, formatter, workspace Clippy and workspace check were rerun
successfully after the final source change. The committed matrix fixture test
also executes fresh and resumed runs through the final implementation in a
temporary output directory.

## Deterministic test coverage

- Known SHA-256 vector; stable sorted manifests; changed original bytes detected;
  document ID remains stable across byte changes; symbolic links rejected.
- Reproducible sampling with a fixed expected rank order; excessive samples fail.
- Valid annotations, preserved disagreements, rejected duplicate IDs, evidence
  hash mismatch, malformed timestamps, unexpected fields and incompatible schema.
- Supported/unsupported rates and policy confusion matrix, Wilson interval
  bounds, baseline behavior, explicit unknown policy behavior and paired overhead.
- Missing independent labels and verifier-origin labels cannot produce factual
  rates; unresolved independent truth is excluded; missing timing/traceability and
  authentication stay unknown; authentication observations require an audit basis.
- Invalid positive provenance assertions fail rather than becoming support.
- Eight-cell/two-model/two-trial/two-cycle fixture, all policies and strict adapter
  compatibility/roundtrip; wrong imported seed fails with a failure report.
- Ground truth cannot cite evidence outside a trial's selected corpus.
- Failed cells generate incomplete attempt reports; missing matrices fail;
  existing runs cannot be overwritten; resumed cells retain exact bytes; changed
  input identities fail; missing cells recover; partial cell JSON is rejected.

## Environmental failures and recovery

The fresh worktree initially lacked the repository's ignored generated patched
crates. `cargo test` could not resolve `core/patches/cudarc/Cargo.toml`.
`bash core/patches/fetch-patches.sh` initially failed DNS resolution for
`static.crates.io` in the sandbox; it passed with approved network access and
materialized the existing checksum-pinned patches in this worktree only.

The initial workspace check and Clippy attempts failed because existing
`kwaai-p2p-daemon`/`kwaai-rpc` build scripts could not download `protoc` from
GitHub inside the sandbox. Approved reruns downloaded the existing required
version and passed. Later checks passed with those generated build artifacts.
Build-script informational warnings about that download remain in cached logs.

The initial feature-enabled Phase 4 regression test invocation failed two mock
HTTP tests because the sandbox refused loopback binds (`PermissionDenied`).
The identical command passed outside the sandbox. No real model was invoked;
the pre-existing live Ollama smoke test stayed ignored.

## Artifacts, scope and limits

The CLI smoke outputs are local, disposable recovery artifacts under
`/tmp/kwaai-phase6-fixture-output/synthetic-smoke`; they are not scientific results
and are not added to the repository. The inventory CLI artifact is the temporary
manifest named in the command table. Fixtures explicitly identify scripted models
and independently authored synthetic labels; no empirical accuracy, runtime gain
or graph-quality improvement is claimed.

The only tracked existing-file changes are the workspace member entry in
`core/Cargo.toml` and the new package entry in `core/Cargo.lock`. All implementation,
fixtures, tests and documentation are inside the new
`core/crates/kwaai-provenance-eval/` directory. Phase 4 interfaces, production graph
behavior, the protected Python provenance scaffold, original documents and the
original five-cycle pilot are unchanged. No commit, push or merge was performed.

The complete workspace test suite, GPU/hardware feature matrix and real model
experiments were not executed. The real Phase 5 producer mapping remains a
versioned adapter implementation task once its authoritative schema is available;
the normalized boundary and deterministic fixtures are usable now. Cryptographic
provenance verification remains the responsibility of that producer adapter.
Confidence intervals and empirical protocol limits are documented in README.md;
recorded seeds alone do not establish deterministic model behavior.
