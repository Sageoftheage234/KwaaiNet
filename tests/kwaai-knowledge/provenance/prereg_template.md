# Provenance study pre-registration (commit alone, before any run past cycle 0)

## Question
Across repeated `kwaainet rag dream run` cycles (KwaaiNet 486bcf6), what fraction of graph facts remain traceable to
chunks that entail them? Does an evidence gate applied before each cycle leave fewer unsupported facts than the same
gate applied after?

## Fixed design
- Snapshot ID: `TBD` (from SNAPSHOT.json). KB: `TBD`. Binary version: `TBD`.
- Arms: ungated, gate_before, gate_after, no_similarity_merge, no_prune, no_relations.
- Dream model and inference URL: `TBD`; other flags at their defaults unless listed here.
- Snapshots: cycles 0, 1, 2, 4, 8, 12. Repeats: 3 (`TBD` if changed).
- Scorer: NLI `MoritzLaurer/DeBERTa-v3-large-mnli-fever-anli-ling-wanli`; θ = `TBD`; θ_gate = `TBD`; corpus top-N = 30.
- Facts: field and relation facts at fact-level evidence (gate-eligible; primary endpoint). Description sentences at
  entity-level evidence: never gated, reported separately, not part of the primary endpoint.
- Primary observation phase: `TBD` (`post_dream` or `end_of_cycle`; see FINDINGS.md, design decision 2).
- Edit-compatibility result: `TBD` (path, and its `binding` block: protocol, snapshot hash, binary hash and version,
  writer-code hash).

## Endpoints
- **Primary:** `unsupported_present_over_cycle0` at cycle 12, at the primary phase, gate_before vs gate_after, both
  with gate_mode `written` (mean over repeats, with every repeat value and the t-interval). A `virtual` arm is not
  a gate arm for this comparison.
- **Secondary** (Holm-corrected together):
  - traceability trajectory;
  - `added_unsupported`;
  - immediate gate effect (`gate_immediate` on post_gate rows) versus the effect carried into the next post_dream;
  - `lost` and loss attribution by cause and confidence; only `observed` and `inferred` causes are reported as
    causes, `ambiguous` and `unknown` are reported as such;
  - the ablation contrasts;
  - false-rejection rate and gate precision from the blinded 150-item sample, with 95% bootstrap CIs and a second
    rater's kappa;
  - retrieval and generation keyword-recall deltas versus ungated at every snapshot (index-matched per repeat and as
    a difference of arm means). These are keyword-coverage scores, not Recall@k;
  - description-sentence outcomes (entity-level evidence).
- Recall@k and MRR are not available from `rag eval` at 486bcf6 and are not reported.

## Thresholds
- Primary effect criterion: `TBD`.
- False-rejection tolerance: `TBD`.
- Retrieval-cost tolerance: `TBD`.

## Exclusions
- Cycles where the dream report shows destructive steps skipped (no completions plus errors) are reported, not
  dropped.
- Any measure that cannot be computed is recorded as `not_computed` with its reason.
- With 3 repeats the t-intervals are wide; they describe run-to-run variation of this pipeline.
