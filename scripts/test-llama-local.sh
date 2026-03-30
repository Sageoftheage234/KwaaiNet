#!/usr/bin/env bash
# Test llama.cpp local inference backend
# Downloads GGUF model if needed, builds with llama-cpp feature, runs inference
set -euo pipefail

MODEL_DIR="$HOME/.kwaainet/models"
MODEL="$MODEL_DIR/llama-3.1-8b-instruct-q4_k_m.gguf"
LOG="$HOME/.kwaainet/logs/llama-local-test-$(date +%Y%m%d-%H%M%S).log"
mkdir -p "$MODEL_DIR" "$HOME/.kwaainet/logs"

log() { echo "[$(date +%H:%M:%S)] $*" | tee -a "$LOG"; }

cd "$(dirname "$0")/../core"

log "=== llama.cpp Local Inference Test ==="

# Check model
if [ ! -f "$MODEL" ]; then
    log "Downloading Llama 3.1 8B GGUF Q4_K_M (~4.7GB)..."
    curl -L -o "$MODEL" \
        "https://huggingface.co/bartowski/Meta-Llama-3.1-8B-Instruct-GGUF/resolve/main/Meta-Llama-3.1-8B-Instruct-Q4_K_M.gguf" \
        2>&1 | tee -a "$LOG"
fi
log "Model: $MODEL ($(du -h "$MODEL" | cut -f1))"

# Build
log "Building with llama-cpp feature..."
cargo build --release -p kwaainet --features llama-cpp 2>&1 | tee -a "$LOG"
log "Build complete."

# TODO: Run inference test once cmd_shard_run_local is wired to llama_local
log "Build succeeded. Inference wiring pending."
log "Log: $LOG"
