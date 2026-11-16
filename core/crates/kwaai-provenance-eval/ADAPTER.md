# Phase 5 → Phase 6 integration contract v1

`phase5::import` consumes a strict `Bundle`. The normal `run CONFIG [--resume]`
CLI detects bundles by `phase5_adapter_version` in each input cell. Existing
synthetic `FixtureAdapter` inputs remain supported, separately identified.
No graph is opened by the integration and no verification is rerun.

The authoritative serde structs are in `src/phase5.rs`:

- Bundle: `phase5_adapter_version: 1`, `phase6_schema_version: 1`,
  `source_jsonl` (exact UTF-8 Phase 5 CLI output), `sidecar`, `sidecar_sha256`.
- Sidecar: `schema_version: 1`, `source_sha256`, `corpus` (validated Phase 6
  Manifest), `model` (ModelConfig including available digest/parameters), `seed`,
  `repetition`, `evidence` (original evidence ID → Phase 6 Evidence), and nonempty
  `assertion_source` identifying the retained experimental protocol.
- `source_sha256` hashes exact source bytes. `sidecar_sha256` hashes compact serde
  JSON for the typed Sidecar (struct field order, BTreeMap key order). Use
  `object_hash(&sidecar)` to construct it. The complete bundle is also byte-hashed
  by the runner. There are no external mutable sidecar paths.

Create a bundle with Rust's exported Bundle/Sidecar types, retaining the unmodified
Phase 5 output in `source_jsonl`, then point an InputCell path at its serialized
JSON. No automatic model/seed discovery is attempted. Missing metadata must be
provided with a separately structurally validated sidecar; unavailable model
weight digests remain null. The corpus version must equal the producer header's
caller-supplied corpus identity. The manifest and model must exactly match the
Phase 6 experiment configuration, including available digest and parameters.
Evidence mappings must resolve against manifest document IDs and byte hashes and
be inside the selected sample. Original evidence IDs, passage hashes, source IDs,
chunk IDs, entity bindings, metadata, assessments and annotations remain in the
retained source; document mappings do not replace those references.

The parser requires actual Phase 5 `shadow_run`, `shadow_evaluation`,
`shadow_summary` framing and schema 1, plus verification schema 1. It validates
request/result identity consistency, recomputes Phase 5 identity/hash validation,
compares every decision (claim ID, status, context, acceptance and reason) with
`kwaai_provenance::shadow::compare` through the actual producer `evaluate`, and
checks the producer summary. Exactly three policies in producer order are
required; missing, duplicate, unknown, reordered or contradictory policies fail.

| Original status | Allow | RejectUnsupported | RequireSupported |
| --- | --- | --- | --- |
| Supported | accept | accept | accept |
| Unsupported | accept | reject | reject |
| Unresolved | accept | accept | reject |

Normalized judgment preserves the original status. Phase 6's policy mappings are
identical, including unresolved handling; baseline also accepts everything.
Claim content is the exact compact serde JSON of the original Claim struct,
including identity, kind and components. Independent Phase 6 annotations must use
that content string. Repeated claim IDs within a cell fail, including identical
repeats; occurrences across distinct cells remain separate observations.

`Imported` returns both `normalized` and the full original bundle, evaluations and
header. Normalization alone is not an archival export: retain the complete bundle.
Run/checkpoint schema **3** adds per-cell `phase5_attribution` with original run,
cycle, stage, timing kind, adapter/producer/evaluation schema versions, source and
sidecar hashes, authority disclaimer and provenance counts. Resume recomputes and
compares this attribution as well as metrics. Older run schemas require a new run.

Every cell must have one original run/cycle/stage/timing category; mixed categories
fail. No timing is inferred from timestamps, verification success or caller
proposal declarations. Both timing kinds retain `capture_timing_authenticated:
false`. Post-stage observations remain post-stage. Retained proposal replay is
only a caller assertion of prior capture. Headline metrics are per cell; no
cross-category pooled headline is produced. Do not interpret stage or replay
comparisons as a controlled intervention.

Phase 5's producer emits `Invalid` or `Unverified`; `Authenticated` is reserved
but never emitted. Any supplied Authenticated value, including an attempted
promotion of Invalid/Unverified, fails. Invalid maps to an assessed false with
an identity/hash failure basis; Unverified maps to null (unknown), never true or
false. Per-cell provenance counts report both exclusions and total observations;
authentication-rate denominator includes only Invalid assessments, with numerator
zero. No authenticated-only semantic metric is implemented. Evidence hashes and
sidecar bindings prove byte consistency, not association authenticity or execution
metadata. Traceability remains unknown. No capture duration is synthesized.

Producer ground_truth is retained only for audit, including its source assertion;
it never populates Phase 6 annotations. Phase 5 supports only sourced binary
labels in genuine evaluations. Phase 6's separate annotation protocol preserves
missing, conflicting and unresolved annotations and excludes verifier-origin
labels. Successful replay establishes neither annotation validity nor correctness.

The implementation fingerprint covers the integration module and the producer
policy, identity, schema, gate and verification source used by normalization, in
addition to prior evaluator/workspace/lock/toolchain inputs. Changing raw imports,
sidecars, config/model/manifest, annotations, schemas or implementation invalidates
resume. Matching hashes are integrity checks, not signatures or trusted lineage.
