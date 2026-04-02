#!/usr/bin/env bash
# Patch the cargo-dist shell installer to auto-detect NVIDIA GPUs and
# prefer the CUDA-enabled archive when available.
#
# Usage: patch-installer-cuda.sh <path-to-kwaainet-installer.sh>

set -euo pipefail

INSTALLER="${1:?Usage: patch-installer-cuda.sh <installer-path>}"

if [ ! -f "$INSTALLER" ]; then
    echo "ERROR: installer not found: $INSTALLER"
    exit 1
fi

# Write the CUDA detection function to a temp file.
CUDA_FUNC=$(mktemp)
cat > "$CUDA_FUNC" <<'CUDA_PATCH'

# ── NVIDIA CUDA variant auto-detection (injected by CI) ──────────
# Override the default linux-gnu archive with the CUDA build when an
# NVIDIA GPU is present.  Called after the main install logic sets
# _url and _artifact_name but before download.
_try_cuda_upgrade() {
    # Only applies to x86_64 linux-gnu
    case "${_artifact_name:-}" in
        kwaainet-x86_64-unknown-linux-gnu.tar.xz) ;;
        *) return 0 ;;
    esac
    if ! command -v nvidia-smi >/dev/null 2>&1; then
        return 0
    fi
    _gpu_name=$(nvidia-smi --query-gpu=name --format=csv,noheader 2>/dev/null | head -1)
    if [ -z "$_gpu_name" ]; then return 0; fi
    say "NVIDIA GPU detected: $_gpu_name — using CUDA-enabled build"
    _cuda_name="kwaainet-x86_64-unknown-linux-gnu-cuda.tar.xz"
    _cuda_url="${_url%/*}/${_cuda_name}"
    # Verify the CUDA archive actually exists in this release
    if curl --output /dev/null --silent --head --fail "$_cuda_url"; then
        _artifact_name="$_cuda_name"
        _url="$_cuda_url"
        # Fetch the sha256 sidecar; disable checksum if unavailable
        _cuda_hash=$(curl -sSfL "${_url}.sha256" 2>/dev/null | awk '{print $1}')
        if [ -n "$_cuda_hash" ]; then
            _checksum_value="$_cuda_hash"
        else
            _checksum_style=""
        fi
    else
        say "CUDA archive not found for this release — falling back to CPU build"
    fi
}
CUDA_PATCH

# Insert the function right after the shebang line so it is defined
# before any call site.  Use a temp file to avoid sed multi-line pain.
{
    head -1 "$INSTALLER"
    cat "$CUDA_FUNC"
    tail -n +2 "$INSTALLER"
} > "${INSTALLER}.tmp"
mv "${INSTALLER}.tmp" "$INSTALLER"
chmod +x "$INSTALLER"
rm -f "$CUDA_FUNC"

# Inject the call to _try_cuda_upgrade just before the download.
# The cargo-dist installer calls `downloader "$_url" "$_file"`.
sed -i '/downloader "$_url" "$_file"/i\    _try_cuda_upgrade' "$INSTALLER"

echo "Patched ${INSTALLER}: CUDA auto-detection added"
