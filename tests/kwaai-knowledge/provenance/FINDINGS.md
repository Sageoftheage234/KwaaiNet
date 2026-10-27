# Source findings — KwaaiNet at 486bcf6

Every statement below was checked against `git show 486bcf6:<path>`. Line numbers are approximate (±5).

## Storage
| Fact | Source |
|---|---|
| Graph DB is `graph-<tenant>.db`; tables `entities`, `relations`, `chunk_entity`, `entity_chunk`, `chunk_mentions`, `entity_timeline_v1`, `interactions` are `(key BLOB, value BLOB) WITHOUT ROWID`; `metadata` is TEXT/TEXT | `core/crates/kwaai-rag/src/graph.rs` L885–909 |
| Entity key = `id.to_le_bytes()`; value = `serde_json::to_vec(EntityNode)` | graph.rs L1129–1132 |
| Relation key = `src LE ‖ dst LE ‖ relation_type bytes` | graph.rs `relation_key`, L6108–6114 |
| `FieldValue { value, evidence_chunk_ids, confidence }` — **evidence is persisted per field** | graph.rs ~L197–205 |
| `RelationRecord.evidence_chunk_ids` — **evidence is persisted per relation** | graph.rs ~L283–310 |
| `EntityNode.evidence` is `skip_serializing` (rebuilt from `entity_chunk` at load); descriptions have no per-sentence evidence | graph.rs ~L255–260 |
| At ingestion the stored description is the longest of the field-derived, incoming and existing descriptions, so field-bearing entities can carry prose not derived from fields | graph.rs ~L1057–1080 |
| In a dream summary completion, the field-derived description wins when it is non-empty | dream.rs ~L820–835 |
| `entity_chunk` / `chunk_entity`: 8-byte LE id → JSON list of i64 | graph.rs `link_chunk`, L1461–1505 |
| MetaStore `<tenant>.db`, `chunks` key = tenant UUID bytes (16) ‖ chunk id LE (8), value = `ChunkMeta` JSON | `core/crates/kwaai-rag/src/meta_store.rs` L53–97 |

Consequence: field and relation facts are traced at fact level; description facts only at entity level.

## KB resolution (isolation)
| Fact | Source |
|---|---|
| Config home = `$KWAAINET_HOME` if set, else `~/.kwaainet`; config is `config.yaml` | `core/crates/kwaai-cli/src/config.rs` L16–41 |
| `--kb NAME` → `cfg.get_rag_kb(NAME)` from `rag_kbs` (legacy `rag` for "default") | `core/crates/kwaai-cli/src/rag_cmd.rs` `load_rag_config_for`, L2530–2544; config.rs L1172–1180 |
| KB data dir = `rag_data_dir` if set, else `<home>/rag` | config.rs `RagConfig::data_dir`, L717–722 |

Every subprocess here runs with `KWAAINET_HOME` set to a private home whose `config.yaml` points `rag_data_dir` at
that run's copy. The original `config.yaml` is read once at snapshot time and never written.

## Dream cycle
| Fact | Source |
|---|---|
| Steps: score → type/summary/relation completion (LLM) → auto-merge → prune → re-score | `core/crates/kwaai-rag/src/dream.rs` L1–11 |
| `DreamReport` fields (counts only; no entity ids) | dream.rs L76–88 |
| Report written to `<data_dir>/dream-report-<tenant>.json` | rag_cmd.rs L7954 |
| Merge/prune/sanitize are **skipped** only if nothing completed **and** a completion failed or a cycle error occurred | dream.rs L897–935 |
| Merge tiers: exact name + embedding similarity ≥ `--dedup-threshold` + honorific. Subset and fuzzy are review-only. **`--dedup-threshold` controls only the similarity tier**, so no flag disables every merge | dream.rs L940–965 |
| Prune: `mention_count <= 1` and no neighbours and no chunk evidence and score < `--prune-threshold` (0.3) | dream.rs L974–994 |
| `merge_entity_into` re-keys relations and **combines** their evidence, skips self-loops, appends the alias name and the alias's aliases to `aliases` **only if not already present**, keeps the longer description, and keeps **only the canonical entity's `fields`** (alias fields are dropped) | graph.rs L2059–2262 (self-loops ~L2117) |
| Step 8 `sanitize_relations` runs every cycle: deletes familial relations (`FAMILIAL_RELS`) with a non-Person endpoint and same-gender `spouse_of` pairs, and adds inverses. The DreamReport does not count these deletions | dream.rs ~L1000–1018; graph.rs `FAMILIAL_RELS` ~L162–177, `sanitize_relations` ~L2324 |
| `rag graph score --json` prints `GraphHealthReport` (`overall`, `entity_count`, `relation_count`, `entity_scores[]` with `entity_id`, …); read-only | cli.rs ~L2167–2175; rag_cmd.rs ~L5014–5023; scorer.rs ~L146–180 |
| `rag dream run` flags: `--inference-url`, `--inference-urls`, `--model` (default `default`), `--threshold` 0.6, `--dedup-threshold` 0.92, `--max-completions` 50, `--workers` 4, `--prune-threshold` 0.3, `--no-relations`, `--relation-summary` | `core/crates/kwaai-cli/src/cli.rs` L1784–1830 |

Consequences: relations deleted by the gate reduce entity degree, so the gate can make entities prunable in the next
cycle. Merges rename relations (tracked as rewrites/consolidations, not losses) and drop alias fields (real losses).

## Evaluation
| Fact | Source |
|---|---|
| `rag eval` flags: `--questions`, `--kb`, `--inference-url`, `--model` (llama3.1:8b), `-k/--top-k` (20), `--mode` (iterative), `--semantic-score`, `--output`, … | cli.rs L1645–1712 |
| Scoring is keyword token overlap. Summary row is "Overall recall (token-overlap)", or with `--semantic-score` "Retrieval recall (token-overlap + semantic)" and "Generation recall (…)" | rag_cmd.rs ~L9210–9227 |
| Ordinary `rag eval` computes **no Recall@k or MRR** (those exist only in `rag dream embed-eval`) | rag_cmd.rs eval path L8422–9360 vs embed-eval ~L8070–8205 |
| `rag eval` does not use the query cache (`query_cache.db`) | no cache reference in the eval path; `core/crates/kwaai-rag/src/cache.rs` |
| `rag eval --output` and `eval-progress.json` write only inside the data dir / given path | cli.rs L1698–1706 |
| The D6 set has 40 questions, all with `expected_keywords` | `tests/kwaai-knowledge/d6_eval_questions.json` |
| JSONL dumps (`eval_dump.rs`) and the eval2 harness do **not** exist at 486bcf6 | `git ls-tree 486bcf6` |

## Design decisions
1. **Primary endpoint.** `unsupported_present_over_cycle0` = unsupported gate-eligible facts present ÷ gate-eligible
   facts at cycle 0. The earlier `unsupported_or_lost_over_cycle0` counts a gated fact as lost, so it cannot show a
   gate helping; it is kept as a secondary column. Over-deletion is caught by `lost` and the false-rejection rate.
2. **Observation point.** Rows are written for `pre_gate`, `post_gate` and `post_dream`. The default primary phase is
   `post_dream`: all arms are measured right after the same operation. The trade-off is that at cycle k a
   gate_after arm has been gated k−1 times and a gate_before arm k times. `end_of_cycle` (each protocol's final
   state) is the alternative; it measures gate_after right after its own clean-up. Choose one in the
   preregistration; both are always recorded.
3. **Identity.** Records are keyed by persisted location (entity id + field name; src id + dst id + relation type;
   entity id + sentence hash + occurrence). Text equality is reported (`records_sharing_content`) but never used as
   a key, so same-named entities (for example, the known Rassool collision) are never conflated.
4. **Attribution.** Causes are inferred between adjacent states at every cycle and phase. There is no operation log
   at 486bcf6 (the DreamReport has counts only), so dream-step causes are at best `inferred`. Only this tooling's
   own gate writes, and in-place field value changes, are `observed`.
5. `no_merge` is named `no_similarity_merge`, because exact-name and honorific merges cannot be disabled.
6. **Virtual vs written gates.** A virtual gate is an immediate counterfactual on one state; the arm's dynamics stay
   ungated. Only a written gate is a longitudinal intervention. Writes need a PASS bound to this runtime.
7. Descriptions are outside the primary endpoint and the gate, and are reported in their own block.

## Found in the first real-binary pilot (kwaainet 0.7.2, WSL, 2026-10-21)
Pilot KB: one novel (*The Red Badge of Courage*), 195 entities, 53 stored relations; one dream cycle per arm.
1. **Dangling relations.** 37 of 53 stored relations pointed at an entity that no longer exists (graph.rs
   `delete_entity` leaves relations stale until `prune_dangling_relations`, ~L4480–4530, which the dream cycle
   does not call). They are not claims about any stored entity, so they are excluded from facts and reported
   per row as `dangling_relations` (and `self_loop_relations`). Before this fix they appeared as 37
   `unknown` losses per arm and as phantom losses inside gate steps.
2. **Batch merges lose alias records (upstream behaviour).** `merge_entity_into` builds the canonical's new
   `aliases` from the in-memory node (~L2230–2240) but its in-memory update (~L2265–2272) changes only
   `mention_count` and `description`. When several entities merge into one canonical in a cycle, only the last
   alias name is kept in `aliases`. Merge attribution therefore also uses the exact-name tier (same
   `normalize_name`, canonical's `mention_count` grew) and, when the DreamReport counts more merges than were
   identified, reports the remaining removals as **ambiguous** merges rather than unexplained deletions. Worth
   raising with the KwaaiNet maintainers: it also means alias-based lookup misses merged names.
