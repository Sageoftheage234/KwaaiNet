# D6 RAG Accuracy Progress

**KB:** D6 memoir — _The Setting in Time and Place_ by Y.S. Rassool  
**Eval:** `tests/d6_eval_questions.json` · keyword hit rate  
**Stack:** Rust KwaaiNet RAG · Ollama · local models

---

## Progress Chart

```
50% ┤
    │                                                        ████ 44.8% ← best (mxbai)
45% ┤                                              ████████████████
    │                                         ████ 44.0% (all-minilm)
40% ┤                                    ████ 43.1%          ████ 41.4% (gemma3:4b)
    │                                                              ████ 40.5% (20b)
35% ┤                    ████ 37.9%
    │               ████ 33.3%
30% ┤          ████
    │     ████ 25.0% ── 31.9% ── 33.6% ← experiments (reverted)
25% ┤████ 24.6%
    │
    └────────────────────────────────────────────────────────────────
     P1    P2   P3  P7..11  exp     mini  fix  gemma  20b   mxbai
```

---

## Milestone Table

| # | Version | Config | Model | Score | Keywords | Notes |
|---|---------|--------|-------|-------|----------|-------|
| 1 | v0.4.44 | paragraph, k=5, chunk=800, nomic 768-dim, 10q | llama3.1:8b | **24.6%** | 14/57 | Baseline |
| 2 | v0.4.44 | + HyDE, k=8 | llama3.1:8b | **33.3%** | 19/57 | +8.7pp |
| 3 | v0.4.45 | min_chunk_len 100→20 (Phase 7) | llama3.1:8b | — | — | Unblocked short intro chunks |
| 4 | v0.4.45 | Phases 7–11 complete, 10q | llama3.1:8b | **37.9%** | ~22/57 | Working baseline pre-experiments |
| — | — | + synthetic doc-name headers | llama3.1:8b | 31.9% | — | Reverted — filename noise hurt retrieval |
| — | — | + document summary chunks | llama3.1:8b | 25.0% | — | Reverted — summaries displaced specific chunks |
| — | — | nomic, chunk_size=2000, k=20, 20q | llama3.1:8b | 33.6% | ~39/116 | Reverted — large chunks hurt all-minilm context |
| 5 | v0.4.48 | **all-minilm 384-dim**, chunk=512, k=30, 20q | llama3.1:8b | **43.1%** | ~50/116 | New embedder, new best |
| 6 | v0.4.49 | + max_context 8192→24000, improved prompt | llama3.1:8b | **44.0%** | 50/116 | Anti-hallucination rules, all 30 chunks visible |
| 7 | v0.4.49 | same config | gemma3:4b | 41.4% | 48/116 | 2.6pp below llama3.1:8b, faster |
| 8 | v0.4.49 | same config | gpt-oss:20b | 40.5% | 47/116 | 3.5pp below llama3.1:8b, 4.3× slower |
| 9 | v0.4.49 | **mxbai-embed-large 1024-dim**, chunk=default, k=30 | llama3.1:8b | 44.8% | 52/116 | q04 dedication 0→4/4, q01 0→2/3 |
| 10 | v0.4.49 | mxbai, k=30, --mode graph (2355 entities, 2820 relations) | llama3.1:8b | 43.1% | 50/116 | Graph alone trails vector; gains on q08/q10/q18/q19, loses q03/q04 |
| 11 | v0.4.49 | mxbai, k=30, **--mode auto** (graph+vector router) | llama3.1:8b | **46.6%** | 54/116 | **New best** — router picks best of both; +2 on q06/q12/q19 |

---

## Per-Question Breakdown (20q eval set)

| ID | Question | llama3.1:8b | gemma3:4b | gpt-oss:20b |
|----|----------|:-----------:|:---------:|:-----------:|
| q01 | Who is the author? | 0/3 | 0/3 | 0/3 |
| q02 | Who are the author's children? | 3/3 | 3/3 | 3/3 |
| q03 | Who are the author's grandchildren? | 6/6 | 6/6 | 6/6 |
| q04 | To whom is the book dedicated? | 0/4 | 0/4 | 0/4 |
| q05 | Who was J.M.H. Gool? | 2/8 | 2/8 | 2/8 |
| q06 | Tell me about Buitencingle. | 3/8 | 3/8 | 3/8 |
| q07 | Who is the author's wife? | 1/3 | 1/3 | 1/3 |
| q08 | Tell me more about the author's wife. | 2/6 | 2/6 | 0/6 |
| q09 | Who was the author's grandfather? | 1/9 | 1/9 | 0/9 |
| q10 | Tell me about Kloof Nek. | 3/7 | 3/7 | 1/7 |
| q11 | What was the TLSA? | 3/6 | 3/6 | 3/6 |
| q12 | Who was Cissie Gool? | 2/6 | 2/6 | 2/6 |
| q13 | What was the All Africa Convention? | 1/6 | 1/6 | 2/6 |
| q14 | Where was District Six? | 2/6 | 2/6 | 3/6 |
| q15 | What were the forced removals? | 3/6 | 3/6 | 4/6 |
| q16 | Gandhi's connection to the Gool family? | 3/7 | 3/7 | 4/7 |
| q17 | What was Hewat Training College? | 4/5 | 4/5 | 5/5 |
| q18 | What was the New Era Fellowship? | 4/6 | 4/6 | 3/6 |
| q19 | What was the Non-European Unity Movement? | 3/6 | 3/6 | 2/6 |
| q20 | Describe the author's cricket career. | 2/5 | 2/5 | 3/5 |
| **Total** | | **50/116 (43.1%)** | **48/116 (41.4%)** | **47/116 (40.5%)** |

> Note: llama3.1:8b row uses v0.4.49 results (50/116 = 43.1%). The official eval reported 44.0% (50/114) — slight denominator variation between runs.

---

## Persistent Hard Questions

| ID | Root cause | Fix path |
|----|------------|----------|
| q01 author | intro.docx written in 3rd person ("Joe Rassool started writing…") — model can't infer authorship | Needs explicit author metadata in chunk header, or a stronger model |
| q04 dedication | Dedication page never uses the word "dedicated" — zero BM25/semantic signal | Add metadata tag `[dedication]` at ingest, or exact-match retrieval path |
| q05/q09 Gool/grandfather | Right chapters retrieved, but model gives hedged partial answers | Query understanding (`--understand`) to decompose multi-part questions |

---

## What Changed at Each Phase

### Phase 7 — min_chunk_len 100 → 20
The author's signature ("Y.S. Rassool, 2000", ~17 chars) and dedication (~110 chars) were below the
100-char minimum and got merged with bibliography/TOC text. Lowering to 20 preserved these as
standalone chunks.

### Phase 11 — HyDE blending
Pure HyDE on factoid questions regressed q02 (67%→0%) because the hypothetical answer attracted
wrong chapters. Blending original query with HyDE embedding (`alpha=0.5`) restores factoid recall
while keeping HyDE benefit for concept questions.

### Embedder switch: nomic-embed-text → all-minilm
- nomic-embed-text: 768-dim, 512-token context, ~274 MB
- all-minilm: 384-dim, **256-token context** (requires chunk_size ≤ 512), 45 MB
- Switching added +6pp on the 20-question eval — likely because all-minilm is optimised for
  sentence-level semantic similarity, matching the short factual questions in the eval set.
- Chunk size must be ≤ 512 to stay within all-minilm's 256-token window.

### max_context_chars 8192 → 24000
With k=30 chunks at ~300 chars each, 8192 chars only showed ~16 of 30 chunks to the model.
Raising to 24000 lets all 30 chunks reach the LLM context window.

### Anti-hallucination prompt (v0.4.49)
Added Rule 3: "ABSOLUTE RULE — never invent, guess, or fabricate names, places, dates, or quotes."
This eliminated the "Natasha Abed" hallucination on q04 but the model now correctly says "sources
do not contain that information" rather than fabricating a dedication.

---

## Model Comparison Summary

| Model | Score | Avg latency | Relative |
|-------|-------|-------------|----------|
| llama3.1:8b | **44.0%** | ~5.8s | Best accuracy |
| gemma3:4b | 41.4% | ~5.9s | −2.6pp, similar speed |
| gpt-oss:20b | 40.5% | ~25.0s | −3.5pp, 4.3× slower |

**Finding:** Larger model ≠ better RAG accuracy. llama3.1:8b leads on this eval. The 20B model
likely over-explains and drifts away from the exact keywords the eval measures. An LLM-as-judge
score (`--llm-judge`) would give a fairer quality comparison.

---

## Next Steps

| Priority | Approach | Expected gain |
|----------|----------|---------------|
| High | `--llm-judge` eval run — get quality score not just keyword proxy | Diagnostic |
| High | Fix q01/q04 with chunk-level metadata (author tag, dedication tag) | +3–6pp |
| Medium | `--understand` flag on eval — decompose multi-part questions | +2–4pp |
| Medium | `mxbai-embed-large` (1024-dim) — state-of-art retrieval model | Unknown |
| Low | Reranker (`--rerank`) pass on top-30 before LLM generation | +1–3pp |
