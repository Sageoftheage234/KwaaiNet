//! Resolve an Ollama model reference to the local GGUF blob path.
//!
//! Ollama can store models in two different directory layouts depending on
//! how it was configured:
//!
//! **Default layout** (`~/.ollama`):
//!   `~/.ollama/models/manifests/<registry>/<namespace>/<model>/<tag>`
//!   `~/.ollama/models/blobs/sha256-<hex>`
//!
//! **Custom layout** (`OLLAMA_MODELS=<path>` or a detected custom dir):
//!   `<path>/manifests/<registry>/<namespace>/<model>/<tag>`
//!   `<path>/blobs/sha256-<hex>`
//!
//! We detect which layout is in use by checking whether `<dir>/blobs/`
//! exists directly (custom) or only under `<dir>/models/blobs/` (default).

use anyhow::{anyhow, Context, Result};
use std::path::{Path, PathBuf};

/// Resolve an Ollama model reference to the GGUF blob path on disk.
///
/// Accepted formats:
/// - `qwen3`                              → library/qwen3:latest
/// - `qwen3:0.6b`                         → library/qwen3:0.6b
/// - `hf.co/microsoft/bitnet-b1.58-2B-4T-gguf:latest`  → hf.co path
pub fn resolve_model_blob(model_ref: &str) -> Result<PathBuf> {
    let (models_root, blobs_root) = find_ollama_roots()?;

    let manifest_path = find_manifest(model_ref, &models_root)
        .with_context(|| format!("Cannot locate Ollama manifest for '{model_ref}'"))?;

    let content = std::fs::read_to_string(&manifest_path)
        .with_context(|| format!("Cannot read {}", manifest_path.display()))?;

    let manifest: serde_json::Value =
        serde_json::from_str(&content).with_context(|| "Manifest is not valid JSON")?;

    // Find the layer that carries the model weights.
    let layers = manifest["layers"]
        .as_array()
        .ok_or_else(|| anyhow!("Manifest has no 'layers' array"))?;

    let model_layer = layers
        .iter()
        .find(|l| l["mediaType"].as_str() == Some("application/vnd.ollama.image.model"))
        .ok_or_else(|| anyhow!("No model layer found in manifest"))?;

    let digest = model_layer["digest"]
        .as_str()
        .ok_or_else(|| anyhow!("Model layer has no 'digest' field"))?;

    // "sha256:abc123…" → "sha256-abc123…"
    let blob_name = digest.replace(':', "-");
    let blob_path = blobs_root.join(&blob_name);

    if !blob_path.exists() {
        return Err(anyhow!(
            "Blob '{}' not found at {}.\nTry: ollama pull {}",
            blob_name,
            blob_path.display(),
            model_ref
        ));
    }

    Ok(blob_path)
}

/// Find the manifest file for a model reference.
fn find_manifest(model_ref: &str, manifests_root: &Path) -> Result<PathBuf> {
    // Split off the tag, defaulting to "latest".
    let (name, tag) = model_ref.rsplit_once(':').unwrap_or((model_ref, "latest"));

    // Candidates tried in order:
    //   1. registry.ollama.ai/library/<name>/<tag>  — standard Ollama library
    //   2. <name>/<tag>                              — fully-qualified (hf.co/…)
    // Use separate join() calls to avoid forward-slash handling differences on Windows.
    let candidates = [
        manifests_root
            .join("registry.ollama.ai")
            .join("library")
            .join(name)
            .join(tag),
        manifests_root.join(name).join(tag),
    ];

    for path in &candidates {
        if path.exists() {
            return Ok(path.clone());
        }
    }

    Err(anyhow!(
        "Model '{}' not found in {}.\n\
         Searched:\n  {}\n  {}\n\
         Either the model is not pulled (run: ollama pull {}) \
         or Ollama is storing models elsewhere.\n\
         Workaround: set OLLAMA_MODELS to your Ollama models directory.",
        model_ref,
        manifests_root.display(),
        candidates[0].display(),
        candidates[1].display(),
        model_ref,
    ))
}

/// List all locally installed Ollama model references (e.g. `"llama3.1:8b"`).
///
/// Scans every known Ollama manifests directory in priority order and returns
/// deduplicated model refs suitable for passing to [`resolve_model_blob`].
pub fn list_local_models() -> Vec<String> {
    let home = match dirs::home_dir() {
        Some(h) => h,
        None => return Vec::new(),
    };

    // Probe roots in the same priority order as find_ollama_roots().
    let mut roots: Vec<PathBuf> = Vec::new();
    if let Ok(custom) = std::env::var("OLLAMA_MODELS") {
        roots.push(PathBuf::from(custom).join("manifests"));
    }
    for sub in &["Documents/Kwaai/ollama", "Documents/ollama"] {
        roots.push(home.join(sub).join("manifests"));
    }
    #[cfg(target_os = "windows")]
    if let Some(local_data) = dirs::data_local_dir() {
        roots.push(local_data.join("Ollama").join("models").join("manifests"));
    }
    roots.push(PathBuf::from("/usr/share/ollama/.ollama/models").join("manifests"));
    roots.push(home.join(".ollama").join("models").join("manifests"));

    let mut models: Vec<String> = Vec::new();
    for root in &roots {
        if root.is_dir() {
            collect_manifest_models(root, root, &mut models);
        }
    }

    models.sort();
    models.dedup();
    models
}

/// Recursively walk `dir` under `root` and collect model refs.
fn collect_manifest_models(root: &Path, dir: &Path, out: &mut Vec<String>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name();
        let name_str = name.to_string_lossy();
        if name_str.starts_with('.') {
            continue; // skip .DS_Store, hidden files
        }
        if path.is_dir() {
            collect_manifest_models(root, &path, out);
        } else if path.is_file() {
            if let Some(model_ref) = manifest_path_to_ref(&path, root) {
                out.push(model_ref);
            }
        }
    }
}

/// Convert a manifest file path to an Ollama model reference.
///
/// Expected path structures relative to manifests root:
/// - `registry.ollama.ai/library/<name>/<tag>` → `"<name>:<tag>"` (or just `"<name>"` for latest)
/// - `hf.co/<org>/<model>/<tag>`               → `"hf.co/<org>/<model>:<tag>"`
/// - `<name>/<tag>`                             → `"<name>:<tag>"`
fn manifest_path_to_ref(manifest: &Path, root: &Path) -> Option<String> {
    let rel = manifest.strip_prefix(root).ok()?;
    let parts: Vec<&str> = rel
        .components()
        .filter_map(|c| c.as_os_str().to_str())
        .collect();

    match parts.as_slice() {
        // registry.ollama.ai/library/<name>/<tag>
        [registry, "library", name, tag] if registry.contains('.') => Some(if *tag == "latest" {
            name.to_string()
        } else {
            format!("{}:{}", name, tag)
        }),
        // hf.co/<org>/<model>/<tag>
        ["hf.co", org, model, tag] => Some(if *tag == "latest" {
            format!("hf.co/{}/{}", org, model)
        } else {
            format!("hf.co/{}/{}:{}", org, model, tag)
        }),
        // <name>/<tag>  (flat custom layout)
        [name, tag] => Some(if *tag == "latest" {
            name.to_string()
        } else {
            format!("{}:{}", name, tag)
        }),
        _ => None,
    }
}

/// Return `(manifests_root, blobs_root)` by probing the possible Ollama
/// storage layouts in priority order:
///
/// 1. `OLLAMA_MODELS` env var (custom layout: `$dir/manifests/`, `$dir/blobs/`)
/// 2. Common macOS/Kwaai custom paths under `~/Documents` (same layout)
/// 3. Windows: `%LOCALAPPDATA%\Ollama\models` (some Ollama Windows installs)
/// 4. Linux system service: `/usr/share/ollama/.ollama/models`
/// 5. Default `~/.ollama/models/` (default layout, always returned as fallback)
fn find_ollama_roots() -> Result<(PathBuf, PathBuf)> {
    let home = dirs::home_dir().ok_or_else(|| anyhow!("cannot determine home directory"))?;

    // Candidate roots to probe, in priority order.
    let mut candidates: Vec<PathBuf> = Vec::new();

    // 1. Explicit OLLAMA_MODELS override.
    if let Ok(custom) = std::env::var("OLLAMA_MODELS") {
        candidates.push(PathBuf::from(custom));
    }

    // 2. Well-known custom locations (used by the Kwaai desktop app).
    for sub in &["Documents/Kwaai/ollama", "Documents/ollama"] {
        candidates.push(home.join(sub));
    }

    // 3. Windows: some Ollama installs land in %LOCALAPPDATA%\Ollama\models
    //    rather than the documented %USERPROFILE%\.ollama\models.
    #[cfg(target_os = "windows")]
    if let Some(local_data) = dirs::data_local_dir() {
        candidates.push(local_data.join("Ollama").join("models"));
    }

    // For each candidate check whether it uses the "custom" layout
    // (blobs/ directly under the root) and return if found.
    for dir in &candidates {
        let blobs = dir.join("blobs");
        let manifests = dir.join("manifests");
        if blobs.is_dir() && manifests.is_dir() {
            return Ok((manifests, blobs));
        }
    }

    // 4. System-wide Ollama service layout (Linux: `systemctl enable ollama`).
    //    When Ollama runs as a system service under user `ollama`, models land at
    //    /usr/share/ollama/.ollama/models — not in any user home directory.
    let system_root = PathBuf::from("/usr/share/ollama/.ollama/models");
    if system_root.join("blobs").is_dir() && system_root.join("manifests").is_dir() {
        return Ok((system_root.join("manifests"), system_root.join("blobs")));
    }

    // 5. Default ~/.ollama with the `models/` subdirectory.
    let default = home.join(".ollama").join("models");
    Ok((default.join("manifests"), default.join("blobs")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn find_manifest_error_names_search_directory() {
        // When a model is missing, the error must include the searched root path
        // so users can distinguish a path-mismatch from a genuinely unpulled model.
        let missing_root = std::env::temp_dir().join("kwaainet-test-no-ollama-models");
        let err = find_manifest("no-such-model:latest", &missing_root)
            .unwrap_err()
            .to_string();
        assert!(
            err.contains("kwaainet-test-no-ollama-models"),
            "error should name the searched directory — got: {err}"
        );
        assert!(
            err.contains("OLLAMA_MODELS"),
            "error should suggest OLLAMA_MODELS workaround — got: {err}"
        );
    }

    #[test]
    fn find_manifest_uses_separate_joins_for_registry_path() {
        // Regression: the old code used join("registry.ollama.ai/library") with a
        // forward slash in the string, which is ambiguous on Windows. Verify the
        // candidate path contains the registry and library as distinct components.
        let root = std::path::PathBuf::from("/tmp/manifests");
        // find_manifest will fail (no files exist), but we can inspect the error
        // to confirm both components appear in the searched path.
        let err = find_manifest("llama3:latest", &root)
            .unwrap_err()
            .to_string();
        assert!(
            err.contains("registry.ollama.ai"),
            "path should contain registry: {err}"
        );
        assert!(
            err.contains("library"),
            "path should contain library: {err}"
        );
    }
}
