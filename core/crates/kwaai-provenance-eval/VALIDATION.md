# Validation, test coverage and limitations

This document describes how to validate `kwaai-provenance-eval`, what its tests
cover, and what it does not establish. Recorded results for each validated
release are kept outside the repository, in the release validation manifest.

## How to validate

Run from `core/`. The repository's patched dependencies must exist first
(`bash patches/fetch-patches.sh`).

| Command | What it covers |
| --- | --- |
| `cargo test -p kwaai-provenance-eval` | Framework unit and integration tests, and the Phase 5 adapter tests. |
| `cargo test -p kwaai-provenance -p kwaai-provenance-rag --all-features` | The producer crates the adapter imports from, including the Phase 4 verifier and Phase 5 shadow tests. Mock HTTP tests need local loopback sockets; the live Ollama smoke test stays ignored. |
| `cargo run -p kwaai-provenance-eval -- run crates/kwaai-provenance-eval/fixtures/config.json` | Synthetic eight-cell smoke run; add `--resume` to check resumption. Writes to the fixture's configured output directory, which must not already contain the run. |
| `cargo clippy -p kwaai-provenance-eval --all-targets -- -D warnings` | Lints. |
| `cargo check --workspace --all-targets` | Workspace build with default features. |

Fixture runs are software-validation outputs, not empirical results. Workspace
checks use the default feature combination; they are not a claim that the full
workspace test suite or hardware feature matrix was run.

## Phase 5 adapter contract and coverage

Integration adapter v1 consumes actual Phase 5 schema-1 serialized evaluations
inside the actual CLI framing. Fixtures call the producer's `evaluate` and serde
serialization, rather than inventing evaluation JSON; negative tests mutate that
serialization. The separate hash-bound sidecar supplies missing experiment metadata
as explicitly identified caller assertions. Output normalized schema remains 1;
run/checkpoint schema is 3, with persisted attribution checked during resume.

The nine adapter tests in `tests/phase5.rs` cover these twenty scenarios (several use tables):

| Scenarios | Test coverage |
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
and corpus mapping. The 22 framework tests in `tests/evaluation.rs` retain resume metric corruption,
implementation fingerprint, sampling, annotations and checkpoint recovery coverage.

## Framework test coverage

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

This integration is ready as an offline input boundary for subsequent controlled
research with retained records and independently audited annotations. It does not
establish readiness for live gating, authenticated proposal capture, production
rollout or causal hallucination-reduction claims. Those uses require trusted
lineage validation, audited capture boundaries, fixed corpus/model protocols,
independent labels and an authorized intervention design before proceeding.

## Further limits

- An interrupted JSON write still requires explicit quarantine; retained
  observations are mandatory for resume.
- Implementation fingerprints attest the listed source bytes and dependency
  resolution, not external toolchains, binaries, environment or hardware.
- Original evaluator elapsed time is not recoverable by replay.
- Recorded seeds alone do not establish deterministic model behavior.
- Confidence intervals and empirical protocol limits are documented in
  [README.md](README.md).
