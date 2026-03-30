//! llama.cpp local inference backend for Apple Silicon.
//!
//! Uses llama-cpp-2 Rust bindings with Metal GPU acceleration for
//! full-model inference at 50+ tok/s on M-series Macs.
//!
//! Feature-gated: only compiled with `--features llama-cpp`.

#[cfg(feature = "llama-cpp")]
use anyhow::{bail, Context, Result};

#[cfg(feature = "llama-cpp")]
use llama_cpp_2::{
    context::params::LlamaContextParams,
    llama_backend::LlamaBackend,
    llama_batch::LlamaBatch,
    model::{params::LlamaModelParams, LlamaModel},
    token::LlamaToken,
};

#[cfg(feature = "llama-cpp")]
use std::io::Write as _;
#[cfg(feature = "llama-cpp")]
use std::path::Path;

#[cfg(feature = "llama-cpp")]
pub struct GenerationResult {
    pub text: String,
    pub tokens_generated: usize,
    pub prefill_ms: f64,
    pub decode_ms: f64,
}

/// Run local inference via llama.cpp with Metal acceleration.
///
/// Loads a GGUF model, tokenizes the prompt, and generates tokens
/// using llama.cpp's optimized Metal pipeline.
#[cfg(feature = "llama-cpp")]
pub fn run_inference(
    model_path: &Path,
    prompt: &str,
    max_tokens: usize,
    temperature: f32,
) -> Result<GenerationResult> {
    use crate::display::*;

    // Initialize backend
    let backend = LlamaBackend::init().context("Failed to init llama.cpp backend")?;

    // Load model with GPU layers
    let model_params = LlamaModelParams::default();
    // Metal is auto-enabled on macOS when llama.cpp is built with Metal support

    print_info("Loading GGUF model via llama.cpp...");
    let model = LlamaModel::load_from_file(&backend, model_path, &model_params)
        .map_err(|e| anyhow::anyhow!("Failed to load model: {e:?}"))?;

    // Create context
    let ctx_params = LlamaContextParams::default()
        .with_n_ctx(std::num::NonZeroU32::new(2048));
    let mut ctx = model
        .new_context(&backend, ctx_params)
        .map_err(|e| anyhow::anyhow!("Failed to create context: {e:?}"))?;

    // Tokenize prompt
    let tokens = model
        .str_to_token(prompt, llama_cpp_2::model::AddBos::Always)
        .map_err(|e| anyhow::anyhow!("Tokenize failed: {e:?}"))?;

    let n_prompt = tokens.len();
    let eos = model.token_eos();

    // Prefill: process all prompt tokens
    let prefill_start = std::time::Instant::now();
    let mut batch = LlamaBatch::new(n_prompt.max(1), 1);
    for (i, &tok) in tokens.iter().enumerate() {
        let is_last = i == n_prompt - 1;
        batch
            .add(tok, i as i32, &[0], is_last)
            .map_err(|e| anyhow::anyhow!("Batch add failed: {e:?}"))?;
    }
    ctx.decode(&mut batch)
        .map_err(|e| anyhow::anyhow!("Prefill decode failed: {e:?}"))?;
    let prefill_ms = prefill_start.elapsed().as_secs_f64() * 1000.0;

    // Decode loop
    let decode_start = std::time::Instant::now();
    let mut generated = Vec::new();
    let mut pos = n_prompt as i32;

    for _ in 0..max_tokens {
        // Sample from logits
        let logits = ctx.get_logits_ith((batch.n_tokens() - 1) as i32);

        // Simple temperature sampling
        let next_token = if temperature <= 0.0 || temperature == 1.0 {
            // Greedy: argmax
            logits
                .iter()
                .enumerate()
                .max_by(|a, b| a.1.partial_cmp(b.1).unwrap_or(std::cmp::Ordering::Equal))
                .map(|(i, _)| LlamaToken::new(i as i32))
                .unwrap_or(eos)
        } else {
            // Temperature sampling (simplified — use logit scaling)
            let scaled: Vec<f32> = logits.iter().map(|&l| l / temperature).collect();
            let max_val = scaled.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
            let exp: Vec<f32> = scaled.iter().map(|&l| (l - max_val).exp()).collect();
            let sum: f32 = exp.iter().sum();
            let probs: Vec<f32> = exp.iter().map(|&e| e / sum).collect();

            // Random sample
            let r: f32 = {
                use std::time::SystemTime;
                let nanos = SystemTime::now()
                    .duration_since(SystemTime::UNIX_EPOCH)
                    .unwrap()
                    .subsec_nanos();
                (nanos as f32) / (u32::MAX as f32)
            };
            let mut cumsum = 0.0f32;
            let mut picked = 0;
            for (i, &p) in probs.iter().enumerate() {
                cumsum += p;
                if cumsum >= r {
                    picked = i;
                    break;
                }
            }
            LlamaToken::new(picked as i32)
        };

        // Check EOS
        if model.is_eog_token(next_token) {
            break;
        }

        // Decode token to text
        if let Ok(piece) = model.token_to_str(next_token, #[allow(deprecated)] llama_cpp_2::model::Special::Tokenize) {
            print!("{piece}");
            std::io::stdout().flush().ok();
        }

        generated.push(next_token);

        // Prepare next batch (single token)
        batch.clear();
        batch
            .add(next_token, pos, &[0], true)
            .map_err(|e| anyhow::anyhow!("Batch add failed: {e:?}"))?;
        pos += 1;

        ctx.decode(&mut batch)
            .map_err(|e| anyhow::anyhow!("Decode failed: {e:?}"))?;
    }

    let decode_ms = decode_start.elapsed().as_secs_f64() * 1000.0;

    // Collect full text
    let mut text = String::new();
    for tok in &generated {
        if let Ok(piece) = model.token_to_str(*tok, #[allow(deprecated)] llama_cpp_2::model::Special::Tokenize) {
            text.push_str(&piece);
        }
    }

    Ok(GenerationResult {
        text,
        tokens_generated: generated.len(),
        prefill_ms,
        decode_ms,
    })
}

/// Check if llama.cpp backend is available (feature compiled in).
pub fn is_available() -> bool {
    cfg!(feature = "llama-cpp")
}

#[cfg(all(test, feature = "llama-cpp"))]
mod tests {
    use super::*;

    #[test]
    fn test_llama_local_inference() {
        let model_path = dirs::home_dir()
            .expect("home")
            .join(".kwaainet/models/llama-3.1-8b-instruct-q4_k_m.gguf");

        if !model_path.exists() {
            eprintln!("[SKIP] test_llama_local_inference — GGUF model not found");
            return;
        }

        eprintln!("  Loading model via llama.cpp...");
        let t0 = std::time::Instant::now();
        let result = run_inference(&model_path, "The capital of France is", 20, 0.0)
            .expect("inference failed");
        let total = t0.elapsed().as_secs_f64();

        let tps = if result.decode_ms > 0.0 {
            result.tokens_generated as f64 / (result.decode_ms / 1000.0)
        } else {
            0.0
        };

        eprintln!();
        eprintln!("  ── llama.cpp Results ──────────────────────────────");
        eprintln!("  Text:      {:?}", result.text.trim());
        eprintln!("  Tokens:    {}", result.tokens_generated);
        eprintln!("  Prefill:   {:.0}ms", result.prefill_ms);
        eprintln!("  Decode:    {:.0}ms ({:.1} tok/s)", result.decode_ms, tps);
        eprintln!("  Total:     {:.1}s", total);
        eprintln!("  ──────────────────────────────────────────────────");
        eprintln!("[OK] test_llama_local_inference");

        assert!(result.tokens_generated > 0, "should generate at least 1 token");
        assert!(result.text.to_lowercase().contains("paris"), "should mention Paris");
    }
}
