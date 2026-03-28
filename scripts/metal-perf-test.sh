#!/usr/bin/env bash
# scripts/metal-perf-test.sh — Automated Metal vs CPU performance diagnostic
#
# Run unattended:
#   cd /Users/rezarassool/Source/KwaaiNet/core && ../scripts/metal-perf-test.sh
#
# Results written to ~/.kwaainet/logs/metal-perf-<timestamp>.log
# Expected runtime: ~5-7 minutes (build + CPU bench + Metal bench)

set -euo pipefail

TIMESTAMP=$(date +%Y%m%d-%H%M%S)
LOG="$HOME/.kwaainet/logs/metal-perf-${TIMESTAMP}.log"
mkdir -p "$HOME/.kwaainet/logs"

log() { echo "[$(date +%H:%M:%S)] $*" | tee -a "$LOG"; }

# Ensure we're in the workspace root
if [ ! -f Cargo.toml ]; then
    echo "ERROR: Run this from core/ (the cargo workspace root)"
    exit 1
fi

BIN=target/release/kwaainet

log "========================================"
log "  Metal Performance Diagnostic"
log "  $(date)"
log "  Log: $LOG"
log "========================================"
log ""

# ── Build ─────────────────────────────────────────────────
log "Step 1/4: Building kwaainet..."
cargo build --release -p kwaainet >>"$LOG" 2>&1
log "Build complete."
log ""

# ── CPU Benchmark (baseline, fast) ────────────────────────
log "Step 2/4: CPU Benchmark (5 decode steps)..."
log "--- CPU BENCHMARK START ---"
$BIN benchmark --steps 5 >>"$LOG" 2>&1
log "--- CPU BENCHMARK END ---"
log ""

# ── Metal Benchmark (instrumented, slow) ──────────────────
log "Step 3/4: Metal Benchmark (1 decode step — expect ~2-3 min)..."
log "--- METAL BENCHMARK START ---"
RUST_LOG=kwaai_inference=debug $BIN benchmark --steps 1 --gpu >>"$LOG" 2>&1
log "--- METAL BENCHMARK END ---"
log ""

# ── Summary ───────────────────────────────────────────────
log "Step 4/4: Extracting summary..."
log ""
log "========================================"
log "  RESULTS"
log "========================================"

SUMMARY="$HOME/.kwaainet/logs/metal-perf-${TIMESTAMP}.summary"

{
    echo "=== Timing from shard.rs instrumentation ==="
    grep -E "\[PERF\]" "$LOG" 2>/dev/null || echo "(no [PERF] lines found — check if eprintln! instrumentation is compiled in)"
    echo ""
    echo "=== Benchmark results ==="
    grep -E "(Prefill|Decode|Device|Load time)" "$LOG" 2>/dev/null || echo "(no benchmark results found)"
    echo ""
    echo "=== Block-level debug timing (first 5 lines) ==="
    grep -E "block forward" "$LOG" 2>/dev/null | head -5 || echo "(no block-level debug found — run with RUST_LOG=kwaai_inference=debug)"
} | tee "$SUMMARY" | tee -a "$LOG"

log ""
log "Full log:  $LOG"
log "Summary:   $SUMMARY"
log "Done."
