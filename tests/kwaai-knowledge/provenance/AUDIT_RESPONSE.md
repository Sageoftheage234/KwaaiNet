# Response to the technical audit (KwaaiNet 486bcf6 provenance tooling)

Line numbers in the "verified" column refer to the **previous** package (`kwaainet-provenance-486bcf6.zip`). Every
finding was checked against that code before anything was changed.

| # | Finding | Verified against previous code | Change |
|---|---|---|---|
| 1 | Independent pytest timed out at `test_written_gates_after_passing_edit_check` | Could not reproduce: the previous suite ran 21 passed in 4.98 s here. That test starts about 30 subprocesses, so a slow process start or a hung child is the likely cause; the previous tests had no per-subprocess timeout | Tests now use a 60 s per-subprocess timeout (`kw` fixture), so a hang fails with a message instead of stalling |
| 2 | Gate timing compromises the comparison | **Correct.** `runner.py` L115–143: snapshot rows were taken after the arm's last operation, so gate_after was scored right after its own gate and gate_before after a dream | Rows at `pre_gate`, `post_gate`, `post_dream` with `end_of_cycle` and `primary` flags; `primary_phase` config (default `post_dream`); `gate_immediate` on post_gate rows |
| 3 | Text-derived fact IDs collapse distinct records | **Correct.** `facts.py` L13–14 hashed kind+text; `lineage.py` L53 `cur_by_id` and `runner.py` L151 `cls_by_origin.setdefault` keyed on it. Same-named entities (e.g. the known Rassool collision) produced identical keys | Separate `record_id` (persisted location), `content_id` (text, reported only), `origin_ids` (many-to-one). Duplicate record ids raise |
| 4 | Loss attribution is heuristic | **Correct, and worse than stated.** `runner.py` L42–61: `alias_names` included every alias ever recorded, so a pruned entity whose name matched any alias was called a merge; relation losses with live endpoints were labelled `rewrite`, but the 486bcf6 path that deletes them is `sanitize_relations` (dream.rs ~L1011) | Per-cycle, per-phase transitions; `cause` + `confidence` (`observed`/`inferred`/`ambiguous`/`unknown`) + candidates; merge needs a newly appended alias on exactly one survivor; prune needs observable preconditions and a consistent DreamReport count; gate-induced prunes flagged |
| 5 | Virtual and written gates answer different questions | **Partly correct.** `runner.py` L170–174 already noted "gate decisions not written; later cycles ran ungated", but `report.py` L13, L32–33 put `virtual_unsupported_present_over_cycle0` beside outcome columns without that note | `gate_mode` column (`written`/`virtual`/`not_computed`); block renamed `virtual_gate_immediate` with an explicit interpretation; plots mark virtual arms "ran ungated" |
| 6 | Retrieval cost incomplete | **Correct.** `config.py` L42 and `default.yaml` L21 set `eval_cycles: [0, 12]`; deltas had no spread; keys were named `retrieval_recall` | Eval at every snapshot by default; keys renamed `*_keyword_recall` with definitions; deltas index-matched per repeat (labelled as a convention) plus difference of means; Recall@k/MRR stay `not_computed` |
| 7 | Plots show one repeat | **Correct.** `report.py` L61 filtered `repeat == 0` | `<csv>.stats.csv` with every repeat value, mean, SD, min, max, 95% t-interval; plots show mean, interval and every repeat |
| 8 | Compatibility check lacks `rag graph score --json` and a field edit | **Correct.** `checks.py` L59–101 deleted one relation only and used `graph stats`. `rag graph score --json` exists at 486bcf6 (cli.rs ~L2167–2175, rag_cmd.rs ~L5014–5023) | Field removal **and** relation deletion; `graph stats` + `graph score --json` before and after; the edited entity must appear in the binary's score output; counts compared after the dream run too |
| 9 | Description provenance incomplete | **Correct, and worse than stated.** `facts.py` L54 skipped descriptions of field-bearing entities on a wrong premise (graph.rs ~L1057–1080 keeps the longest description at ingestion), and `runner.py` L150–166 counted description facts in the primary endpoint | Descriptions extracted for every entity, never gated, reported in a separate `descriptions` block; primary endpoint is field + relation only |
| 10 | Preserve strengths | Kept: isolation, per-fact evidence, stale-report handling, `no_similarity_merge`, `not_computed`, write gating; their tests remain | — |
| 11 | Authorization not bound to runtime | **Correct.** `runner.py` L22–33 checked only `status` and `snapshot_id` | `compat.py`: PASS bound to protocol, snapshot file hashes, config template, freeze commit, executable bytes and version, writer-code hash; mismatches listed key by key |

## Limits that remain
- Dream-step causes are inferred from state differences. 486bcf6 writes no per-operation log; the DreamReport has
  counts only, and sanitize deletions are not counted at all.
- The prune check uses the observable preconditions (mention count, degree, chunk links). It does not recompute
  `score_entity(...) < --prune-threshold`.
- The default primary phase (`post_dream`) gives gate_after one fewer gate than gate_before at each cycle. The
  alternative (`end_of_cycle`) scores gate_after right after its own clean-up. This is a preregistration choice.
- Repeats of different arms share only the starting snapshot; matching by repeat index is a convention.
- Not executed: the real `kwaainet` binary, the NLI scorer and Windows.
