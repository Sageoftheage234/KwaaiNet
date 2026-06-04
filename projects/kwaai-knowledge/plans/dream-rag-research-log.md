# Dream RAG — Research Experiment Log

**Paper title (working):** Dream RAG: Iterative Knowledge Graph Enrichment for Long-Form Document Retrieval  
**Dataset:** D6 — *The Setting in Time and Place* by Y.S. Rassool (District Six memoir, ~200 pages, 1136 chunks)  
**Eval set:** `tests/kwaai-knowledge/d6_eval_questions.json` — 40 questions, keyword hit rate + LLM-as-judge  
**System:** KwaaiNet RAG · Rust · llama3.1:8b · mxbai-embed-large / nomic-embed-text

---

## Research Narrative

This log captures the experimental journey that led to the Dream RAG architecture. The story is one of progressive problem discovery: each improvement revealed the next bottleneck, and the system evolved from pure dense retrieval toward a self-improving knowledge graph with iterative enrichment.

---

## Phase 1 — Dense Retrieval Baseline (v0.4.44–v0.4.48)

### Problem
Standard RAG over a 200-page memoir achieves only 24.6% keyword recall on factual questions.
Dense retrieval has a structural blind spot: person-entity questions (who was X?, who was Y's wife?)
require cross-chunk identity resolution that cosine similarity cannot provide.

### Experiments

| Config | Keyword | Notes |
|--------|---------|-------|
| paragraph chunks, nomic 768-dim, k=5 | 24.6% | Baseline |
| + HyDE, k=8 | 33.3% | +8.7pp |
| min_chunk_len 100→20 | 37.9% | Unblocked dedication/signature chunks |
| chunk=512, all-minilm 384-dim | 43.1% | |
| chunk=512, mxbai-embed-large 1024-dim | 44.8% | Best embedder for this corpus |
| + context window 8192→24000 | 46.6% | All k=30 chunks now fit LLM context |

### Key finding
**Chunk size and embedder quality matter more than retrieval parameters.** A single-line
fix (lowering `min_chunk_len`) recaptured 13pp by unblocking physically short but critical
text (author name, book dedication). The larger embedding space (1024-dim mxbai) consistently
outperformed 384-dim all-minilm.

---

## Phase 2 — Knowledge Graph Layer (v0.4.49–v0.4.56)

### Problem
Dense retrieval cannot resolve entity identity. "J.M.H. Gool", "JMH Gool", and "Joosub Gool"
were three disconnected nodes. Queries for any form returned the wrong chunks.

### Architecture added
- Per-chunk LLM entity extraction (Person, Place, Organization, Event, Concept, …)
- Deterministic entity IDs: `sha256(name.lower() + "::" + type)[..8]`
- Per-entity BFS retrieval: graph mode merges entity neighbourhood chunks with dense results
- Hybrid routing: `auto` mode detects entity-heavy queries and blends graph + vector

### Key experiments

| Config | Keyword | Notes |
|--------|---------|-------|
| graph-only mode | 43.1% | Below hybrid — graph alone adds noise |
| auto hybrid | 46.6% | Router picks right mode per query |
| + family tree YAML seed | 50.0% | **First unambiguous graph win** |

**Family tree seed (YAML):** Authoritative canonical names + alias merges + 46 family relations
for 24 key persons. Consolidated "JMH Gool" / "Joosub Gool" / "J.M.H. Gool" → single node.
Single biggest single-step improvement to date.

### Iterative retrieval (v0.4.53)
Multi-round gap-filling: after Round 1, check coverage; if < 70%, do a graph gap-fill pass;
if still < 75%, LLM reformulates a sub-query. **Judge score 1.65 → 1.80** (+0.15).

---

## Phase 3 — Graph Deduplication (v0.4.55–v0.4.56)

### Problem
The raw graph contained hundreds of duplicate entity nodes: "Hassen Mall" / "Hassen",
"Mitchell s Plain" / "Mitchell's Plain", OCR artifacts like "S  Jayiya" / "S. Jayiya".
These created noise in entity retrieval and split evidence across disconnected nodes.

### Dedup system (4 tiers)
1. **Tier 1** — exact normalised names (alphanumeric + lowercase) → always auto-merge
2. **Tier 2** — embedding cosine similarity ≥ threshold with shared significant name token
3. **Tier 3** — structural: honorific-stripped match, subset-name + shared neighbours, edit-dist ≤ 2
4. **Tier 4** — role/pronoun neighbour containment ("Grandpa" → known Person)

Additions during person-only experiments:
- **Jaro-Winkler gate** for Tier 2: skip pairs whose name JW similarity < 0.60
- **Type-match guard**: cross-type pairs never merge
- **Disambiguation filter**: parenthetical suffixes or Roman numeral ordinals block merge
- **Qualification suffix stripping** (Tier 3D): "Ben Kies M.A" → "Ben Kies"

### Result
k=20, auto mode, cleaned graph: **51.7%** (new keyword best). Iterative k=20: **56.9% / 1.80/2**
(new best on both metrics simultaneously).

---

## Phase 4 — Dream RAG: Iterative Graph Enrichment (v0.4.72+)

### Core idea
A single extraction pass sees each chunk once. For a 200-page memoir, most Person entities
appear in 2–5 different chapters with different biographical details scattered across them.
**Dream RAG** is an autonomous cycle that re-visits incomplete entities using their source
chunks to fill missing fields and enrich descriptions — analogous to memory consolidation.

### Architecture
1. **Health scorer**: 3-pillar per-entity score (type completeness, description quality, relation richness)
2. **Dream cycle**: score → collect below-threshold entities with source text → fan-out LLM
   completion calls → write back richer descriptions and structured fields → re-score
3. Cycle runs until plateau or budget exhausted

### Dream cycle progression on D6 (M22, 31 cycles)

| Metric | Before | After 31 cycles |
|--------|--------|-----------------|
| Graph health | 51.5% | 78.1% |
| Unknown-type entities | 88 | 5 |
| Type distribution | scattered | Person 521, Place 188, Org 180 |
| Keyword recall | ~50% | **58.6%** (new best) |

**2% per cycle** improvement for first 24 cycles, plateau at cycle 25 (all completable entities
exhausted). Ghost entities (names in text but no chunk links) were confirmed useful: removing
them severed 2972 graph relations and caused a 7pp regression. Ghost entities preserve traversal
paths even without direct evidence.

---

## Phase 5 — NER Pre-screening (v0.4.79)

### Problem
The LLM extraction was called on every chunk regardless of whether it contained named entities.
Boilerplate chunks (chapter headers, table of contents) wasted inference budget. Also, the
regex-based candidate detector missed names that didn't start with capital letters (OCR errors,
mid-sentence references).

### GLiNER integration
- `scripts/gliner_server.py` — FastAPI server wrapping `urchade/gliner_small-v2.1` (~5ms/chunk)
- GLiNER detects Person spans and feeds them as validated hints into the extraction prompt
- **Gating upgrade**: candidates = regex ∪ GLiNER spans; LLM skipped only when BOTH find nothing
- **Result on 1% corpus**: 34 → 40 entities from same 12 chunks (+18% recall from better gating)

---

## Phase 6 — Confidence-Scored Hybrid Extraction (2026-06-03)

### Research question
Can we use CC (chunk-centric extraction) as a fast first pass, score each entity's completeness,
and escalate only low-confidence entities to EC (entity-centric extraction with aggregated context)?

### Confidence signal
Reuses existing `score_entity()` from `scorer.rs`: `confidence = (type_score + summary_score + relation_score) / 3`.
No LLM prompt changes needed — structural completeness IS the confidence proxy.

### Entity-centric extraction
For each low-confidence entity:
1. Collect all chunk IDs from `entity.evidence`
2. Take top-3 source chunks → expand each to ±1 context window → deduplicate → concatenate
3. Call `extract_from_text()` with entity name as sole candidate + GLiNER hint
4. Merge improved record back via `upsert_entity()`

### Stage 1 (1% corpus, 12 chunks)

| Mode | Calls | Time | Entities | FT recall |
|------|------:|-----:|--------:|----------:|
| CC only | 12 | 186s | 40 | 13.6% |
| CC + EC (threshold=0.45, budget=10) | 12 + 10 | ~200s | 46 | — |

EC added +6 new entities. At 1% scale, aggregation doesn't add new context (same chunks).

### Stage 2 (10% corpus, 114 chunks)

| Mode | Calls | Time | Entities | Entities with fields |
|------|------:|-----:|--------:|--------------------:|
| CC only | 114 | 708s | 223 | 0 (0%) |
| CC + EC (threshold=0.45, budget=50) | 114 + 50 | 708s + EC | 242 | 100 (41%) |

**+19 new entity discoveries. +100 entities gained ≥1 biographical field.**

---

## Phase 7 — Mini Dream Loop on 10% Graph (2026-06-03/04)

The 10% graph was used as a controlled microcosm to validate the full pipeline before scaling.

### Step sequence

| Step | Before | After | Delta |
|------|--------|-------|-------|
| CC + EC build | — | 242 entities, 0% fields | — |
| Dedup (Tier 1+3+4) | 242 | 224 entities | -18 clean merges |
| Sanitize + reembed | 224 | 223 entities (1 stub pruned) | |
| Family tree seed | 223 entities, 0 rel | 219 entities, 118 relations | 69 relations planted |
| Dream cycle 1 (150 completions) | 38.5% health | 41.5% health (+2.9%) | 120 summaries, 1 merge |
| EC refine-only (threshold=0.34, budget=31) | 218 entities | 241 entities | +6 improved, **+23 new** |

### Confidence distribution after dream (pre-EC at 0.34)
- 31 entities at 30–34% (dream had no source text — empty)
- 32 entities at 35–39% (thin description)
- 133 entities at 40–44% (description + some fields, but no relations)
- 18 entities at 65–69% (seeded family tree members with relations)
- 1 entity at 70–74% (best-evidenced entity)

**Key finding:** A 0.45 threshold on a post-dream graph captures almost everyone (196/217).
The surgical threshold after dream is 0.34 — which targets only the 31 completely empty entities
that dream couldn't reach. This is the regime where EC adds most value: it provides multi-chunk
context for entities that appeared in scattered mentions the LLM couldn't enrich in isolation.

---

## Comparison Table: All Major Milestones

| ID | Phase | Config | Keyword | Judge | Notes |
|----|-------|--------|---------|-------|-------|
| M1 | Baseline | paragraph, nomic, k=5 | 24.6% | — | |
| M4 | Dense | min_chunk_len=20, k=10 | 37.9% | — | |
| M9 | Dense | mxbai, k=30 | 44.8% | — | |
| M12 | Graph | + family tree seed, auto k=20 | 50.0% | 1.85/2 (lenient) | |
| M14 | Iterative | mode=iterative, k=10 | 49.1% | 1.80/2 | |
| M17 | Dedup+iter | k=20, dedup+reembed | 56.9% | 1.80/2 | Both metrics best |
| M21 | Merge fix | chunk-transfer fix | 54.3% | 1.85/2 | Judge best (strict) |
| M22 | Dream | 31 dream cycles | 58.6% | — | Keyword best |
| M25 | NER | GLiNER pre-screener rebuild | 55.2% | — | |
| M35 | Restore | M30 graph, Ollama | 59.5% | — | Single-run best ever |
| — | Person-only | 10% CC+EC+dedup+seed+dream | pending | pending | Mini-loop complete |

---

## Novel Contributions (for paper)

1. **Dream RAG loop**: autonomous iterative enrichment of knowledge graph entities using
   scored completeness as the stopping criterion. Analogous to memory consolidation — the
   system "processes" entity knowledge overnight to fill gaps left by single-pass extraction.

2. **Confidence-scored hybrid extraction** (CC → EC): chunk-centric extraction as a fast
   first pass, followed by entity-centric refinement targeted at low-confidence entities only.
   Reduces EC overhead to the entities that actually need it (typically 15–30% of the graph).

3. **Structural confidence as routing signal**: `score_entity()` (type + summary + relation
   completeness) serves as a zero-cost proxy for extraction quality without any LLM calls.
   The threshold adapts naturally as dream cycles improve the graph: a 0.45 threshold on a
   fresh graph targets everything; a 0.34 threshold on a post-dream graph targets only the
   truly unreachable entities.

4. **Multi-tier dedup for community corpora**: the D6 memoir is a pathological dedup case —
   dozens of people share surnames (Gool, Rassool, Abrahams). Standard embedding similarity
   dedup fails at 69% false-positive rate. The 4-tier system with JW gate, type-match guard,
   and qualification stripping brings this to manageable levels.

5. **Ghost entity preservation**: entities with no direct source evidence but with graph
   relations to evidenced entities proved crucial. Removing 361 "ghost" entities severed 2972
   relations and caused a 7pp regression. For graph-based retrieval, structural connectivity
   is as important as direct evidence linkage.

---

## Open Questions (for paper discussion)

- Does the dream cycle plateau at ~78% health because of corpus limitations (many entities are
  mentioned only once, with no biographical text) or because of model limitations (8B model
  cannot infer biographical facts)?
- Can EC refinement substitute for dream cycles, or are they complementary (EC adds new entities;
  dream enriches existing ones)?
- How does the approach generalise beyond memoir corpora to academic papers, legal documents,
  or news archives where entity-density and biographical richness differ significantly?
- What is the optimal threshold trajectory as dream cycles progress? Does a dynamic threshold
  (e.g., mean confidence − 1σ) outperform a fixed one?

---

## Eval Results Log

| Date | Graph state | Q | Recall | Judge | Notes |
|------|------------|---|--------|-------|-------|
| 2026-05-29 | M30 (NER+31-dream cycles) | 20 | 52.6% | 1.80/2 | Baseline for this phase |
| 2026-06-02 | + family tree seed | 20 | 54.3% | 1.60/2 | Seed improves recall, hurts judge |
| 2026-06-02 | + 9 curator descriptions | 40 | 56.9% | 1.73/2 | Expanded to full 40q |
| 2026-06-02 | + LLM relation extraction | 40 | 56.4% | 1.65/2 | **REGRESSION** — LLM relations too noisy |
| 2026-06-03 | Round 3 (entity-only rebuild) | 40 | 54.3% | 1.65/2 | Entity-only confirmed better |
| 2026-06-03 | + 1 dream cycle + alias-scan | 40 | 55.2% | 1.80/2 | Matched baseline judge |
| 2026-06-04 | 10% mini-loop (CC+EC+dedup+seed+dream+EC) | 40 | _pending_ | _pending_ | |
