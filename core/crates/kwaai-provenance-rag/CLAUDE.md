# kwaai-provenance-rag crate

Observe-only adapter: implements `kwaai_rag::dream::DreamObserver` with
`kwaai-provenance`, recording dream-cycle graph changes as lineage and writing
per-cycle traceability summaries. Used by `kwaainet` only with
`--features provenance` and `KWAAI_PROVENANCE_DIR` set.

**Full project context:** `projects/kwaai-knowledge/` — CLAUDE.md, GLOSSARY.md, and
`plans/NativeProvenance-plan.md` (Phase 3: requirements, decisions, limitations).

## Key source files

| File | Description |
|------|-------------|
| `src/facts.rs` | What a fact is (`FactKey`, `Fact`), `snapshot` of the graph, chunk artifact IDs |
| `src/plan.rs` | Pure diff of two snapshots into lineage steps: extract, evidence added, rewrite, remove |
| `src/observer.rs` | `ProvenanceObserver`: baseline, drift reconciliation, stage recording, gate, `CycleSummary` |
| `src/lib.rs` | `observer_from_env` |
| `src/bin/provenance-report.rs` | Replays the log, checks the digest, prints a table or CSV per cycle |
| `tests/observer.rs` | Real temporary KBs: graph unchanged by observing, traceability, drift, refusals, report tool |

## Build

```bash
cargo test -p kwaai-provenance-rag
cargo clippy -p kwaai-provenance-rag --all-targets -- -D warnings
```

## Do not

- Do not take `&mut GraphStore` or `&mut MetaStore` in a hook, or call anything that
  writes. Observation must not be able to change the cycle.
- Do not let an error or panic leave a hook. Use `ProvenanceObserver::guarded`.
- Do not run the gate in Enforce mode here. Enforcement is Phase 5 and needs its own PR.
- Do not count a chunk that is not in the chunk store as original (`facts::chunk_artifact`).
- Do not share a provenance directory between KBs; `kb.json` enforces this.
- Do not commit provenance directories produced from a private KB (D6).
