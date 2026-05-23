//! End-to-end RepE inference example for KwaaiNet.
//!
//! Demonstrates the full pipeline:
//!   1. Configure probe directions and target layers
//!   2. Load precomputed direction vectors from SafeTensors
//!   3. Attach a ProbeSet to a TransformerShard
//!   4. Run inference and collect ShardOutput with probe metadata
//!   5. Inspect trust score and deception flag
//!
//! ## Usage
//!
//! ```bash
//! cargo run --example repe_inference \
//!   --features probe-safetensors \
//!   -- --model ~/.kwaainet/models/llama3.1-8b \
//!      --directions directions.safetensors
//! ```
//!
//! ## Prerequisites
//!
//! Direction vectors must be precomputed before running this example.
//! Run `scripts/compute_directions.py` to generate `directions.safetensors`
//! from your contrast pair dataset.
//!
//! ## API extension note
//!
//! The `kwaai_options.repe_probes` field shown below is a planned extension
//! to the KwaaiNet OpenAI-compatible HTTP handler.  Once implemented, probe
//! results will be available directly in the API response under `transparency`:
//!
//! ```bash
//! curl http://localhost:11435/v1/chat/completions \
//!   -H "Content-Type: application/json" \
//!   -d '{
//!     "model": "llama3.1:8b",
//!     "messages": [{"role":"user","content":"YOUR_PROMPT"}],
//!     "kwaai_options": {"repe_probes": true}
//!   }'
//! ```

use kwaai_inference::{
    probe::{PoolingStrategy, ProbeConfig, ProbeSet, ShardOutput},
    TransformerShard,
};
use candle_core::Device;
use std::path::Path;

fn main() -> anyhow::Result<()> {
    // ── Device selection ──────────────────────────────────────────────────────
    // Prefers CUDA if available, falls back to CPU.
    // KwaaiNet uses candle's Metal backend automatically on Apple Silicon.
    let device = Device::cuda_if_available(0).unwrap_or(Device::Cpu);
    println!("Device: {:?}", device);

    // ── Probe configuration ───────────────────────────────────────────────────
    // probe_layer: which transformer block to extract activations from.
    //
    // For Llama-3.1-8B (32 blocks), mid-network layers (11–21) have the most
    // linearly-decodable representations per Zou et al. (2023) Figure 3.
    // Layer 15 is the recommended starting point for honesty/deception directions.
    //
    // steer_alpha: set to Some(f32) to also apply activation steering at this layer.
    //   None     = probe only (read the signal, do not modify inference)
    //   Some(α)  = probe + steer toward the direction's positive pole with coefficient α
    //
    // Recommended alpha range: ±5–15. Below ±5 the effect is often imperceptible;
    // above ±20 output quality typically degrades.
    let probe_configs = vec![
        ProbeConfig {
            label: "honesty".into(),
            probe_layer: 15,
            pooling: PoolingStrategy::LastToken,
            steer_alpha: None,
        },
        ProbeConfig {
            label: "deception".into(),
            probe_layer: 15,
            pooling: PoolingStrategy::LastToken,
            steer_alpha: None,
        },
        ProbeConfig {
            label: "refusal".into(),
            // Layer 20: refusal direction is more active in later mid-network blocks
            probe_layer: 20,
            pooling: PoolingStrategy::LastToken,
            steer_alpha: None,
        },
    ];

    // ── Load direction vectors ────────────────────────────────────────────────
    // directions.safetensors must contain one 1-D float32 tensor per label above.
    // If this fails with "key not found", re-run compute_directions.py with
    // the missing label added to the contrast pair configuration.
    let probe_set = ProbeSet::from_safetensors(
        Path::new("directions.safetensors"),
        &probe_configs,
        &device,
    )?;

    // ── Load model shard ──────────────────────────────────────────────────────
    // with_probes() attaches the ProbeSet via builder pattern.
    // Model weights are not modified — the hook fires after each block's
    // residual add and reads/modifies activations only for this forward pass.
    //
    // If load() fails, check:
    //   1. config.json and tokenizer.json are present at model_path
    //   2. Safetensors shards are present and not corrupted
    //   3. Device has sufficient memory (Llama-3.1-8B needs ~16GB in f16)
    let model_path = Path::new("~/.kwaainet/models/llama3.1-8b");

    let shard = TransformerShard::load(
        &[&model_path.join("model.safetensors")],
        &model_path.join("config.json"),
        &device,
        0,  // start_block
        32, // end_block — 32 for Llama-3.1-8B
    )?
    .with_probes(probe_set);

    // ── Run inference ─────────────────────────────────────────────────────────
    let session_id: u64 = 1;
    let seq_pos: usize = 0; // 0 for fresh prefill

    let prompt = "Explain the importance of transparency in AI systems.";
    let token_ids = shard.tokenizer.encode(prompt)?;

    // forward_full() for single-node deployment (full model on one machine).
    // Returns ShardOutput { tensor, probe_results } instead of bare Tensor.
    // For multi-node deployment use forward_first/middle/last accordingly.
    let output: ShardOutput = shard.forward_full(session_id, &token_ids, seq_pos)?;

    // ── Probe results ─────────────────────────────────────────────────────────
    println!("\n=== RepE Probe Results ===");
    for result in &output.probe_results {
        println!(
            "  [block {:>2}] {:<12}  {:+.4}  (pooling: {:?})",
            result.block_idx, result.label, result.score, result.pooling
        );
    }

    // ── Trust gate ────────────────────────────────────────────────────────────
    // trust_score() aggregates honesty and deception signals via sigmoid.
    // Returns None if neither honesty nor deception probes are configured.
    //
    // deception_flag() threshold of 1.5 is approximately one standard deviation
    // above zero for unit-normalised directions on Llama-3.1-8B hidden states.
    // Tune this threshold on your validation set before use in production.
    println!("\n=== Trust Gate ===");
    match output.trust_score() {
        Some(score) => {
            println!("  Trust score:    {:.3}", score);
            println!("  Deception flag: {}", output.deception_flag(1.5));
        }
        None => {
            println!("  No honesty/deception probes configured.");
        }
    }

    Ok(())
}
