#!/usr/bin/env bash
# Real-binary validation for the provenance tooling, run INSIDE the WSL Ubuntu instance that hosts kwaainet.
#
#   bash tests/kwaai-knowledge/provenance/run_wsl_checks.sh [KB] [MODEL]
#
# Steps: unit tests -> snapshot -> binary check -> edit-compatibility check -> 1-cycle lexical smoke run -> report.
# The original KB is only read and copied. Everything is written under ~/kwaai-prov/<timestamp>/, and the
# results (no KB data) are packed into ~/kwaai-prov/<timestamp>-results.tar.gz to send back.
#
# Optional environment variables:
#   KWAAINET_CMD    binary to use (default: kwaainet on PATH)
#   INFERENCE_URL   Ollama URL (default: auto-detect localhost, then the Windows host for WSL2 NAT networking)
#   WITH_NLI=1      also install transformers/torch and run the smoke cycle with the NLI scorer (large download)
#   SKIP_OLLAMA_CHECK=1  do not probe the inference URL (tests only)
set -u
KB="${1:-D6}"
MODEL="${2:-llama3.1:8b}"
KW="${KWAAINET_CMD:-kwaainet}"
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
KK="$(dirname "$HERE")"                      # tests/kwaai-knowledge
STAMP="$(date -u +%Y%m%dT%H%M%SZ)"
ROOT="${PROV_ROOT:-$HOME/kwaai-prov}/$STAMP"
mkdir -p "$ROOT"
SUMMARY="$ROOT/SUMMARY.txt"
log() { echo "$*" | tee -a "$SUMMARY"; }

log "provenance real-binary checks  $STAMP"
log "host: $(uname -a)"
grep -qi microsoft /proc/version 2>/dev/null && log "environment: WSL" || log "environment: not WSL"
REPO="$(git -C "$KK" rev-parse --show-toplevel 2>/dev/null || echo '?')"
log "repo: $REPO  branch: $(git -C "$KK" rev-parse --abbrev-ref HEAD 2>/dev/null || echo '?')  HEAD: $(git -C "$KK" rev-parse --short HEAD 2>/dev/null || echo '?')"
if [ "$(git -C "$KK" rev-parse --abbrev-ref HEAD 2>/dev/null)" = "main" ]; then
  log "WARNING: on main; this work belongs on the experiments branch"
fi

# ---- binary
if ! command -v "${KW%% *}" >/dev/null 2>&1 && [ ! -x "${KW%% *}" ]; then
  log "FAIL: kwaainet not found ($KW). Set KWAAINET_CMD to the Linux build inside WSL."; exit 2
fi
case "$(command -v "${KW%% *}" 2>/dev/null)" in
  *.exe|/mnt/c/*) log "FAIL: $KW resolves to a Windows binary. Use the Linux kwaainet inside WSL."; exit 2;;
esac
log "kwaainet: $($KW --version 2>&1 | head -1)"
if pgrep -x kwaainet >/dev/null 2>&1; then
  log "FAIL: a kwaainet process is running. Stop it (kwaainet stop / end the node) before taking a snapshot."; exit 2
fi

# ---- inference endpoint
if [ -z "${INFERENCE_URL:-}" ]; then
  INFERENCE_URL="http://localhost:11434"
  if [ -z "${SKIP_OLLAMA_CHECK:-}" ] && ! curl -s -m 3 "$INFERENCE_URL/api/tags" >/dev/null 2>&1; then
    WINHOST="$(ip route show 2>/dev/null | awk '/default/ {print $3; exit}')"
    if [ -n "$WINHOST" ] && curl -s -m 3 "http://$WINHOST:11434/api/tags" >/dev/null 2>&1; then
      INFERENCE_URL="http://$WINHOST:11434"
    fi
  fi
fi
if [ -z "${SKIP_OLLAMA_CHECK:-}" ]; then
  if curl -s -m 3 "$INFERENCE_URL/api/tags" | grep -q "\"${MODEL%%:*}"; then
    log "inference: $INFERENCE_URL has $MODEL"
  else
    log "WARNING: $INFERENCE_URL is unreachable or does not list $MODEL; the edit check will be INCONCLUSIVE"
  fi
fi

# ---- python environment (a venv: Ubuntu's system pip refuses installs, PEP 668)
PY="$ROOT/venv/bin/python"
python3 -m venv "$ROOT/venv" >>"$ROOT/setup.log" 2>&1 || { log "FAIL: python3 -m venv failed (sudo apt install python3-venv)"; exit 2; }
"$PY" -m pip install -q pyyaml pytest matplotlib >>"$ROOT/setup.log" 2>&1 || { log "FAIL: pip install failed; see setup.log"; exit 2; }
if [ -n "${WITH_NLI:-}" ]; then
  "$PY" -m pip install -q transformers torch >>"$ROOT/setup.log" 2>&1 || log "WARNING: NLI install failed; see setup.log"
fi
log "python: $("$PY" --version 2>&1)"

cd "$KK" || exit 2
step() {  # name, then the command
  local name="$1"; shift
  "$@" >"$ROOT/$name.log" 2>&1
  local rc=$?
  log "$(printf '%-18s exit %s  %s' "$name" "$rc" "$(tail -1 "$ROOT/$name.log")")"
  return $rc
}

step pytest "$PY" -m pytest provenance/tests -v -p no:cacheprovider
step check_snapshot "$PY" -m provenance.check_snapshot --kb "$KB" --dest "$ROOT/snapshot-486bcf6" \
     --kwaainet "$KW" --out-json "$ROOT/snapshot.json" || { log "stopping: no snapshot"; STOP=1; }
if [ -z "${STOP:-}" ]; then
  step check_binary "$PY" -m provenance.check_binary --snapshot "$ROOT/snapshot-486bcf6" --scratch "$ROOT/scratch" \
       --kwaainet "$KW" --out-json "$ROOT/binary.json"
  step check_edit_compat "$PY" -m provenance.check_edit_compat --snapshot "$ROOT/snapshot-486bcf6" \
       --scratch "$ROOT/scratch-edit" --kwaainet "$KW" --model "$MODEL" --inference-url "$INFERENCE_URL" \
       --out-json "$ROOT/edit.json"
  SCORER=lexical; [ -n "${WITH_NLI:-}" ] && SCORER=nli
  cat >"$ROOT/smoke.yaml" <<EOF
kwaainet_cmd: '$KW'
model: '$MODEL'
inference_url: '$INFERENCE_URL'
repeats: 1
snapshots: [0, 1]
scorer: $SCORER
edit_compat_result: '$ROOT/edit.json'
EOF
  step smoke_run "$PY" -m provenance.cli run --snapshot "$ROOT/snapshot-486bcf6" --out "$ROOT/runs" \
       --config "$ROOT/smoke.yaml" --max-cycle 1 --run-id smoke && \
  step report "$PY" -m provenance.cli report "$ROOT/runs/smoke/results.jsonl" "$ROOT/smoke.csv" --plots "$ROOT/plots"
  [ -f "$ROOT/runs/smoke/RUN.json" ] && log "authorization: $(grep -o '"edit_compat": "[^"]*"' "$ROOT/runs/smoke/RUN.json")"
fi

# ---- bundle results. Left out: KB copies, workspaces, the venv, and SNAPSHOT.json (it holds a copy of your
# kwaainet config.yaml, which may contain keys). results.jsonl does contain fact texts from the KB.
tar -czf "$ROOT-results.tar.gz" -C "$ROOT" \
  --exclude='snapshot-486bcf6/data' --exclude='snapshot-486bcf6/SNAPSHOT.json' --exclude='scratch*' --exclude='runs/*/workspaces' --exclude='venv' . 2>/dev/null
log "results: $ROOT-results.tar.gz"
