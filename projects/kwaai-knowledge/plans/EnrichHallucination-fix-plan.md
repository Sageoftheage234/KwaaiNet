# Entity Description Hallucination Fix

## Status: COMPLETE (v0.4.117)

All changes implemented and shipped. See root cause analysis and full change log below.

---

## Root Causes

1. **Alias hint ambiguous.** `(also referred to as: Fatima, Fatima (Timmie), Timmie, ...)` caused
   llama3.1:8b to read parenthetical aliases as separate people.

2. **No post-generation grounding check.** After the LLM returned a description it was validated
   only syntactically and written directly to the database with no fact verification.

3. **entity_to_chunks alias collision.** For "Fatima Gool", the bare alias "Fatima" matched chunk 520
   (about a *different* Fatima — Uncle Aity's twin daughter). The 7 chunks linked to Fatima Gool via
   this alias all contained the wrong person. The LLM read "twins Fatima and Rahima, elder sisters of
   Zeenith" and generated fabricated family relationships.

4. **Co-occurrence inference.** LLM listed co-mentioned names as family members without explicit
   kinship phrasing.

---

## Changes Implemented

### Change 1 — Alias hint wording (`enrich.rs`)
Changed from `(also referred to as: ...)` to an explicit statement:
> "NOTE: the following are all alternative names for this same person — not children, relatives,
> or other people: ..."

### Change 2 — Person prompt CRITICAL clarification
Added after "Do NOT add information absent from the excerpts":
> "CRITICAL: Any aliases listed above are all names for {name} themselves — do NOT treat them
> as names of their children, relatives, or other people."

### Change 3 — Post-generation grounding check (`unsupported_proper_nouns()`)
New function checks all title-cased words in the generated description against the evidence text.
Any proper noun not found in evidence emits `WARN hallucination risk in "...": claims [...] not found
in evidence`. Writes the description anyway (false positives exist) but flags at enrichment time.

### Change 4 — Co-occurrence relationship guard in Person prompt
Family members instruction now requires explicit kinship phrasing attached to `{name}` (e.g.
"{name}'s daughter", "son of {name}") — co-mentioned names are NOT listed as relatives.

### Change 5 — Evidence filtering (pre-LLM)
Before building the evidence text, compute *distinctive aliases* (all aliases except bare first name).
Filter entity_to_chunks to only include chunks containing the canonical full name OR a distinctive
alias. Fetches 4× fetch_limit from the pool before filtering, then caps at fetch_limit.

### Change 6 — Full-corpus fallback
If distinctive-alias filtering removes all evidence chunks (Fatima Gool case: all 7 linked chunks
came from the wrong person's context), fall back to scanning `meta.all_chunks()` for the full
corpus. Finds chunks by canonical name or distinctive alias across all 1152 chunks.

### Change 7 — `rag graph set-description` CLI command
Manual override command for cases where LLM enrichment is fundamentally broken (corrupted
entity_to_chunks links). Bypasses enrichment entirely:
```bash
kwaainet rag graph --kb D6 set-description \
  --entity "Fatima Gool" \
  --description "Fatima Gool, affectionately known as Auntie Timmie..."
```
Calls `GraphStore::set_description()` directly.

### Change 8 — `rag chat --local` flag
`kwaainet rag chat --local` routes to `http://localhost:11434` (local Ollama) instead of the
remote p2p inference pool.

---

## Fatima Gool — Manual Fix Applied

The LLM-based enrichment could not fix Fatima Gool because even after the corpus fallback fired
and found the correct "Timmie" chunks, the model still generated wrong family relationships.
(Root cause: the corpus fallback order included chunks adjacent to "Timmie" mentions that
also referenced the OTHER Fatima's family context.)

**Fix applied**: `rag graph set-description` with description grounded in evidence:
> "Fatima Gool, affectionately known as Auntie Timmie, was the author's aunt and a member of the
> Gool family in Cape Town. Manilal Gandhi, son of Mahatma Gandhi, proposed to marry her, but the
> match was opposed on religious grounds (a Hindu marrying a Muslim). She never married, but
> remained close friends with Manilal Gandhi and his wife. In 1930 she wrote an article suggesting
> she was entertaining notions of becoming a journalist."

**Evidence chunks**: 150 (Gandhi-Timmie context), 151 ("my Aunt Timmie never left her single
status"), 171 ("an article she wrote in 1930").

---

## Test Results (v0.4.117)

- Fatima Gool description: FIXED (manually via `set-description`)
- Grounding warnings: firing correctly for unsupported proper nouns in 4 other entities
- Eval: running (11/40 at 87.0% recall — see eval log)
- `rag chat --local`: verified working

---

## Scope

`core/crates/kwaai-rag/src/enrich.rs`, `core/crates/kwaai-cli/src/cli.rs`,
`core/crates/kwaai-cli/src/rag_cmd.rs`.
