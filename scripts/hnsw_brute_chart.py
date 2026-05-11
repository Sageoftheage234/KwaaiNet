#!/usr/bin/env python3
"""
Generate HNSW vs Brute-Force benchmark chart.
Outputs: docs/hnsw_vs_brute_force.png
"""

import matplotlib
matplotlib.use("Agg")
import matplotlib.pyplot as plt
import matplotlib.ticker as ticker
import numpy as np
from pathlib import Path

# ── Section 1: ef_search sweep at 10K corpus, random vectors ─────────────────
ef_values  = [32,   64,    128,   256,   400,   512,   1024]
ef_hnsw_us = [999,  1451,  2402,  3858,  4916,  5521,  6933]
ef_recall  = [24.3, 34.5,  52.5,  73.9,  85.7,  90.1,  95.8]
BRUTE_10K  = 8700  # µs (representative median)

# ── Section 3: Corpus sweep — clustered vectors (realistic) ──────────────────
corpus       = [200,  500,  865,  1_000, 2_000, 5_000, 10_000, 50_000]
rand_rec64   = [99.9, 96.7, 95.7, 93.7,  80.4,  55.1,  35.9,   11.7]
rand_rec200  = [100., 98.3, 98.6, 98.9,  97.1,  88.2,  69.5,   28.3]
clus_rec64   = [99.3, 97.7, 97.0, 97.7,  97.7,  98.0,  97.8,   98.5]
clus_rec200  = [99.6, 98.6, 98.1, 98.7,  98.5,  98.0,  98.0,   98.6]

# ── Section 4: Build params ───────────────────────────────────────────────────
efc_values  = [32,    64,    100,   200,   400]
efc_recall  = [25.8,  98.3,  98.5,  98.5,  98.6]
efc_build_s = [2.7,   10.2,  16.7,  26.3,  34.3]

m_values    = [8,     12,    16,    24,    32,    48]
m_recall    = [94.4,  97.3,  98.6,  99.5,  99.4,  99.8]
m_build_s   = [13.8,  19.7,  26.1,  35.4,  37.7,  39.4]

THRESHOLD = 2_000
D6_SIZE   = 865

# ── Style ──────────────────────────────────────────────────────────────────────
BLUE   = "#2563EB"
RED    = "#DC2626"
GREEN  = "#16A34A"
AMBER  = "#D97706"
PURPLE = "#7C3AED"
GRAY   = "#6B7280"

plt.rcParams.update({
    "font.family":      "sans-serif",
    "font.size":        10.5,
    "axes.spines.top":  False,
    "axes.spines.right":False,
    "axes.grid":        True,
    "grid.color":       "#E5E7EB",
    "grid.linewidth":   0.8,
    "figure.dpi":       150,
})

fig, axes = plt.subplots(2, 2, figsize=(15, 10))
fig.suptitle(
    "HNSW Parameter Tuning  •  dim=768  top_k=10  m=16  (release build, Apple Silicon)",
    fontsize=13, fontweight="bold", y=1.01,
)
axes = axes.flatten()

# ── Panel 1: ef_search sweep — recall vs latency ─────────────────────────────
ax = axes[0]
ax2 = ax.twinx()
ax2.spines["top"].set_visible(False)

bar_colors = [GREEN if r >= 90 else AMBER if r >= 70 else RED for r in ef_recall]
bars = ax.bar(range(len(ef_values)), ef_recall, color=bar_colors, alpha=0.75, width=0.55)
ax.axhline(90, color=AMBER, linewidth=1.2, linestyle="--", alpha=0.8)
ax.set_ylim(0, 115)
ax.set_ylabel("Recall@10 (%)")
ax.set_xticks(range(len(ef_values)))
ax.set_xticklabels([str(e) for e in ef_values])
ax.set_xlabel("ef_search  (10K random vectors, ef_construction=200)")
ax.set_title("ef_search: Recall vs Latency\n(10K corpus, random vectors)", fontweight="bold")

ax2.plot(range(len(ef_values)), ef_hnsw_us, "o-", color=BLUE, linewidth=2, markersize=5)
ax2.axhline(BRUTE_10K, color=RED, linewidth=1.2, linestyle=":", alpha=0.8)
ax2.set_ylabel("Latency (µs)", color=BLUE)
ax2.tick_params(axis="y", labelcolor=BLUE)
ax2.spines["right"].set_visible(True)
ax2.spines["right"].set_color(BLUE)

for i, (bar, r) in enumerate(zip(bars, ef_recall)):
    ax.text(bar.get_x() + bar.get_width()/2, r + 2, f"{r:.0f}%",
            ha="center", va="bottom", fontsize=8, fontweight="bold")

ax.annotate("← old\ndefault\n(ef=64)", (1, ef_recall[1] + 5), ha="center", fontsize=8, color=GRAY)

legend_items = [
    plt.Rectangle((0,0),1,1, color=GREEN, alpha=0.75, label="≥90% recall"),
    plt.Rectangle((0,0),1,1, color=AMBER, alpha=0.75, label="70–90% recall"),
    plt.Rectangle((0,0),1,1, color=RED,   alpha=0.75, label="<70% recall"),
    plt.Line2D([0],[0], color=BLUE, marker="o", linewidth=2, label="HNSW latency"),
    plt.Line2D([0],[0], color=RED,  linewidth=1.2, linestyle=":", label=f"Brute-force (~{BRUTE_10K}µs)"),
    plt.Line2D([0],[0], color=AMBER,linewidth=1.2, linestyle="--", label="90% target"),
]
ax.legend(handles=legend_items, fontsize=7.5, loc="lower right")

# ── Panel 2: Recall — random vs clustered ────────────────────────────────────
ax = axes[1]

def shared_x_log(ax):
    ax.set_xscale("log")
    ax.set_xticks(corpus)
    ax.get_xaxis().set_major_formatter(ticker.FuncFormatter(lambda v, _: f"{int(v):,}"))
    ax.tick_params(axis="x", rotation=45)
    ax.set_xlabel("Corpus size (log scale)")
    ax.axvline(THRESHOLD, color=AMBER, linewidth=1.4, linestyle="--", alpha=0.8)
    ax.axvline(D6_SIZE,   color=GREEN, linewidth=1.4, linestyle=":",  alpha=0.9)

ax.plot(corpus, rand_rec64,  "o--", color=RED,    linewidth=2,   markersize=5, label="Random ef=64")
ax.plot(corpus, rand_rec200, "o-",  color=RED,    linewidth=2.2, markersize=5, alpha=0.5, label="Random ef=200")
ax.plot(corpus, clus_rec64,  "s--", color=BLUE,   linewidth=2,   markersize=5, label="Clustered ef=64 (realistic)")
ax.plot(corpus, clus_rec200, "s-",  color=PURPLE, linewidth=2.2, markersize=5, label="Clustered ef=200")
ax.axhline(90, color=AMBER, linewidth=1.0, linestyle="--", alpha=0.7)
ax.set_ylim(0, 108)
ax.set_ylabel("Recall@10 (%)")
ax.set_title("Recall@10: Random vs Clustered\n(ef=64 and ef=200)", fontweight="bold")
shared_x_log(ax)

ax.legend(fontsize=7.5, loc="lower left")
ax.text(350,  5, "brute zone", ha="center", fontsize=8, color=GREEN, alpha=0.9)
ax.text(12000, 5, "HNSW zone",  ha="center", fontsize=8, color=GRAY, alpha=0.9)

# ── Panel 3: ef_construction sweep ───────────────────────────────────────────
ax = axes[2]
ax2 = ax.twinx()
ax2.spines["top"].set_visible(False)

efc_colors = [RED if r < 50 else GREEN for r in efc_recall]
bars = ax.bar(range(len(efc_values)), efc_recall, color=efc_colors, alpha=0.75, width=0.5)
ax.axhline(90, color=AMBER, linewidth=1.2, linestyle="--", alpha=0.8)
ax.set_ylim(0, 115)
ax.set_ylabel("Recall@10 (%)")
ax.set_xticks(range(len(efc_values)))
ax.set_xticklabels([str(e) for e in efc_values])
ax.set_xlabel("ef_construction  (m=16, ef_search=128, 10K clustered)")
ax.set_title("ef_construction: Recall vs Build Time\n(m=16, 10K clustered vectors)", fontweight="bold")

ax2.plot(range(len(efc_values)), efc_build_s, "o-", color=BLUE, linewidth=2, markersize=5)
ax2.set_ylabel("Build time (s)", color=BLUE)
ax2.tick_params(axis="y", labelcolor=BLUE)
ax2.spines["right"].set_visible(True)
ax2.spines["right"].set_color(BLUE)

for i, (bar, r) in enumerate(zip(bars, efc_recall)):
    ax.text(bar.get_x() + bar.get_width()/2, r + 2, f"{r:.0f}%",
            ha="center", va="bottom", fontsize=8.5, fontweight="bold")

ax.annotate("← old\n(64)", (1, 90), ha="center", fontsize=8, color=GRAY)
ax.annotate("← new\n(200)", (3, 90), ha="center", fontsize=8, color=GREEN)

legend_items3 = [
    plt.Rectangle((0,0),1,1, color=RED,   alpha=0.75, label="Broken (<50% recall)"),
    plt.Rectangle((0,0),1,1, color=GREEN, alpha=0.75, label="Acceptable (≥90%)"),
    plt.Line2D([0],[0], color=BLUE, marker="o", linewidth=2, label="Build time (s)"),
    plt.Line2D([0],[0], color=AMBER,linewidth=1.2, linestyle="--", label="90% target"),
]
ax.legend(handles=legend_items3, fontsize=7.5, loc="lower right")

# ── Panel 4: m sweep ──────────────────────────────────────────────────────────
ax = axes[3]
ax2 = ax.twinx()
ax2.spines["top"].set_visible(False)

m_colors = [RED if r < 95 else AMBER if r < 98 else GREEN for r in m_recall]
bars = ax.bar(range(len(m_values)), m_recall, color=m_colors, alpha=0.75, width=0.5)
ax.set_ylim(90, 101)
ax.set_ylabel("Recall@10 (%)")
ax.set_xticks(range(len(m_values)))
ax.set_xticklabels([str(m) for m in m_values])
ax.set_xlabel("m (bidirectional links per node)  ef_construction=200, ef_search=128")
ax.set_title("m: Recall vs Build Time\n(ef_construction=200, 10K clustered)", fontweight="bold")

ax2.plot(range(len(m_values)), m_build_s, "o-", color=BLUE, linewidth=2, markersize=5)
ax2.set_ylabel("Build time (s)", color=BLUE)
ax2.tick_params(axis="y", labelcolor=BLUE)
ax2.spines["right"].set_visible(True)
ax2.spines["right"].set_color(BLUE)

for i, (bar, r) in enumerate(zip(bars, m_recall)):
    ax.text(bar.get_x() + bar.get_width()/2, r + 0.1, f"{r:.1f}%",
            ha="center", va="bottom", fontsize=8, fontweight="bold")

ax.annotate("default\n(m=16)", (2, 97.5), ha="center", fontsize=8, color=GRAY)
ax.annotate("+1.5pp\n(m=24)", (3, 98.8), ha="center", fontsize=8, color=GREEN)

legend_items4 = [
    plt.Rectangle((0,0),1,1, color=RED,   alpha=0.75, label="<95% recall"),
    plt.Rectangle((0,0),1,1, color=AMBER, alpha=0.75, label="95–98% recall"),
    plt.Rectangle((0,0),1,1, color=GREEN, alpha=0.75, label="≥98% recall"),
    plt.Line2D([0],[0], color=BLUE, marker="o", linewidth=2, label="Build time (s)"),
]
ax.legend(handles=legend_items4, fontsize=7.5, loc="lower right")

# ── Final layout ──────────────────────────────────────────────────────────────
plt.tight_layout()

out = Path(__file__).parent.parent / "docs" / "hnsw_vs_brute_force.png"
out.parent.mkdir(parents=True, exist_ok=True)
plt.savefig(out, bbox_inches="tight")
print(f"Chart saved → {out}")
