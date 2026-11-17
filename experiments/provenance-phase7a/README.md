# Phase 7A development feasibility pilot, v1

This is a development dataset and pipeline diagnostic, **not the final publication
corpus or a held-out evaluation**. No independent annotations were created.
Constructed expectations are authored diagnostic checks, not independent ground
truth. Neither diagnostic agreement nor natural verifier outputs estimate
publication-grade classification accuracy. No final publication dataset is defined.

## Preserved Stage 1 and Stage 2 findings

Audited starting branch `feat/provenance-integration`, then verified the user-created
`feat/provenance-phase7-pilot` branch at clean HEAD
`4c460d698a32d1378d5a9ca94f5b0451e98e1c4a` before Phase 7A changes.

The Phase 4 native verifier validates identities, original passage retrieval and
exact text hashes before inference. Explicit contradiction maps to `unsupported`;
insufficiency/conflict maps to `unresolved`. Infrastructure failures also yield
unresolved results but have separate failure fields. They must not count as
successful semantic uncertainty classifications. Hash consistency is not source
association authentication or factual support.

Phase 6 requires separate corpus-bound annotations with exact claim content,
evidence locators/hashes, annotator, rationale, timestamp and origin. Verifier
annotations cannot become ground truth; independent conflicts are excluded. The
Phase 5 adapter preserves original judgments and recomputes producer policies;
producer ground-truth fields do not populate Phase 6 annotations.

**Three-class scoring gap:** Phase 6 reports binary acceptance-policy metrics and
excludes unresolved reference labels from those binary denominators. It does not
provide the three-class classification matrix needed here. A small experiment-local
scorer is sufficient for Phase 7A. Production code and the evaluation framework
remain unchanged. Phase 6 scoring/integration was not executed in this diagnostic;
this pilot does not establish empirical correctness of its binary metrics.

Stage 2 found 390 unique natural claims (255 fields, 135 relations), 75 original
chunks (50,010 UTF-8 bytes), and entity records for all claim endpoints. All evidence
references resolved and all claim IDs occurred in proposed lineage outputs. There
are 138 single-chunk claims; eleven claims exceed the default input budget in text
alone. Full serialized input budgets must be checked before a larger sample.
Source material is plentiful, but independent labels and three-class coverage are
not established. Model QA answers and prior lexical support scores are not labels.

## Snapshot and diagnostic design

`development-v1/snapshot.json` identifies the copied source set as:

`phase7a-development-v1-a51142ff72050504b9f0727bd758a3cc864f42897f9b2cd8a1074d7eaaf23e02`

Its ID hashes canonical JSON of the file inventory, including exact byte hashes,
sizes and original paths. The manifest retains capture time, tenant and scope.
It covers three SQLite stores, all seven original lifecycle files, and three
original documents. Sources were read only: no WAL files were present, and all
source hashes matched before/after copying and after the diagnostic. This is a
stable copied current-state snapshot, not an atomic or authenticated historical
multi-file snapshot. The source corpus and source stores were not changed.

| Retained source document | Chunks |
| --- | ---: |
| Franck Report.html | 61 |
| Einstein-Szilard Letter.txt | 6 |
| Potsdam Declaration.html | 8 |

`natural-claims.json` retains all 390 native structured claims with current
evidence references and bindings. Their evidence hashes bind the captured current
text; they are not recovered historical hashes. `natural-diagnostic-sample.json`
contains three unlabeled natural claims: two fields and one relation. Selection
takes single-chunk claims ordered by content ID, in field/relation/field order,
without inspecting verifier predictions. It is a convenience diagnostic sample.

`constructed-challenges.json` contains nine separately identified constructed
claims: three authored expectations per class. They use two existing original
letter passages without modifying evidence. Their predicates and objects were
written for diagnostics; they are not graph observations. The expectations were
saved before live predictions and explicitly carry
`independent_ground_truth: false`. No annotation file asserts independent origin.
`requests.jsonl` combines the three natural and nine constructed requests only
for bounded execution. `diagnostics.json` binds all derived input files by hash.

## Executed checks and outcomes

Built the actual checked-out native verifier with:

```bash
cd core
cargo build --locked --offline -p kwaai-provenance-rag --features verification --bin provenance-verify
cargo build --locked --offline -p kwaai-provenance-rag --bin provenance-report
```

`verification-01.jsonl` retains the failed sandboxed transport attempt: all twelve
assessments were inference failures because localhost access was blocked. It is
not a classification experiment. The subsequent approved execution outside the
sandbox is retained separately as `verification-02.jsonl`:

```bash
core/target/debug/provenance-verify experiments/provenance-phase7a/development-v1/requests.jsonl experiments/provenance-phase7a/development-v1/store f89d5add-4335-4b23-be54-47820a20f155 phase7a-development-v1-a51142ff72050504b9f0727bd758a3cc864f42897f9b2cd8a1074d7eaaf23e02 experiments/provenance-phase7a/verification-02.jsonl 12
python3 experiments/provenance-phase7a/pilot.py score experiments/provenance-phase7a/verification-02.jsonl
```

Outputs are exclusively created: choose a new output filename for a rerun. The
scorer also refuses to overwrite its corresponding `.scores.json`. Retain changed
inputs in a new dataset version rather than editing this snapshot. The preparation
command `python3 experiments/provenance-phase7a/pilot.py prepare` refuses to replace
the existing `development-v1` directory; it captures sources, not a deterministic
historical reconstruction of them.

The successful run used `llama3.1:8b`, digest
`46e0c10c039e019119339687c3c1757cc81b9da49709a3b3924863ba87ca666e`,
seed 0, temperature 0, top_k 1, top_p 1, context 8192, output limit 512. Native
output retains configuration, model rationale, requests and validated passages.
Execution metadata retains the verifier binary hash, compiler version and source
hash checks. No model was downloaded. Twelve assessments completed in 26.75 seconds:
five supported, three unsupported, four unresolved; no missing evidence or
verification failures. These are verdict counts, not reference-label counts.

The scorer checks request identity, exact evidence references, source-text hashes,
input hash, output framing, counts and snapshot integrity. A deliberately
off-diagonal fixture checks matrix orientation and inclusion of unresolved.
Infrastructure failures are excluded from semantic completion counts and reported
separately. Natural claims remain unscored.

| Authored diagnostic expectation | Observed supported | Observed unsupported | Observed unresolved |
| --- | ---: | ---: | ---: |
| supported | 3 | 0 | 0 |
| unsupported | 0 | 3 | 0 |
| unresolved | 1 | 0 | 2 |

Eight of nine constructed challenges agreed with expectations. Challenge 09 asks
whether Congo uranium ore was priced at ten dollars per tonne. The passage gives
no price, but the verifier returned supported, citing Congo as a uranium source.
This identifies an unsupported-detail inference failure. The raw result and
rationale are preserved; no prompt/model tuning or label adjustment followed.
The tiny, correlated, deliberately constructed sample supports no accuracy claim.

## Observational consolidation feasibility

`lifecycle-validation.json` retains the native `provenance-report --csv` command,
successful exit, and output. Strict native replay validates 464 proposals and
464 commits. The reporter checks the latest summary's lineage digest and entry
count; this is not a claim that it checks every intermediate digest. Five completed
cycles report 262, 346, 378, 382 and 390 facts, with no reported drift or conflicts.
Actors include baseline, completion, merge and sanitize. All 266 gate records
used `unresolved_stub`, Observe mode and applied allow; they are not Phase 4 semantic
verdicts or evidence of intervention effectiveness.

**Suitable now:** descriptive structural counts, stage attribution, recorded
rewrites/removals, and a current-state source-associated diagnostic sample.
**Insufficient now:** factual-support trajectories before/after consolidation.
Historical records lack full claim content, contemporaneous entity bindings,
original passage hashes/text, and complete per-stage graph snapshots. Current
content cannot establish historical support, especially for removed/rewritten
claims. Do not fabricate missing proposals or label fresh verification as a
historical cycle assessment. Standalone diagnostic challenges have no genuine
cycle/stage; they were not forced through the Phase 5 adapter using invented context.

## Interfaces and metadata for the extended-corpus experiments

Before the main experiments, retain a versioned corpus inventory with acquisition
and transformation records, exact document bytes, stable document/chunk mapping,
locators, original chunk hashes and extraction configuration. Store document-level
and transformed-text hashes separately; the Phase 6 adapter needs an explicit
chunk/evidence-to-document mapping. Detect duplicates across formats and versions.

The missing experimental capture/export interface should save, for each genuine
stage/cycle, exact native claims, evidence references/hashes, matching entity
bindings, immutable original text, graph identity and lifecycle context. Export
removed/rewritten claims before they disappear. Retain full verification envelopes,
producer JSONL and typed Phase 5 sidecars; authenticate lineage separately if required.
Current lifecycle IDs alone cannot supply this interface.

Retain run/model digest, runtime, decoding configuration, seeds, extraction and
consolidation settings, complete repetition/cycle identity, errors and paired timing
boundaries. Model metadata in Phase 6 is caller asserted, not independently attested.
Provide a genuinely independent, preferably blinded annotation process with exact
claim content, evidence scope, rationale, annotator identity, disagreements and
separately retained adjudication. Assess actual support-class balance only after
annotation; do not assume contradiction examples exist naturally.

The eventual scoring protocol must decide whether a versioned external three-class
report suffices or framework functionality is needed. Keep failure rates and
coverage separate from semantic metrics, and account for shared-document/claim
dependence. Phase 7A does not authorize a production framework change.

## Incorporating the extension without held-out contamination

Audit the separately assembled extension when available; do not assign its final
version or freeze it here. Preserve this development snapshot, every inspected
passage and diagnostic challenge as development exposure. Before defining a final
evaluation split, deduplicate the extension against these documents, overlapping
passages, claims, paraphrases and source families. Split at document/source-family
level as appropriate; random claim splits can leak shared evidence. Any overlap
retained for study belongs in an explicitly exposed development stratum.

Use development material for pipeline, prompt and threshold decisions. Declare and
lock those decisions, the sampling/annotation protocol and scoring implementation
before opening held-out outcomes. An independent custodian should retain held-out
labels; avoid exposing held-out passages during development. Inventory exposure,
assign fresh corpus/run versions, and never append extension data or new labels to
this v1 snapshot. Freeze publication data only after its separate audit and approval.
Any later tuning informed by held-out results requires a new uncontaminated evaluation
set. Phase 7A results must remain separately reported development diagnostics.
