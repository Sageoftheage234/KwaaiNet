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

Inspection confirmed branch `feat/provenance-verifier`, parent tip
`a609e759af8d3a9c9466ac509a8ee778667f9797` on `feat/provenance-observe`.
On 2026-11-15, `http://localhost:11434/api/tags` reported:

- `llama3.1:8b`: `46e0c10c039e019119339687c3c1757cc81b9da49709a3b3924863ba87ca666e`
- `nomic-embed-text:latest`: `0a109f422b47e3a30ba2b10eca18548e944e8a23073ee3f3e947efcf3c45e59f`

The embedding model is not used for factual verification.

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

## Validation executed on 2026-11-15

Commands below ran from `/home/kwaai/KwaaiNet/core` unless otherwise noted.

| Exact command | Outcome |
| --- | --- |
| `cargo test -p kwaai-provenance --all-features` | Initial implementation: 43 unit + 4 integration + 1 doc test passed. |
| `cargo test -p kwaai-provenance-rag --all-features` | Initial implementation: 14 unit + 9 observer integration tests passed. |
| `cargo test -p kwaai-provenance -p kwaai-provenance-rag --all-features` | Final transport/adapter/CLI implementation: 45 provenance unit + 4 evidence integration + 14 adapter unit + 9 observer integration + 3 verification integration + 1 doc test passed (76 total). Ran outside sandbox for loopback fixtures. |
| `cargo test -p kwaai-provenance --no-default-features --test evidence_verification` | 4 deterministic tests passed without model or network. |
| `cargo test -p kwaai-rag dream_observer` | 2 lifecycle/noninterference tests passed; other RAG tests filtered out. |
| `cargo test -p kwaai-provenance --features ollama local_ollama_smoke -- --ignored --nocapture` | 1 explicitly selected live smoke passed using installed llama3.1:8b; valid structured supported response with no failure, 4.83 seconds test runtime. No accuracy claim. |
| `cargo fmt --all -- --check` | Passed. |
| `cargo clippy -p kwaai-provenance -p kwaai-provenance-rag --all-targets --all-features -- -D warnings` | Passed, including optional Ollama and CLI code. |
| `cargo clippy --workspace --all-targets -- -D warnings` | Passed outside sandbox; existing daemon build script downloaded protoc. |
| `cargo check --workspace` | Passed. |
| `cargo check --workspace --features kwaai-provenance-rag/verification` | Passed. |
| `git diff --check` | Passed. |

The live smoke test was added after the combined suite run and then executed
explicitly; it is ignored during ordinary tests. The final feature-enabled strict
Clippy run also checked it.

Earlier attempts and environmental failures were not counted as passes:

- The combined feature-enabled test invocation from the repository root using
  `cargo test --manifest-path core/Cargo.toml -p kwaai-provenance -p kwaai-provenance-rag --all-features`
  failed when the sandbox refused both HTTP tests' loopback socket binds. The
  later combined invocation above passed outside the sandbox.
- The first workspace strict Clippy attempt failed because the existing build
  script could not resolve github.com to download protoc. The identical command
  subsequently passed outside the sandbox.
- One `cargo fmt --all -- --check` invocation from the repository root failed
  because Cargo.toml is under core; the corrected workspace invocation passed.

No tests under `tests/kwaai-knowledge/provenance/` or the existing pilot baseline
were modified or used to manufacture labels. The pre-existing untracked
`tests/kwaai-knowledge/provenance/` directory was left untouched. No corpus was
ingested, no dream graph mutation code was changed, and no push, merge or PR was
performed. Broad workspace compilation/Clippy are not a claim that the entire
workspace test suite or hardware feature matrix was executed.
