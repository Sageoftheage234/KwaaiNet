#!/usr/bin/env bash
# Patch the cargo-dist PowerShell installer to auto-detect NVIDIA GPUs and
# prefer the CUDA-enabled archive when available.
#
# Usage: patch-installer-cuda-ps1.sh <path-to-kwaainet-installer.ps1>

set -euo pipefail

INSTALLER="${1:?Usage: patch-installer-cuda-ps1.sh <installer-path>}"

if [ ! -f "$INSTALLER" ]; then
    echo "ERROR: installer not found: $INSTALLER"
    exit 1
fi

# Write the CUDA detection function to a temp file.
CUDA_FUNC=$(mktemp)
cat > "$CUDA_FUNC" <<'CUDA_PATCH'

# ── NVIDIA CUDA variant auto-detection (injected by CI) ──────────
function Invoke-CudaUpgrade($download_url, [ref]$artifact_name_ref) {
    $current = $artifact_name_ref.Value
    if ($current -notlike "*x86_64-pc-windows-msvc*") { return }
    $nvidiaSmi = Get-Command "nvidia-smi" -ErrorAction SilentlyContinue
    if (-not $nvidiaSmi) { return }
    try {
        $gpuName = & nvidia-smi --query-gpu=name --format=csv,noheader 2>$null | Select-Object -First 1
    } catch { return }
    if ([string]::IsNullOrWhiteSpace($gpuName)) { return }
    $cudaName = "kwaainet-x86_64-pc-windows-msvc-cuda.zip"
    $cudaUrl = "$download_url/$cudaName"
    try {
        Invoke-WebRequest -Uri $cudaUrl -Method Head -ErrorAction Stop | Out-Null
        Write-Host "NVIDIA GPU detected: $gpuName - using CUDA-enabled build"
        $artifact_name_ref.Value = $cudaName
    } catch {
        Write-Host "CUDA archive not found for this release - falling back to CPU build"
    }
}
CUDA_PATCH

# Insert the function at the top of the file.
{
    head -1 "$INSTALLER"
    cat "$CUDA_FUNC"
    tail -n +2 "$INSTALLER"
} > "${INSTALLER}.tmp"
mv "${INSTALLER}.tmp" "$INSTALLER"
rm -f "$CUDA_FUNC"

# Inject the call just before the URL is constructed from $artifact_name.
# The cargo-dist PS1 installer has:
#   $url = "$download_url/$artifact_name"
# We inject the CUDA upgrade call on the line before it.
if grep -q '\$url = "\$download_url/\$artifact_name"' "$INSTALLER"; then
    sed -i '/\$url = "\$download_url\/\$artifact_name"/i\  Invoke-CudaUpgrade $download_url ([ref]$artifact_name)' "$INSTALLER"
    echo "Patched ${INSTALLER}: CUDA auto-detection added"
else
    echo "WARNING: Could not find '\$url = \"\$download_url/\$artifact_name\"' in PS1 installer"
    echo "  Searching for download-related lines:"
    grep -n 'download_url\|artifact_name\|downloadFile' "$INSTALLER" | head -10
fi
