# D6 Person-Entity Experiments Log

Plan: [`projects/kwaai-knowledge/d6-person-entity-experiments.md`](../../projects/kwaai-knowledge/d6-person-entity-experiments.md)

---

| Snapshot | Date | Exp | Chunks | Raw entities | After dedup | Eval recall | Judge | Notes |
|----------|------|-----|--------|-------------|-------------|-------------|-------|-------|
| D6_person_1pct_tier1_20260602 | 2026-06-02 | 1 | 12 (1%) | 34 | 33 | — | — | sanity check only |
| D6_person_10pct_dedup_v1_20260602 | 2026-06-02 | 2 | 114 (10%) | 288 | 262 | — | — | Tier 2 NOT merged — FP rate 69%; see analysis |
| D6_confidence_hybrid_10pct_v1_20260603 | 2026-06-03 | confidence hybrid Stage 2 | 114 (10%) | 223 CC | 242 CC+EC | — | — | 11 improved, 19 new discovered |

---

## 2026-06-02 – D6_person_1pct_tier1_20260602

- **Experiment:** 1 (1% sample sanity check)
- **Build:** 1% of corpus (12 chunks), Person entities only, no relations, llama3.1:8b, 4 workers, localhost:11434
- **Dedup:** Tier 1 + Tier 3 + Tier 4 only (--auto --auto-threshold 1.01; Tier 2 cosine disabled)
- **Stats:** raw=34, after-dedup=33 (1 Tier 1 merge: "Mr. Edross" → "Mr Edross")
- **Sanitize:** 1 gender inferred, 0 relations affected
- **Eval:** N/A — sanity check only

### Dedup detail
- Tier 1: 1 merge (punctuation normalisation — `Mr. Edross` → `Mr Edross`)
- Tier 2: 3 candidates above 0.85 threshold, all skipped (auto-threshold 1.01)
- Tier 3: 0 structural duplicates
- Tier 4: 0 neighbour-containment candidates

### Entity spot-check (8 of 33)

| Entity | Mentions | Description | Assessment |
|--------|----------|-------------|------------|
| Yousuf (Joe) Rassool | 1 | occupation: author | ✅ plausible |
| Joe Rassool | 4 | occupation: author | ✅ plausible — same person, not yet merged |
| Hassen Abrahams | 1 | (empty) | ✅ name looks real |
| Hassen | 3 | affiliation: team | ✅ plausible — likely same as Hassen Abrahams |
| Mr Edross | 2 | occupation: teacher | ✅ merged correctly from Mr. Edross |
| Hamid Khan | 1 | (empty) | ✅ plausible name |
| Hamid | 1 | (empty) | ✅ plausible — likely fragment of a longer name |
| P.V. Tobias | 4 | occupation: academic | ✅ correct (Philip Tobias, anthropologist) |

### Observations

**Strengths:**
- 100% Person type assignment — no junk entity types (Place, Organization, Unknown)
- Tier 1 correctly merged `Mr. Edross` / `Mr Edross` (punctuation variant)
- No obvious garbage extractions (e.g. no "He", "The Author", multi-entity blobs)
- `P.V. Tobias` correctly identified as academic (Philip Tobias, University of the Witwatersrand)
- NER phrase-merge bug from M40 does NOT appear — names look clean

**Known fragmentation (expected at 1%, no Tier 2):**
- `Joe Rassool` and `Yousuf (Joe) Rassool` are the same person — not merged yet
  (Tier 2 would handle this; they exist in separate chunks so no shared tokens for Tier 1/3)
- `Hassen` and `Hassen Abrahams` are likely the same — not merged (no shared neighbours at 1%)
- `Hamid` and `Hamid Khan` are likely the same — not merged (same reason)

**The 3 skipped Tier 2 candidates** are the interesting ones for Experiment 2 — when JW gate
is active and Tier 2 is un-blocked, we'll be able to see whether they are correct merges.

**Verdict:** ✅ Sanity check passes. Extraction is producing clean Person names from D6.
Ready to proceed to Experiment 2 (needs JW gate build + 10% run).

---

## 2026-06-02 – D6_person_10pct_dedup_v1_20260602

- **Experiment:** 2 (10% sample + JW gate calibration)
- **Build:** 10% of corpus (114 chunks), Person only, no relations, llama3.1:8b, 8 workers
- **Inference URLs:** `http://localhost:11434` + `mux://12D3KooWCzuhpXrZXD8aezgm4JCkCZSTgj48uDywYYdTzUhF8SHs` (metro-linux)
- **Build time:** ~702s (~11.7 min). **Timeouts:** ~27/114 chunks (24%) silently skipped
- **Dedup:** Tier 1+3+4 auto (--auto --auto-threshold 1.01); Tier 2 NOT auto-merged
- **Stats:** raw=288 → after Tier 1+3+sanitize=262 (26 merged/pruned)
- **Eval:** N/A — threshold calibration step

### Dedup detail

| Tier | Merges | Notes |
|------|--------|-------|
| Tier 1 (exact normalized) | 11 | Punctuation variants: Mr./Mr, Prof./Prof, W.P./W.P, BM.Kies/BM Kies, etc. |
| Tier 2 (cosine ≥ 0.85) | 0 auto-merged; 54 candidates found | All held back for analysis (see below) |
| Tier 3 (structural) | 13 | Honorific strips + 2 fuzzy: Yusuf→Yousuf, J.M.H.→J.H.M (good); James→Professor James |
| Sanitize (stub prune) | 2 | Removed bare "MS" and "Rev" as honorific stubs |

### Tier 2 candidate analysis — 54 pairs at threshold 0.85

**False-positive rate: 37/54 = 69%** — far above the 5–10% target. JW gate (0.60) blocked only 1.

| Category | Count | Verdict | JW blocks? |
|----------|-------|---------|------------|
| Fragment/alias of same person | 14 | ✅ Should merge | No — JW 0.80–0.87 |
| Same-surname, different people | 16 | ❌ False positive | No — JW 0.71–0.79 |
| Same-title, different people | 13 | ❌ False positive | No — JW 0.65–0.83 |
| Same-first-name, different people | 6 | ❌ False positive | No — JW 0.71–0.75 |
| Cross-Gool-family pairs | 5 | ❌ False positive | Barely — JW 0.60–0.67 |
| JW gate correctly blocks | 1 | ✅ Gate works | Yes — Ciraj/Yousuf Rassool (JW≈0.55) |

**Key false positive examples (should NOT merge):**
- General Zhukov/Paulus/MacArthur/Hertzog/Nguyen Giap — 10 pairs, all sim=0.960; all different WW2/Cold War generals grouped by shared "General" token + similar context
- Lord Headley ↔ Lord Roberts — different British lords
- Ruth/Ciraj/Rebecca Rassool cross-pairs — family members with same surname, different people
- Hamid Khan ↔ Abdul Hamid — CRITICAL different people sharing "Hamid" token (sim=0.960)
- Peter Abrahams ↔ Hassen Abrahams — different people, same surname
- AH Gool ↔ Joseph Gool / Yusuf Gool — sim=1.000 with empty descriptions; meaningless similarity

**Key true positive examples (should merge):**
- Mr. Edross ↔ Solly Edross (sim=0.946) — correct; same teacher, formal/informal name
- Abdurahman ↔ Dr. Abdullah Abdurahman (sim=0.957) — correct alias
- Joe Rassool ↔ Y.S. Rassool (sim=0.862) — correct; same author
- BM.Kies / Kies ↔ Ben Kies M.A — correct; same person

### Root cause of high false-positive rate

1. **sim=1.000 pairs (empty description):** When two entities have no description, their embeddings are identical (just the name vector). This produces spurious sim=1.000 even for completely different names like "AH Gool" / "Joseph Gool". These should be excluded from Tier 2 entirely.

2. **Title-prefix dominance:** "General X" / "General Y" pairs share the "General" token (≥3 chars, not in stop list), then have high embedding similarity because context is always "military commander." The existing blob/surname guards don't cover this.

3. **Same-surname community:** In a District Six memoir, dozens of unrelated people share surnames (Rassool, Gool, Abrahams). The surname token alone is enough to create a false shared-token bucket.

### Recommendations before Experiment 3

Two targeted fixes, in priority order:

**Fix 1 — Exclude empty-description pairs from Tier 2 (quick win):**
In `find_dedup_candidates()`, add a guard:
```rust
if na.description.len() < 20 && nb.description.len() < 20 {
    continue; // neither has a real description; embedding similarity is meaningless
}
```
This alone eliminates the sim=1.000 pairs (#1, #2, and several others).

**Fix 2 — Title-prefix guard:**
Add "general" (and other military/political titles) to `DEDUP_STOP`, OR add a guard:
if the ONLY shared significant token is a title-word (general, lord, colonel, president,
prime, minister, advocate, reverend, captain), cap sim at 0.94 (same as blob guard).
Eliminates 10+ false positives from the Generals cluster.

**JW gate assessment:** Threshold 0.60 is too low to be meaningful. After Fix 1 and Fix 2,
re-evaluate whether JW at 0.75+ would add net value without blocking true positives.

### Timeout analysis — known robustness gap

~27/114 chunks (24%) timed out and were silently skipped. From `access-delta-inference.md`,
the fix is retry-with-backoff at `graph.rs:3033–3055`. This gap will matter more at 100%.
For 10% sanity purposes, the skipped chunks mean the entity set is incomplete but usable.

### Verdict

❌ **Tier 2 dedup NOT safe to auto-merge at 0.85 threshold** for Person-only D6 graph.
Two code fixes needed (empty-description guard + title-prefix guard) before Experiment 3.
JW gate threshold should be revisited after those fixes.

---

## 2026-06-03 – D6_confidence_hybrid_10pct_v1

- **Plan:** `projects/kwaai-knowledge/plans/confidence-hybrid-extraction.md`
- **Build:** 10% corpus (114 chunks), Person-only, no relations, llama3.1:8b, 8 workers
- **Inference:** localhost:11434 + mux://metro-linux (round-robin)
- **GLiNER:** http://127.0.0.1:9099
- **Build time:** ~708s CC phase, then EC refinement pass
- **Timeouts:** ~25 chunks (~22%) — known retry gap

### CC phase results
- Raw entities after CC: **223**
- Avg confidence post-scoring: **33%** (type pillar 100%, no fields, no relations)
- All entities below threshold 0.45 (expected for a fresh graph with no dream cycles)

### EC refinement results (threshold=0.45, budget=50)
- Entities escalated: **50** (lowest-confidence first)
- Existing entities improved confidence: **11/50** (avg ↑ +0.03)
- **New entities discovered via EC context: +19** (223 → 242 total)
- Final entity count: **242**
- Entities with any fields filled: **100/242 = 41%**

### Quality metrics
- Avg confidence: **35.0%** (baseline ~33%, slight improvement from 100 entities with fields)
- **FT recall: 96/147 = 65.3%** (vs 13.6% at 1% scale — dramatic recall gain from corpus coverage)
- Top confidence: 43% (Earnest Livingston MQotsi, Omar Abrahams — have occupation fields)

### Key observations

**EC refinement value at 10%:**
- +19 new entity discoveries is the biggest contribution (bonus recall)
- 11 entities that only appeared in single CC chunks now have field data from aggregated context
- The +0.03 avg confidence improvement is modest — fields only improve from 0% to 10% (one field added)
- Most entities still lack fields because they only appear briefly in D6 text with no biographical detail

**Confidence score behaviour:**
- All fresh entities score 33% (type:100%, summary:0%, relation:0%)
- After EC, 100 entities have ≥1 field → summary pillar >0% → confidence >33%
- A threshold of 0.45 captures all entities needing enrichment at this graph health level
- Dream cycles would push confidence much higher by filling all the summary pillars

**What's needed for higher confidence:**
- Dream cycles (adds descriptions, fills missing fields) — pushes summary pillar toward 100%
- Family tree seed (adds relations) — pushes relation pillar from 0%
- Combined: expected confidence >0.65 for well-evidenced entities

**Comparison vs pure CC baseline (Experiment 2):**
| Metric | CC only | CC + EC hybrid | Δ |
|--------|---------|----------------|---|
| Entities | 223 | 242 | +19 (+8.5%) |
| FT recall | ~13.6% | 65.3% | — (corpus diff, not hybrid diff) |
| Entities with fields | 0 | 100 (41%) | +100 |
| Build time | ~708s | ~708s + EC pass | ~+10min |

Note: FT recall jump is from 10% corpus size, not EC itself. The EC contribution is the +19 entities
and the field enrichment of 11 existing entities.

### Next steps
- Run dedup (Tier 1+3) to clean the 242 entities
- Add family tree seed for relation pillar coverage
- Run dream cycles to improve summary scores toward 0.65+ health
- Re-evaluate threshold: consider 0.40 to capture only the truly empty entities in larger graphs
