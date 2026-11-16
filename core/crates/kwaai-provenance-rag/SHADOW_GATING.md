# Experimental provenance shadow gating (Phase 5)

This opt-in research facility compares hypothetical decisions without changing
DreamRAG graph contents. Nothing registers a new production hook or enables
Enforce. There is no claim of empirical hallucination reduction.

## Implemented boundaries

`kwaai_provenance::shadow::compare` is a pure deterministic engine. It maps the
three named policies onto the existing gate `Policy`, without inference, rewriting
verification results, or applying gate outcomes:

| Policy | Supported | Unsupported | Unresolved |
| --- | --- | --- | --- |
| Allow | accept | accept | accept |
| RejectUnsupported | accept | reject | accept |
| RequireSupported | accept | reject | reject |

`kwaai_provenance_rag::shadow::verify_post_stage` consumes an explicit retained
request from a completed stage, calls the existing Phase 4 `verify` once, and
compares all policies over the same immutable result. It uses the caller's
resolver/backend (including `OriginalEvidenceResolver`); evidence retrieval and
semantic verification are not reimplemented. It rejects proposal attribution.
This is an explicit API, not an automatic live observer or snapshot exporter.

`shadow::evaluate` consumes an existing request plus assessment. Both completed
stage observations (`post_stage`) and explicitly retained genuine proposals
(`retained_proposal_replay`) are supported. Attribution is supplied, not
independently established. There is no live pre-mutation evaluation capability.
Historical observer gate JSONL contains insufficient claim/evidence text, bindings
and hashes to build these records. Missing historical requests are not invented.

## Runner and schema

From `core/`:

```bash
cargo run -p kwaai-provenance-rag --features verification --bin provenance-shadow -- \
  /path/stage-assessments.jsonl /path/new-shadow.jsonl 100 RUN_ID CORPUS_SNAPSHOT_ID
```

Input JSONL contains `ShadowInput` records (the Rust type is the schema authority):

```text
{
  context: {run_id: RUN_ID, cycle: u64, stage: lifecycle stage name,
            kind: "post_stage" | "retained_proposal_replay"},
  request: Phase 4 VerificationRequest,
  assessment: Phase 4 EvidenceVerification,
  ground_truth?: {status: "supported" | "unsupported", source: annotation dataset/version}
}
```

A Phase 4 `evidence_verification` output's `request` and `assessment` can be
retained verbatim and associated with explicit snapshot attribution. That
association must be justified by the producer; the runner does not infer it from
current graph state. Keep corpus identity and available snapshot/lineage hashes
in `request.metadata`. Keep the assessment's backend/model identifier, available
digest and inference configuration. Absent historical metadata remains absent;
the runner does not discover a current model digest and substitute it for an old
one. Claim/evidence IDs, recorded hashes, retrieved hashes, rationale, failures,
verifier and backend metadata are preserved in every evaluation.

The manifest is capped at 16 MiB, LIMIT is 1..10000, and only its first LIMIT
records are selected. Empty input produces zero counts and null rates. Selected
malformed envelopes, mismatched claim/evidence IDs between request and assessment,
unsupported verification schema versions, empty run/stage IDs, run-ID mismatches,
and invalid ground-truth declarations fail before output creation. Records beyond
LIMIT are not parsed; their bytes still contribute to the manifest hash.
Counts are observations, not deduplicated claim IDs: repeated claims from separate
stages remain separate observations.

Output JSONL schema version 1:

- `shadow_run`: timestamp, run ID, current Git commit/dirty state, exact manifest
  SHA-256, limit, supplied corpus identity and metadata trust descriptions.
- `shadow_evaluation`: `evaluation` containing schema version, unchanged `input`,
  computed `provenance_validation`, and three decisions in stable policy order.
  Each decision contains claim ID, unchanged status, policy, accepted boolean,
  deterministic reason code, and run/cycle/stage/timing context. These are
  hypothetical decisions; there is no applied-mutation field.
- `shadow_summary`: schema version, summary counts/metrics and elapsed runner
  runtime seconds. Verification runtime is not measured by replaying assessments.

The output is exclusively created, never overwritten. An I/O failure may leave a
partial file; a completed experiment requires the final summary. The CLI never
opens a graph, source database, lineage log, or inference connection.

## Provenance and metadata trust

`Authenticated`, `Unverified`, and `Invalid` are distinct schema values. This
implementation never emits Authenticated: no trusted lineage association
validation is available at this boundary. Internally consistent IDs and a caller's
snapshot/lineage digest are insufficient authentication.

An observation is Invalid when request identity consistency fails or Phase 4
reports InvalidIdentity/HashMismatch; otherwise it is Unverified. Invalid identity
observations remain recorded rather than silently disappearing. Invalid does not
change the retained verifier verdict: even Allow accepts such observations
hypothetically. Provenance is a separate reporting axis, not a fourth policy.
Missing evidence and retrieval/inference failures are retained with their exact
Phase 4 failure category and unresolved outcome. A retained assessment is supplied
data, not independently reverified by replay; use `verify_post_stage` for fresh
Phase 4 validation of a retained request against original passages.

Only current Git state, exact manifest bytes and local identity consistency are
computed by the runner. Corpus identity, stage/cycle/timing attribution, associations,
backend metadata, source retrieval history and independent-label declarations are
supplied. The retained assessment records Phase 4's original-source checks; this
is not proof of lineage authentication.

## Metrics and independent annotations

Summary includes total, supported/unsupported/unresolved, missing-evidence and all
verification failures (missing evidence is included), invalid/unverified counts,
and per-policy accepted/rejected counts, rejection rate and unresolved handling.
Rates have null values for zero denominators.

Optional ground truth must be independently annotated, with a nonempty source
identifier; unresolved labels are rejected. Independence is a caller assertion,
not authenticated by this implementation. Never populate labels from verifier
outputs. Without labels, label-dependent rates remain null. With labels, only
labeled observations enter the confusion counts:

- False rejection rate = rejected independently supported / independently supported.
- False acceptance rate = accepted independently unsupported / independently unsupported.
- Unsupported-detection precision = rejected independently unsupported / all rejected labeled.
- Unsupported-detection recall = rejected independently unsupported / independently unsupported.

Partial labels are permitted and the labeled count is explicit. Such metrics do
not automatically establish representativeness or scientific accuracy.

## Proposal capture feasibility assessment

Inspection boundary: `kwaai-rag/src/dream.rs`, `dream_observer.rs`, `graph.rs`, and
`kwaai-provenance-rag/src/plan.rs`. The provenance planner diffs snapshots after
mutations; its lineage steps are not production mutation proposals.

- `EntityCompletion` in `dream.rs` holds proposed schema type, fields, description
  and relation names before Step 5. It is not a final graph mutation plan. Field
  merging, description recomputation and embedding success determine the actual
  entity update inside the application loop. Description/schema type are outside
  the provenance fact model, which tracks structured fields and relations.
- Relations become applicable only after target-name lookup in the current graph,
  `no_relations` filtering, and entity chunk inheritance. Placeholder chunk 0 may
  be used. Raw completion relations do not carry independently authenticated
  relation-specific source passages. A callback over raw completions would be
  useful diagnostics, but would not capture final verification-ready proposals.
- Merge pairs and zombie IDs are local pre-application candidate vectors. Merge
  rewrites many facts internally. Their current observer notifications occur after
  writes; candidate IDs alone lack full claim/evidence payloads and original hashes.
- `GraphStore::sanitize_relations` discovers and applies deletions, inverse additions
  and strength changes internally. There is no read-only full proposal plan exposed
  before those writes.

A uniform, safe capture facility needs a prepare/apply boundary that materializes
final claim changes with original evidence identities before each mutation,
including inside merge/sanitize operations. Adding a raw completion callback alone
would not meet that contract. Capturing final proposals now requires instrumentation
through application paths or a production planning refactor, with failure isolation
and additional regression work. No such changes were made in this bounded research
implementation. No mutations were blocked, reordered, rewritten or suppressed.

## Scientific limits and Phase 6 readiness

The Phase 4 limitations remain: no automatic historical snapshot-to-manifest
exporter, no trusted authentication of claim/evidence associations against lineage,
no independently established semantic-verifier accuracy, and no production graph
enforcement. No live corpus experiment, pilot replacement or empirical model
accuracy evaluation was performed. Deterministic mocks test orchestration only.
Post-stage observations have survivorship bias and cannot measure how rejected
pre-mutation proposals would have changed subsequent stages or retrieval quality.

Ready for bounded Phase 6 comparisons on explicitly retained stage requests and
assessments, including independently labeled fixtures/datasets when available.
Not ready for production enforcement, causal hallucination-reduction claims,
pre-mutation gate comparisons, or historical pilot replay without matching retained
snapshots. Phase 6 needs audited snapshot exports, authentic association validation,
independent labels and a fixed corpus/model protocol before accuracy, false rejection,
retrieval cost or causal effects can be claimed.
