#!/usr/bin/env python3
"""
rag_chunk_test.py — RAG chunking strategy benchmark for a knowledge base.

Usage:
    python3 scripts/rag_chunk_test.py <source_dir> [--kb <name>] [--queries-file <path>]

The script:
  1. Runs the query set against the current KB as a baseline (no re-ingest).
  2. For each chunking strategy: destroys the KB, re-inits, syncs the source dir,
     runs the full query set, and records metrics.
  3. Prints a side-by-side comparison table and writes results to
     /tmp/rag_chunk_results_<kb>.json.

Chunking strategies tested:
  baseline   800 / 200 / 100   (current default — no re-ingest)
  small      400 / 100 /  50   (better precision for rare proper nouns)
  large     1500 / 400 / 150   (better thematic context)
  tiny       200 /  50 /  20   (very fine-grained; paragraph-level)
  wide-lap   800 / 400 / 100   (high overlap; more context continuity)
  xlarge    2500 / 500 / 200   (chapter-level context windows)

Metrics per query:
  top1_score   score of the best retrieved chunk
  top1_doc     document the best chunk came from
  weak         1 if top1_score < 0.020 (floor/noise territory)
"""

import argparse
import json
import os
import subprocess
import sys
import time
from pathlib import Path
from typing import NamedTuple

KWAAINET = os.path.expanduser("~/.cargo/bin/kwaainet")
ENV = {**os.environ, "RUST_LOG": "warn"}

QUERIES = [
    "Buitencingle",
    "TLSA",
    "Tabata",
    "Gool",
    "NEUM",
    "Unity Movement",
    "District Six",
    "Riebeeck",
    "Waheida",
    "Wilson",
]

STRATEGIES = [
    # name          chunk_size  overlap  min_len  re_ingest
    ("baseline",       800,      200,     100,    False),
    ("small",          400,      100,      50,    True),
    ("large",         1500,      400,     150,    True),
    ("tiny",           200,       50,      20,    True),
    ("wide-lap",       800,      400,     100,    True),
    ("xlarge",        2500,      500,     200,    True),
]

WEAK_THRESHOLD = 0.020


class QueryResult(NamedTuple):
    query: str
    top1_score: float
    top1_doc: str
    all_scores: list[float]

    @property
    def is_weak(self) -> bool:
        return self.top1_score < WEAK_THRESHOLD


def run_queries(kb: str) -> list[QueryResult]:
    results = []
    for q in QUERIES:
        try:
            raw = subprocess.check_output(
                [KWAAINET, "rag", "query", q, "--kb", kb, "--json"],
                env=ENV,
                stderr=subprocess.DEVNULL,
                timeout=60,
            )
            hits = json.loads(raw)
        except (subprocess.CalledProcessError, json.JSONDecodeError, subprocess.TimeoutExpired) as e:
            print(f"  ⚠️  query '{q}' failed: {e}", file=sys.stderr)
            hits = []

        top = hits[0] if hits else {}
        results.append(QueryResult(
            query=q,
            top1_score=round(top.get("score", 0.0), 6),
            top1_doc=top.get("doc", ""),
            all_scores=[round(h["score"], 6) for h in hits],
        ))
    return results


def reingest(source_dir: str, kb: str, chunk_size: int, overlap: int, min_len: int) -> None:
    # Destroy existing KB
    subprocess.run(
        [KWAAINET, "rag", "destroy", "--kb", kb, "--yes"],
        env=ENV, stderr=subprocess.DEVNULL, stdout=subprocess.DEVNULL, check=True,
    )
    # Re-init
    subprocess.run(
        [KWAAINET, "rag", "init", "--name", kb],
        env=ENV, stderr=subprocess.DEVNULL, stdout=subprocess.DEVNULL, check=True,
    )
    # Sync with new chunk config
    subprocess.run(
        [
            KWAAINET, "rag", "sync", source_dir,
            "--kb", kb,
            "--chunk-size", str(chunk_size),
            "--chunk-overlap", str(overlap),
            "--min-chunk-len", str(min_len),
        ],
        env=ENV, check=True,
    )


def metrics(results: list[QueryResult]) -> dict:
    scores = [r.top1_score for r in results]
    weak = sum(1 for r in results if r.is_weak)
    return {
        "mean_top1": round(sum(scores) / len(scores), 6),
        "min_top1": round(min(scores), 6),
        "max_top1": round(max(scores), 6),
        "weak_count": weak,
    }


def short_doc(doc: str) -> str:
    return doc.replace(".docx", "").replace("Part_", "")[-38:]


def print_strategy_table(all_data: list[dict]) -> None:
    strategies = [d["strategy"] for d in all_data]
    col_w = max(14, *(len(s) for s in strategies)) + 2

    # Header
    print(f"\n{'Query':<22}", end="")
    for d in all_data:
        label = f"{d['strategy']} ({d['config']['chunk_size']}/{d['config']['chunk_overlap']})"
        print(f"  {label:>{col_w}}", end="")
    print()
    print("-" * (22 + (col_w + 2) * len(all_data)))

    # Per-query rows
    for i, q in enumerate(QUERIES):
        print(f"{q:<22}", end="")
        for d in all_data:
            r = d["results"][i]
            flag = "⚠" if r["top1_score"] < WEAK_THRESHOLD else " "
            cell = f"{flag}{r['top1_score']:.4f}"
            print(f"  {cell:>{col_w}}", end="")
        print()

    # Summary rows
    print("-" * (22 + (col_w + 2) * len(all_data)))
    for label, key in [("Mean top-1", "mean_top1"), ("Min top-1", "min_top1"), ("Weak queries", "weak_count")]:
        print(f"{label:<22}", end="")
        for d in all_data:
            val = d["metrics"][key]
            cell = f"{val:.4f}" if isinstance(val, float) else str(val)
            print(f"  {cell:>{col_w}}", end="")
        print()
    print()


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("source_dir", help="Directory containing the source documents")
    parser.add_argument("--kb", default="D6", help="Knowledge base name (default: D6)")
    parser.add_argument("--queries-file", help="JSON file with list of query strings (overrides built-in set)")
    parser.add_argument("--strategies", help="Comma-separated subset of strategies to run (default: all)")
    args = parser.parse_args()

    global QUERIES
    if args.queries_file:
        QUERIES = json.loads(Path(args.queries_file).read_text())
        print(f"Loaded {len(QUERIES)} queries from {args.queries_file}")

    selected = set(args.strategies.split(",")) if args.strategies else None
    strategies = [s for s in STRATEGIES if selected is None or s[0] in selected]

    source_dir = str(Path(args.source_dir).resolve())
    kb = args.kb

    print(f"\n{'='*70}")
    print(f"  RAG Chunk Strategy Test  |  KB: {kb}  |  {len(QUERIES)} queries  |  {len(strategies)} strategies")
    print(f"{'='*70}\n")

    all_data = []

    for name, chunk_size, overlap, min_len, do_reingest in strategies:
        cfg = {"chunk_size": chunk_size, "chunk_overlap": overlap, "min_chunk_len": min_len}
        print(f"▶  {name:<12}  chunk={chunk_size}  overlap={overlap}  min_len={min_len}", end="", flush=True)

        if do_reingest:
            print(f"  [re-ingesting {source_dir}]", flush=True)
            t0 = time.time()
            reingest(source_dir, kb, chunk_size, overlap, min_len)
            print(f"     ingested in {time.time()-t0:.0f}s", flush=True)
        else:
            print("  [using current KB — no re-ingest]", flush=True)

        t0 = time.time()
        results = run_queries(kb)
        print(f"     queried in {time.time()-t0:.1f}s", flush=True)

        m = metrics(results)
        all_data.append({
            "strategy": name,
            "config": cfg,
            "results": [
                {
                    "query": r.query,
                    "top1_score": r.top1_score,
                    "top1_doc": r.top1_doc,
                    "all_scores": r.all_scores,
                    "weak": r.is_weak,
                }
                for r in results
            ],
            "metrics": m,
        })
        print(f"     mean={m['mean_top1']:.4f}  min={m['min_top1']:.4f}  weak={m['weak_count']}/{len(QUERIES)}\n")

    # Restore KB to best strategy (highest mean top-1, fewest weak)
    best = max(all_data, key=lambda d: (
        -d["metrics"]["weak_count"],
        d["metrics"]["mean_top1"],
    ))

    print_strategy_table(all_data)

    print(f"Best strategy: {best['strategy']}  "
          f"(chunk={best['config']['chunk_size']}/overlap={best['config']['chunk_overlap']})  "
          f"mean={best['metrics']['mean_top1']:.4f}  weak={best['metrics']['weak_count']}/{len(QUERIES)}")

    last_strategy = all_data[-1]["strategy"]
    if best["strategy"] != last_strategy:
        print(f"\nRestoring KB '{kb}' to best strategy ({best['strategy']})…")
        if best["strategy"] == "baseline":
            # baseline means current defaults — just re-ingest with defaults
            reingest(source_dir, kb, 800, 200, 100)
        else:
            reingest(
                source_dir, kb,
                best["config"]["chunk_size"],
                best["config"]["chunk_overlap"],
                best["config"]["min_chunk_len"],
            )
        print("Done.")

    out = Path(f"/tmp/rag_chunk_results_{kb}.json")
    out.write_text(json.dumps({"kb": kb, "source_dir": source_dir, "strategies": all_data}, indent=2))
    print(f"\nFull results → {out}")


if __name__ == "__main__":
    main()
