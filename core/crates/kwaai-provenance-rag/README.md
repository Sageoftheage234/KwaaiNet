# kwaai-provenance-rag

Observe-only provenance for the `kwaai-rag` dream cycle. After each dream stage it
compares the graph's facts with the previous stage and records every difference as
`kwaai-provenance` lineage, then writes a per-cycle summary with the share of facts
that still trace to a source chunk.

It never changes the dream cycle:

- the hooks (`kwaai_rag::dream::DreamObserver`) receive the graph by shared
  reference, so they cannot write to it;
- any error or panic stops observation and is recorded in that cycle's summary,
  without reaching the cycle;
- the provenance gate runs in Observe mode only.

`tests/observer.rs::observing_does_not_change_the_graph` runs the same operations on
two identical knowledge bases, one observed, and checks that the graphs are identical.

## Turning it on

Off unless both are true: `kwaainet` was built with `--features provenance`, and
`KWAAI_PROVENANCE_DIR` names a directory.

```bash
cd core
cargo build --release -p kwaainet --features provenance

export KWAAI_PROVENANCE_DIR=~/provenance/my-kb     # one directory per KB
for i in 1 2 3 4 5; do
  ./target/release/kwaainet rag dream run --kb my-kb --model <model>
done

cargo run -p kwaai-provenance-rag --bin provenance-report -- "$KWAAI_PROVENANCE_DIR"
cargo run -p kwaai-provenance-rag --bin provenance-report -- "$KWAAI_PROVENANCE_DIR" --csv > traceability.csv
```

Each run prints `Provenance: observing cycle N → <dir> (observe-only)`. If the
directory cannot be used, it prints `Provenance: NOT recording: <reason>` and the
cycle runs normally.

Work on a copy of the KB when collecting study data, so its history is only
observed cycles. Changes made between observed cycles are still caught, and are
reported as drift.

## What is recorded

| File | Contents |
|------|----------|
| `kb.json` | The KB (tenant) the directory belongs to; another KB's directory is refused |
| `lineage.jsonl` | Lineage entries and Observe-mode gate outcomes, appended across cycles |
| `cycle-NNNN.json` | Per cycle: facts, traceable facts and rate, breakdown by origin, what each stage changed, drift, gate counts, lineage digest |

A **fact** is a relation `(src, type, dst)` or a non-empty entity field value, with
its evidence chunks. Descriptions, schema types and embeddings are not facts.

| Graph change | Lineage |
|--------------|---------|
| Fact present when observation starts | `extract` from its chunks, actor `baseline` |
| New fact in a stage | `extract` from its chunks, actor `dream.<stage>` |
| Fact gained chunks | `extract` from the new chunks only |
| Fact moved to the canonical entity by a merge | `claim_rewrite` old → new, actor `dream.merge` |
| Field value changed on the same entity and field | `claim_rewrite` |
| Fact removed | `prune` |
| Change between observed cycles | the same, actor `untracked` (drift) |

A chunk ID that is not in the chunk store, such as the dream cycle's placeholder
`0`, is recorded as `synthetic:chunk:N` and never counts as source text. A fact
with no chunks at all derives from `synthetic:no-evidence`.

**Traceable** means a fact's lineage reaches at least one real chunk. It does not
mean the chunk supports the fact: dream-added relations take every chunk of their
source entity, so they are traceable by association. Support needs a verifier
(Phase 4 of the plan).

## Design and limitations

`projects/kwaai-knowledge/plans/NativeProvenance-plan.md`, Phase 3.

Opt-in native factual support verification and the bounded offline evaluator are
explained in [VERIFICATION.md](VERIFICATION.md). Historical traceability and gate
observations are preserved separately from these semantic assessments.

Experimental Phase 5 policy comparison and post-stage evaluation are documented in
[SHADOW_GATING.md](SHADOW_GATING.md). They are opt-in and never enforce mutations.
