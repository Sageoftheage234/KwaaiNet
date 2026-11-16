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
