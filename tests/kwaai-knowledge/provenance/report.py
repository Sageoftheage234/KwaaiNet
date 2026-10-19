"""Flatten results.jsonl into CSVs and plots with repeat-level spread (audit items 6 and 7).

Outputs:
  <csv>                     one row per summary record (arm, repeat, cycle, phase)
  <csv stem>.stats.csv      per arm × phase × cycle × metric: n, values, mean, sd, min, max, 95% t-interval
  <csv>.metadata.json       provenance of the report
  <plots>/primary-endpoint.png, <plots>/keyword-recall-delta.png (when eval rows exist)

With 3 repeats the t-interval uses 2 degrees of freedom (t = 4.303); it is wide and imprecise by design and
should be read as a statement about run-to-run variation of this pipeline, not population inference.
"""
from __future__ import annotations

import csv
import json
import math
import statistics
from collections import defaultdict
from pathlib import Path

from .retrieval_cost import DEFINITIONS, METRICS, NOT_COMPUTED, gate_cost

_T975 = {1: 12.706, 2: 4.303, 3: 3.182, 4: 2.776, 5: 2.571, 6: 2.447, 7: 2.365, 8: 2.306, 9: 2.262, 10: 2.228,
         12: 2.179, 15: 2.131, 20: 2.086, 30: 2.042}

FIELDS = ["run_id", "snapshot_id", "git_commit", "scorer_status", "arm", "repeat", "cycle", "phase", "primary",
          "end_of_cycle", "status", "gate", "gate_mode", "cycle0_facts", "present_facts", "lost", "added",
          "added_unsupported", "unsupported_present_over_cycle0", "unsupported_over_present",
          "traceable_over_present", "lost_over_cycle0", "unsupported_or_lost_over_cycle0",
          "records_sharing_content", "lost_to_prune_after_gate_isolation",
          "traceable", "misattributed", "unsupported", "unlinked",
          "gate_rejected", "gate_removed_relations", "gate_removed_fields",
          "virtual_immediate_rejected", "virtual_immediate_unsupported_present_over_cycle0_if_removed_now",
          "desc_cycle0_facts", "desc_present_facts", "desc_unsupported_present_over_cycle0",
          "desc_traceable_over_present", "desc_lost",
          "retrieval_keyword_recall", "generation_keyword_recall",
          "retrieval_keyword_recall_delta_vs_ungated_same_repeat_index",
          "generation_keyword_recall_delta_vs_ungated_same_repeat_index", "recall_at_k", "mrr", "reason"]
STAT_METRICS = ["unsupported_present_over_cycle0", "traceable_over_present", "unsupported_over_present",
                "lost_over_cycle0", "added_unsupported", "desc_unsupported_present_over_cycle0",
                "retrieval_keyword_recall", "generation_keyword_recall",
                "retrieval_keyword_recall_delta_vs_ungated_same_repeat_index",
                "generation_keyword_recall_delta_vs_ungated_same_repeat_index"]


def t975(df: int) -> float:
    if df <= 0:
        return math.nan
    keys = sorted(_T975)
    for k in keys:
        if df <= k:
            return _T975[k]
    return 1.96


def describe(values: list[float]) -> dict:
    v = [x for x in values if x is not None]
    n = len(v)
    if n == 0:
        return {"n": 0}
    mean = statistics.fmean(v)
    sd = statistics.stdev(v) if n > 1 else None
    half = t975(n - 1) * sd / math.sqrt(n) if sd is not None else None
    return {"n": n, "values": v, "mean": mean, "sd": sd, "min": min(v), "max": max(v),
            "ci95_low": mean - half if half is not None else None,
            "ci95_high": mean + half if half is not None else None,
            "interval": "t-based 95% interval over repeats" if half is not None else "n=1: no interval"}


def _flatten(r: dict, status: str, ungated_eval: dict) -> dict:
    flat = {k: r.get(k) for k in FIELDS}
    flat.update(r.get("counts") or {})
    for k, v in (r.get("loss_attribution") or {}).items():
        flat["lost|" + k] = v
    flat["scorer_status"] = status
    gi = r.get("gate_immediate") or {}
    flat["gate_rejected"], flat["gate_removed_relations"], flat["gate_removed_fields"] = \
        gi.get("rejected"), gi.get("removed_relations"), gi.get("removed_fields")
    vg = r.get("virtual_gate_immediate") or {}
    flat["virtual_immediate_rejected"] = vg.get("rejected")
    flat["virtual_immediate_unsupported_present_over_cycle0_if_removed_now"] = \
        vg.get("unsupported_present_over_cycle0_if_removed_now")
    d = r.get("descriptions") or {}
    for k in ("cycle0_facts", "present_facts", "unsupported_present_over_cycle0", "traceable_over_present", "lost"):
        flat["desc_" + k] = d.get(k)
    ev = r.get("eval")
    if ev:
        for m in METRICS:
            flat[m] = ev.get(m)
        flat["recall_at_k"], flat["mrr"] = ev.get("recall_at_k"), ev.get("mrr")
        base = ungated_eval.get((r["repeat"], r["cycle"]))
        if base is not None and r["arm"] != "ungated":
            c = gate_cost(ev, base)
            for m in METRICS:
                flat[f"{m}_delta_vs_ungated_same_repeat_index"] = c[f"{m}_delta"]
    return flat


def build(results_jsonl, out_csv, plot_dir=None) -> Path:
    rows = [json.loads(l) for l in Path(results_jsonl).read_text(encoding="utf-8").splitlines()]
    rows = [r for r in rows if r.get("record_type") == "summary"]
    if not rows:
        raise ValueError("no summary rows found")
    lexical = any(str(r.get("config", {}).get("scorer", "")).lower() == "lexical" for r in rows)
    status = "NOT FOR REPORTING" if lexical else "reportable"
    ungated_eval = {(r["repeat"], r["cycle"]): r["eval"] for r in rows
                    if r["arm"] == "ungated" and r.get("status") == "ok" and r.get("end_of_cycle") and r.get("eval")}
    flat = [_flatten(r, status, ungated_eval) for r in rows]
    extra = sorted({k for f in flat for k in f} - set(FIELDS))
    out_csv = Path(out_csv)
    with open(out_csv, "w", newline="", encoding="utf-8") as fh:
        w = csv.DictWriter(fh, fieldnames=FIELDS + extra)
        w.writeheader()
        w.writerows(flat)

    groups: dict[tuple, list[dict]] = defaultdict(list)
    for f in flat:
        if f.get("status") == "ok":
            groups[(f["arm"], f["gate_mode"], f["phase"], bool(f["primary"]), f["cycle"])].append(f)
    stats_rows = []
    for (arm, mode, phase, primary, cycle), fs in sorted(groups.items(), key=lambda x: (x[0][0], x[0][4], x[0][2])):
        for m in STAT_METRICS:
            d = describe([f.get(m) for f in fs])
            if d["n"] == 0:
                continue
            stats_rows.append({"arm": arm, "gate_mode": mode, "phase": phase, "primary": primary, "cycle": cycle,
                               "metric": m, "n_repeats": d["n"], "mean": d["mean"], "sd": d["sd"], "min": d["min"],
                               "max": d["max"], "ci95_low": d["ci95_low"], "ci95_high": d["ci95_high"],
                               "interval": d["interval"], "values": json.dumps(d["values"]),
                               "scorer_status": status})
    # difference of arm means for keyword recall (the honest unpaired comparison)
    means = {(s["arm"], s["cycle"], s["metric"]): s for s in stats_rows if s["metric"] in METRICS}
    for (arm, cycle, m), s in list(means.items()):
        u = means.get(("ungated", cycle, m))
        if arm != "ungated" and u is not None:
            stats_rows.append({"arm": arm, "gate_mode": s["gate_mode"], "phase": s["phase"], "primary": s["primary"],
                               "cycle": cycle, "metric": f"{m}_difference_of_means_vs_ungated",
                               "n_repeats": f"{s['n_repeats']} vs {u['n_repeats']}",
                               "mean": s["mean"] - u["mean"], "sd": None, "min": None, "max": None,
                               "ci95_low": None, "ci95_high": None, "interval": "point estimate",
                               "values": "", "scorer_status": status})
    stats_path = out_csv.with_name(out_csv.stem + ".stats.csv")
    with open(stats_path, "w", newline="", encoding="utf-8") as fh:
        w = csv.DictWriter(fh, fieldnames=["arm", "gate_mode", "phase", "primary", "cycle", "metric", "n_repeats",
                                           "mean", "sd", "min", "max", "ci95_low", "ci95_high", "interval",
                                           "values", "scorer_status"])
        w.writeheader()
        w.writerows(stats_rows)
    Path(str(out_csv) + ".metadata.json").write_text(json.dumps(
        {"git_commit": rows[0].get("git_commit"), "snapshot_id": rows[0].get("snapshot_id"),
         "run_id": rows[0].get("run_id"), "config": rows[0].get("config"), "source": str(results_jsonl),
         "primary_phase": rows[0].get("primary_phase"), "scorer_status": status, "stats": str(stats_path),
         "keyword_recall_definitions": DEFINITIONS, "not_computed": NOT_COMPUTED,
         "notes": ["virtual gate_mode arms ran ungated; their virtual_immediate_* columns are single-step "
                   "counterfactuals, not longitudinal outcomes",
                   "keyword-recall deltas matched by repeat index are a convention, not a true pairing; the "
                   "difference of arm means is also given",
                   "intervals are t-based over repeats; with 3 repeats they are imprecise"]},
        indent=2, default=str), encoding="utf-8")
    if plot_dir:
        _plots(flat, stats_rows, Path(plot_dir), status)
    return out_csv


def _plots(flat, stats_rows, pd: Path, status: str):
    import matplotlib
    matplotlib.use("Agg")
    import matplotlib.pyplot as plt
    pd.mkdir(parents=True, exist_ok=True)
    primary = [f for f in flat if f.get("status") == "ok" and f.get("primary")]
    fig, ax = plt.subplots(figsize=(7, 4.5))
    for arm in sorted({f["arm"] for f in primary}):
        mode = next(f["gate_mode"] for f in primary if f["arm"] == arm)
        label = f"{arm} (virtual: ran ungated)" if mode == "virtual" else arm
        pts = [f for f in primary if f["arm"] == arm]
        st = sorted((s for s in stats_rows if s["arm"] == arm and s["primary"]
                     and s["metric"] == "unsupported_present_over_cycle0"), key=lambda s: s["cycle"])
        xs = [s["cycle"] for s in st]
        line = ax.plot(xs, [s["mean"] for s in st], marker="o", label=label,
                       linestyle="--" if mode == "virtual" else "-")[0]
        lo = [s["ci95_low"] if s["ci95_low"] is not None else s["mean"] for s in st]
        hi = [s["ci95_high"] if s["ci95_high"] is not None else s["mean"] for s in st]
        ax.fill_between(xs, lo, hi, alpha=0.12, color=line.get_color())
        ax.scatter([p["cycle"] for p in pts], [p["unsupported_present_over_cycle0"] for p in pts], s=10,
                   alpha=0.5, color=line.get_color())
    ax.set_xlabel("dream cycle")
    ax.set_ylabel("unsupported facts present / cycle-0 facts")
    ax.set_title(f"Primary endpoint at the primary phase: mean, 95% t-interval, every repeat ({status})", fontsize=9)
    ax.legend(fontsize=7)
    fig.savefig(pd / "primary-endpoint.png", bbox_inches="tight")
    plt.close(fig)
    delta = [s for s in stats_rows if s["metric"] == "retrieval_keyword_recall_delta_vs_ungated_same_repeat_index"]
    if delta:
        fig, ax = plt.subplots(figsize=(7, 4.5))
        for arm in sorted({s["arm"] for s in delta}):
            st = sorted((s for s in delta if s["arm"] == arm), key=lambda s: s["cycle"])
            ax.errorbar([s["cycle"] for s in st], [s["mean"] for s in st],
                        yerr=[[s["mean"] - (s["ci95_low"] if s["ci95_low"] is not None else s["mean"]) for s in st],
                              [(s["ci95_high"] if s["ci95_high"] is not None else s["mean"]) - s["mean"] for s in st]],
                        marker="o", capsize=3, label=arm)
        ax.axhline(0, color="grey", linewidth=0.8)
        ax.set_xlabel("dream cycle")
        ax.set_ylabel("retrieval keyword recall, arm − ungated")
        ax.set_title(f"Retrieval keyword-recall cost (not Recall@k) ({status})", fontsize=9)
        ax.legend(fontsize=7)
        fig.savefig(pd / "keyword-recall-delta.png", bbox_inches="tight")
        plt.close(fig)
