# Native evidence verification (Phase 4)

Verification is explicitly invoked offline. It never participates in a dream
cycle or mutates a graph. Source traceability and factual support remain separate.
Historical `UnresolvedVerifier` gate outcomes remain unresolved observations.

## Reused infrastructure and inspection

- `kwaai-provenance` v1 `Claim`, `EvidenceRef`, IDs, `SupportStatus`, and
  `VerificationResult` are unchanged. Claim/evidence IDs are domain-separated,
  length-prefixed SHA-256 identities. Parent IDs are informational, not identity.
- `LineageLog` committed ancestry and strict JSONL replay remain authoritative for
  derivation, not semantic support. No new ancestry or mutation is fabricated.
- `Gate::decide(&assessment.result)` accepts a new verification result. It retains
  Observe/Enforce semantics; this implementation never invokes enforcement.
- `MetaStore::get_chunks` retrieves original text by chunk ID, scoped to tenant.
  `OriginalEvidenceResolver` wraps it; graph retrieval, summaries and `surrounding`
  are never substituted for original text. A new read-only open path avoids the
  normal store opener's schema initialization and writes.
- Observer lifecycle is unchanged: cycle start captures original chunks and their
  exact text SHA-256, stage callbacks reconcile facts, finalization verifies the
  persisted lineage before writing the cycle summary. Callbacks retain shared
  graph access and failure isolation.
- Existing inference code uses reqwest/Ollama HTTP but graph extraction helpers
  do not expose this verifier's output schema and deterministic decoding contract.
  The backend reuses the workspace transport rather than extraction prompts.
- Existing observer tests, graph noninterference tests, schema/gate/lineage and
  persistence tests remain in place. `ollama` is optional in provenance;
  `verification` enables the evaluation binary in provenance-rag. Default builds
  do not run verification. The existing RAG `pdf` feature is unrelated.

This module builds on the Phase 3 observer (`feat/provenance-observe`). Model
identity is recorded per run: the backend records the Ollama model tag and digest
it used, and the release validation manifest records the environment for each
validated release. The embedding model is not used for factual verification.

## Requests and source identity

The CLI accepts a JSONL manifest of `VerificationRequest` records. Each contains:

```text
claim: existing Claim {id, kind, subject, predicate, object}
evidence: existing EvidenceRef[] {id, kind, source_id, content_hash, chunk_id, parent_ids}
entity_bindings: map of exact entity:N IDs to names from the corresponding snapshot
metadata: optional provenance context (lineage digest, graph snapshot identity, cycle)
```

Use serialized existing claim/evidence records. A caller that constructs these
from an independently retained graph/source snapshot must use `Claim::new` and
`EvidenceRef::new`, retain that snapshot's original hashes, and document the
snapshot identity. The observer's `facts::snapshot` supplies its claim objects;
entity names must come from the corresponding graph, not generated summaries.
Source namespace must be `kb:<tenant UUID>` for this resolver. Evidence hashes
must be lowercase SHA-256 over the original chunk's exact UTF-8 text, matching
the observer's hashing scheme.

**Historical observer JSONL alone is insufficient to reconstruct a request:** it
stores claim IDs and gate outcomes, not full claim text, names or evidence refs.
The tool deliberately does not invent those fields or replace historical hashes
with current hashes. Without a corresponding snapshot/request manifest, historical
factual verification cannot be completed. Changing current text yields an explicit
hash mismatch; it does not silently establish a new source identity.

## Running a bounded evaluation

From `core/`:

```bash
cargo run -p kwaai-provenance-rag --features verification --bin provenance-verify -- \
  /path/requests.jsonl /path/source-store TENANT_UUID CORPUS_SNAPSHOT_ID \
  /path/new-results.jsonl 100
```

Optional final argument: an Ollama configuration JSON file containing all fields:

```json
{"endpoint":"http://localhost:11434","model":"llama3.1:8b","timeout_secs":120,"seed":0,"num_ctx":8192,"num_predict":512}
```

The manifest is bounded to 16 MiB; LIMIT is 1..10000 and only its first LIMIT
records are evaluated. Malformed requests abort before output creation. The source
store must already exist. Output uses exclusive creation and cannot overwrite a
baseline or earlier result. Use a stable corpus snapshot to avoid concurrent edits.
No ingestion, model download, or experimental baseline changes occur.

Output JSONL contains a run header, one assessment per claim, and a summary:
claims evaluated, supported, unsupported, unresolved, missing evidence,
verification failures, runtime seconds. Missing evidence is also counted in
verification failures; ordinary semantic uncertainty has no failure. Input or
output I/O errors abort the run; partial output without a final summary is not a
completed evaluation. A model startup failure is recorded and each eligible claim
becomes unresolved while missing evidence remains separately identifiable.

Run metadata records Git commit and dirty state, verifier version, model tag and
installed digest when available, inference configuration, corpus/tenant identity,
manifest hash, timestamp, limit and input budget. Every assessment retains the
request, original source/document identifiers, recorded hashes, retrieved text and
computed hashes for validated sources. Results use an additive verification
schema envelope around the unchanged provenance `VerificationResult`. Do not
append these envelopes to the observer's v1 lineage JSONL reader.

## Interpretation and limits

The prompt asks whether original evidence entails the entire precise claim.
Explicit contradiction maps to unsupported; insufficient or mutually conflicting
evidence maps to unresolved. Malformed classifications, missing rationale,
incomplete generation, retrieval failures, missing/derived evidence, invalid IDs,
missing entity bindings, hash drift and unavailable inference cannot become
supported. Large inputs are rejected rather than silently truncated. The backend
reserves context space conservatively using a byte budget.

Temperature 0, seed 0, top_k 1, top_p 1, fixed context/output limits and stream=false
reduce variability; model runtime/hardware and tag changes can still affect
reproduction. Digest discovery does not lock an Ollama model tag against concurrent
replacement. Bindings and corpus identifiers are caller assertions; verify them
against the retained snapshot before scientific evaluation.

Mock tests use independently specified fixture labels and scripted responses to
test orchestration and failure behavior. They do not establish model NLI accuracy.
The optional live smoke test verifies transport and schema only. No accuracy or
false rejection rate is reported without independent ground truth. No pilot
baseline or scientific protocol has been changed.

## Validation

Run from `core/`. The repository's patched dependencies must exist first
(`bash patches/fetch-patches.sh`).

| Command | What it covers |
| --- | --- |
| `cargo test -p kwaai-provenance -p kwaai-provenance-rag --all-features` | Unit, evidence-verification, observer and verification integration tests, including the Ollama backend against scripted mock HTTP responses. Needs local loopback sockets. |
| `cargo test -p kwaai-provenance --no-default-features --test evidence_verification` | Deterministic verification tests with no model or network. |
| `cargo test -p kwaai-rag dream_observer` | Observer lifecycle and graph noninterference. |
| `cargo test -p kwaai-provenance --features ollama local_ollama_smoke -- --ignored --nocapture` | Optional live smoke test against a local Ollama model. Ignored by default; checks transport and schema only, never accuracy. |
| `cargo clippy -p kwaai-provenance -p kwaai-provenance-rag --all-targets --all-features -- -D warnings` | Lints, including the optional Ollama and CLI code. |
| `cargo check --workspace --features kwaai-provenance-rag/verification` | Workspace build with the verifier CLI enabled. |

Recorded results for each validated release are kept outside the repository, in
the release validation manifest. Passing these checks establishes software
behavior only: it is not an accuracy, false-rejection or calibration result, and
workspace compilation is not a claim that the full workspace test suite or
hardware feature matrix was run.
