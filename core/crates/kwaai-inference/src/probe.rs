//! Representation Engineering (RepE) probes and steering vectors for KwaaiNet.
//!
//! Implements the core RepE methodology from:
//!   Zou et al. (2023) "Representation Engineering: A Top-Down Approach to AI Transparency"
//!   https://arxiv.org/abs/2310.01405
//!
//! # Conceptual Overview
//!
//! Large language models store information about their current "cognitive state"
//! (honesty, deception, refusal, emotional tone, etc.) as linear directions in
//! the residual stream — the running sum of activations that flows through every
//! transformer block.  RepE makes this readable:
//!
//! 1. **Direction vectors** are computed offline by contrasting pairs of prompts
//!    that activate opposite poles of a concept (e.g. honest vs. deceptive prompts).
//!    PCA over the difference vectors extracts the principal axis of variation — the
//!    "honesty direction."  See [`RepEProbe`] and the offline pipeline below.
//!
//! 2. **Probes** project the residual stream at a target layer onto a direction
//!    vector, producing a signed scalar score.  Positive = aligned with the positive
//!    pole (e.g. honest); negative = aligned with the negative pole (e.g. deceptive).
//!
//! 3. **Steering vectors** add `α × direction` to the residual stream at inference
//!    time, nudging the model toward a target behaviour without modifying weights.
//!    See [`SteeringVector`].
//!
//! # Architecture Integration
//!
//! ```text
//! Token IDs
//!     │
//!     ▼
//! [Block 0] ──► residual stream ──► [Block 1] ──► ... ──► [Block N]
//!                                        │
//!                                   probe fires here (post-MLP residual add)
//!                                        │
//!                                   ProbeResult { label, score, block_idx }
//!                                        │
//!                                   ShardOutput { tensor, probe_results }
//!                                        │
//!                                   trust_score() / deception_flag()
//! ```
//!
//! The hook fires in `ShardBlock::forward()` *after* the final MLP residual add.
//! This is the correct extraction point per Zou et al.: the post-MLP residual stream
//! captures the most linearly-decodable representation of model state because it
//! accumulates both attention and feed-forward contributions for that block.
//!
//! # Offline Direction Vector Pipeline
//!
//! Direction vectors are NOT computed at inference time — they are precomputed once
//! and loaded from a SafeTensors file.  Generation steps:
//!
//! 1. Collect N contrast pairs: `(positive_prompt, negative_prompt)`
//!    e.g. ("Give me an honest answer", "Deceive me about this")
//!    Use 20–50 pairs minimum; 100+ for production-quality vectors.
//!
//! 2. Run both prompts through the model; extract the post-MLP residual stream
//!    at the target layer (typically layer N/2 for mid-network representations).
//!    Take the last-token hidden state: shape `[hidden_dim]`.
//!
//! 3. Compute difference vectors: `diff_i = act_positive_i - act_negative_i`
//!
//! 4. Stack into matrix `D` of shape `[n_pairs, hidden_dim]`.
//!    Run PCA; the first principal component (PC-1) is the direction vector.
//!    PCA is used rather than mean difference because it is more robust to
//!    prompt-level variation — it finds the axis of *maximum variance* in the
//!    contrast, filtering out noise that affects both poles equally.
//!
//! 5. Unit-normalise PC-1 and save to SafeTensors: key = label, shape = `[hidden_dim]`.
//!
//! Reference Python implementation: https://github.com/andyzoujm/representation-engineering
//! KwaaiNet pipeline script: `scripts/compute_directions.py`
//!
//! # Usage
//!
//! ```rust,ignore
//! use kwaai_inference::probe::{ProbeConfig, ProbeSet, PoolingStrategy};
//!
//! // Define which directions to probe and at which layers
//! let configs = vec![
//!     ProbeConfig { label: "honesty".into(),   probe_layer: 15,
//!                   pooling: PoolingStrategy::LastToken, steer_alpha: None },
//!     ProbeConfig { label: "deception".into(), probe_layer: 15,
//!                   pooling: PoolingStrategy::LastToken, steer_alpha: None },
//! ];
//!
//! // Load precomputed direction vectors from SafeTensors
//! let probes = ProbeSet::from_safetensors("directions.safetensors", &configs, &device)?;
//!
//! // Attach to shard — all forward calls now return ShardOutput with probe_results
//! let shard = TransformerShard::load(...)?.with_probes(probes);
//!
//! // Inspect results after inference
//! let output = shard.forward_full(session_id, &token_ids, seq_pos)?;
//! println!("trust score: {:?}", output.trust_score());
//! println!("deception flag: {}", output.deception_flag(1.5));
//! ```

use candle_core::{DType, Tensor};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use thiserror::Error;

// ── Errors ────────────────────────────────────────────────────────────────────

/// Errors that can occur during probe construction, scoring, or steering.
#[derive(Debug, Error)]
pub enum ProbeError {
    /// Candle tensor operation failed (shape mismatch, device error, etc.)
    #[error("candle tensor error: {0}")]
    Candle(#[from] candle_core::Error),

    /// The direction vector's hidden_dim does not match the hidden state's hidden_dim.
    /// This usually means the direction was computed for a different model architecture.
    #[error("direction vector '{label}' has wrong shape: expected [{expected}], got [{got}]")]
    ShapeMismatch {
        label: String,
        expected: usize,
        got: usize,
    },

    /// Direction vector is not unit-normalised.  Call `normalise()` or ensure the
    /// offline pipeline applies `v / ||v||` before saving.
    #[error(
        "direction vector '{0}' is not unit-normalised (norm = {1:.4}); \
         call normalise() or fix the offline pipeline"
    )]
    NotNormalised(String, f32),

    /// SafeTensors file could not be loaded or a required key was missing.
    #[error("safetensors error: {0}")]
    SafeTensors(String),

    /// Hidden state tensor has an unexpected number of dimensions.
    /// Expected `[1, seq_len, hidden_dim]` (3-D).
    #[error("hidden state has unexpected shape: {0}")]
    UnexpectedShape(String),
}

/// Convenience alias for probe operation results.
pub type ProbeResult_<T> = Result<T, ProbeError>;

// ── ProbeResult ───────────────────────────────────────────────────────────────

/// A single probe score emitted at one transformer block boundary.
///
/// Attached to [`ShardOutput::probe_results`] after every forward pass that
/// has a [`ProbeSet`] configured.  Serialises to JSON so it can be embedded
/// in the KwaaiNet OpenAI-compatible API response under the `transparency` key.
///
/// ## Score interpretation
///
/// The score is the dot product of the (pooled, unit-normalised) hidden state
/// with the direction vector.  Its magnitude reflects how strongly the model's
/// internal state is aligned with the direction's positive pole at this layer.
///
/// Typical range after unit-normalisation: `[-3.0, +3.0]`, though unbounded.
/// Values near zero indicate the model state is orthogonal to the direction —
/// neither aligned nor anti-aligned.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProbeResult {
    /// Human-readable direction label, e.g. `"honesty"`, `"deception"`, `"refusal"`.
    /// Matches the key used in the SafeTensors direction file.
    pub label: String,

    /// Signed projection score.
    /// - Positive → internal state aligned with positive pole (e.g. honest)
    /// - Negative → internal state aligned with negative pole (e.g. deceptive)
    pub score: f32,

    /// Global transformer block index at which this score was extracted.
    /// Corresponds to [`ShardBlock::block_idx`].
    pub block_idx: usize,

    /// Pooling strategy used to reduce `[1, seq_len, hidden_dim]` → `[hidden_dim]`
    /// before computing the dot product.
    pub pooling: PoolingStrategy,
}

// ── PoolingStrategy ───────────────────────────────────────────────────────────

/// How to reduce the sequence dimension of the hidden state before projection.
///
/// ## When to use each
///
/// | Strategy | Best for | Tradeoff |
/// |---|---|---|
/// | `LastToken` | Autoregressive generation (standard) | Sensitive to final token; fast |
/// | `Mean` | Prompt-level classification | More robust to prompt length variation; slightly slower |
/// | `FirstToken` | CLS-style encoders | Rarely correct for decoder-only models |
///
/// **Default is `LastToken`** — this matches the extraction strategy used in the
/// Zou et al. reference implementation and is correct for Llama-family models.
/// Use `Mean` if you find probe scores vary too much with prompt phrasing.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PoolingStrategy {
    /// Average hidden states across all sequence positions.
    /// More robust to prompt-length variation than `LastToken`.
    Mean,
    /// Use only the last token's hidden state.
    /// Standard for autoregressive (decoder-only) models like Llama.
    LastToken,
    /// Use only the first token's hidden state.
    /// Appropriate for CLS-token encoder models; rarely correct for Llama.
    FirstToken,
}

impl Default for PoolingStrategy {
    fn default() -> Self {
        // LastToken is the correct default for Llama-family autoregressive models.
        // The last token has attended to the entire prompt and its hidden state
        // carries the most accumulated context.
        Self::LastToken
    }
}

// ── RepEProbe ──────────────────────────────────────────────────────────────────

/// A precomputed direction vector in residual-stream space.
///
/// Each `RepEProbe` encodes one "concept axis" (e.g. honesty, deception, refusal)
/// as a unit-normalised vector of shape `[hidden_dim]`.  At inference time,
/// [`RepEProbe::score`] projects the hidden state onto this vector via dot product.
///
/// ## Construction
///
/// Prefer loading from a SafeTensors file via [`ProbeSet::from_safetensors`].
/// Direct construction via [`RepEProbe::new`] requires a pre-normalised tensor
/// and will return `Err(ProbeError::NotNormalised)` if the norm deviates from
/// 1.0 by more than 1e-3.
#[derive(Debug)]
pub struct RepEProbe {
    /// Human-readable label matching the SafeTensors key, e.g. `"honesty"`.
    pub label: String,

    /// Unit-normalised direction vector of shape `[hidden_dim]`.
    /// Private to enforce the normalisation invariant — access via `score()`.
    direction: Tensor,

    /// Global transformer block index at which to extract the hidden state.
    /// Should be in the range `[num_total_blocks/3, 2*num_total_blocks/3]` —
    /// mid-network layers have the most linearly-decodable representations
    /// per Zou et al. (2023) Figure 3.
    pub probe_layer: usize,

    /// Pooling strategy for collapsing the sequence dimension.
    pub pooling: PoolingStrategy,
}

impl RepEProbe {
    /// Construct a probe from a pre-normalised direction tensor.
    ///
    /// `direction` must be:
    /// - 1-D with shape `[hidden_dim]`
    /// - Approximately unit-normalised (norm within 1e-3 of 1.0)
    ///
    /// Returns `Err` if either condition is violated.  If your direction vector
    /// came from the offline PCA pipeline, call [`RepEProbe::normalise`] first
    /// if you're unsure whether it was saved as unit-normalised.
    pub fn new(
        label: impl Into<String>,
        direction: Tensor,
        probe_layer: usize,
        pooling: PoolingStrategy,
    ) -> ProbeResult_<Self> {
        let label = label.into();

        // Enforce 1-D shape invariant
        if direction.dims().len() != 1 {
            return Err(ProbeError::UnexpectedShape(format!(
                "direction for '{}' must be 1-D, got {:?}",
                label,
                direction.dims()
            )));
        }

        // Enforce unit-norm invariant.
        // Tolerance of 1e-3 allows for fp16 rounding errors from the offline pipeline
        // without being so loose that meaningfully un-normalised vectors pass through.
        let norm = direction
            .sqr()?
            .sum_all()?
            .to_scalar::<f32>()?
            .sqrt();
        if (norm - 1.0_f32).abs() > 1e-3 {
            return Err(ProbeError::NotNormalised(label, norm));
        }

        Ok(Self { label, direction, probe_layer, pooling })
    }

    /// Project the hidden state onto this direction vector, returning a signed score.
    ///
    /// ## Process
    ///
    /// 1. Pool `hidden` from `[1, seq_len, hidden_dim]` → `[hidden_dim]`
    ///    using `self.pooling` strategy.
    /// 2. Cast both tensors to f32 for stable dot product regardless of model dtype
    ///    (models run in f16/bf16; dot products in f16 lose precision at large hidden_dim).
    /// 3. Compute `score = pooled · direction` (dot product).
    ///
    /// The score is NOT normalised by hidden_dim — the unit-norm direction vector
    /// ensures the score is bounded by the norm of the pooled hidden state, which
    /// is approximately constant across prompts after layer normalisation.
    ///
    /// `block_idx` is the global block index passed through from `ShardBlock::forward()`;
    /// it is attached to the result for downstream filtering without re-lookup.
    pub fn score(&self, hidden: &Tensor, block_idx: usize) -> ProbeResult_<ProbeResult> {
        let dims = hidden.dims();
        if dims.len() != 3 {
            return Err(ProbeError::UnexpectedShape(format!(
                "expected [1, seq_len, hidden_dim], got {:?}", dims
            )));
        }

        let hidden_dim = dims[2];
        let dir_dim = self.direction.dims()[0];

        // Catch architecture mismatches early with a clear error message.
        // Common cause: direction vectors were computed for a different model size
        // (e.g. 8B vs 70B have different hidden_dim).
        if hidden_dim != dir_dim {
            return Err(ProbeError::ShapeMismatch {
                label: self.label.clone(),
                expected: dir_dim,
                got: hidden_dim,
            });
        }

        let seq_len = dims[1];

        // Pool sequence dimension → [1, hidden_dim] then flatten → [hidden_dim]
        let pooled = match self.pooling {
            PoolingStrategy::Mean => {
                // Average over all token positions — more stable than last-token
                // when the prompt length varies significantly across stimuli.
                hidden.mean(1).map_err(ProbeError::Candle)?
            }
            PoolingStrategy::LastToken => {
                // Standard for autoregressive models: the last token has attended
                // to the full context and carries the most accumulated state.
                hidden
                    .narrow(1, seq_len - 1, 1)
                    .map_err(ProbeError::Candle)?
                    .squeeze(1)
                    .map_err(ProbeError::Candle)?
            }
            PoolingStrategy::FirstToken => {
                hidden
                    .narrow(1, 0, 1)
                    .map_err(ProbeError::Candle)?
                    .squeeze(1)
                    .map_err(ProbeError::Candle)?
            }
        };

        // Flatten batch dim → [hidden_dim]
        let flat = pooled.squeeze(0).map_err(ProbeError::Candle)?;

        // Cast to f32 regardless of model dtype.
        // f16 dot products over 4096-dim vectors accumulate ~0.1% relative error
        // vs f32 — acceptable for weights but not for alignment scoring where we
        // care about sign and small magnitude differences.
        let flat_f32 = flat.to_dtype(DType::F32).map_err(ProbeError::Candle)?;
        let dir_f32 = self.direction.to_dtype(DType::F32).map_err(ProbeError::Candle)?;

        // Dot product: score = Σ(flat_i * direction_i)
        let score = flat_f32
            .mul(&dir_f32)
            .map_err(ProbeError::Candle)?
            .sum_all()
            .map_err(ProbeError::Candle)?
            .to_scalar::<f32>()
            .map_err(ProbeError::Candle)?;

        Ok(ProbeResult {
            label: self.label.clone(),
            score,
            block_idx,
            pooling: self.pooling,
        })
    }

    /// Re-normalise `direction` to unit norm and return a new `RepEProbe`.
    ///
    /// Use this if your offline pipeline saves unnormalised direction vectors.
    /// Prefer fixing the pipeline instead — normalisation here runs on every load
    /// rather than once during vector computation.
    pub fn normalise(mut self) -> ProbeResult_<Self> {
        let norm = self.direction
            .sqr()?
            .sum_all()?
            .to_scalar::<f32>()?
            .sqrt();
        // Divide by norm cast to f64 to match candle's scalar arithmetic API
        self.direction = (self.direction / norm as f64)?;
        Ok(self)
    }
}

// ── SteeringVector ────────────────────────────────────────────────────────────

/// Additive activation steering — shifts the residual stream toward a target behaviour.
///
/// Applies `α × direction` to the hidden state at `steer_layer` during inference.
/// This is a zero-weight-modification intervention: the model's parameters are
/// unchanged; only the activations for this specific forward pass are shifted.
///
/// ## Alpha tuning guide
///
/// | α range | Effect |
/// |---|---|
/// | ±1–5 | Subtle nudge; output may be unchanged |
/// | ±5–15 | Visible behavioural shift; recommended starting range |
/// | ±15–25 | Strong intervention; may cause incoherence at high values |
/// | > ±25 | Likely to produce degenerate output |
///
/// Start at `α = 8` and adjust based on output quality.  For alignment enforcement
/// (nudging toward honesty), `α = 8–12` is typically sufficient without degrading
/// coherence.
///
/// ## Ordering with probes
///
/// In `ShardBlock::forward()`, steering is applied *before* probe evaluation.
/// This means probes measure the steered hidden state, not the pre-steering state.
/// If you want to compare pre/post-steering scores, attach two `ProbeSet`s — one
/// with steering and one without — and compare `ShardOutput::probe_results`.
#[derive(Debug)]
pub struct SteeringVector {
    /// Human-readable label matching the direction it was derived from.
    pub label: String,

    /// Precomputed `α × direction` tensor of shape `[hidden_dim]`.
    /// Pre-scaled at construction so application is a single broadcast add —
    /// no per-token multiplication at inference time.
    scaled_direction: Tensor,

    /// Global transformer block index at which to apply steering.
    /// Should match the `probe_layer` of the corresponding [`RepEProbe`]
    /// so that probe scores reflect the steered state.
    pub steer_layer: usize,
}

impl SteeringVector {
    /// Construct from a direction vector and scalar alpha.
    ///
    /// `direction` must be 1-D `[hidden_dim]`.  It does NOT need to be
    /// unit-normalised here — the alpha already encodes the desired magnitude.
    /// However, using a unit-normalised direction makes alpha interpretable
    /// as "how many standard deviations to shift the representation."
    ///
    /// The scaled vector `α × direction` is computed once at construction
    /// so `apply()` is a single broadcast add with no per-call multiplication.
    pub fn new(
        label: impl Into<String>,
        direction: Tensor,
        alpha: f32,
        steer_layer: usize,
    ) -> ProbeResult_<Self> {
        // Pre-scale: store α × direction so apply() is a pure broadcast add
        let scaled = (direction * alpha as f64).map_err(ProbeError::Candle)?;
        Ok(Self {
            label: label.into(),
            scaled_direction: scaled,
            steer_layer,
        })
    }

    /// Add the steering vector to the hidden state via broadcast addition.
    ///
    /// `hidden` shape: `[1, seq_len, hidden_dim]`
    ///
    /// The scaled direction `[hidden_dim]` is reshaped to `[1, 1, hidden_dim]`
    /// and broadcast-added across the batch and sequence dimensions.  Every token
    /// position receives the same shift — this is correct for generation tasks
    /// where we want to shift the model's overall "mode" rather than individual tokens.
    ///
    /// Returns a new tensor; the original `hidden` is not mutated.
    pub fn apply(&self, hidden: &Tensor) -> ProbeResult_<Tensor> {
        // Reshape [hidden_dim] → [1, 1, hidden_dim] for broadcast over [1, seq_len, hidden_dim]
        let steer = self.scaled_direction
            .to_dtype(hidden.dtype()) // match model dtype (f16/bf16/f32)
            .map_err(ProbeError::Candle)?
            .unsqueeze(0)
            .map_err(ProbeError::Candle)?
            .unsqueeze(0)
            .map_err(ProbeError::Candle)?;

        hidden.broadcast_add(&steer).map_err(ProbeError::Candle)
    }
}

// ── ProbeConfig ───────────────────────────────────────────────────────────────

/// Serialisable configuration for a single probe, suitable for TOML/JSON config files.
///
/// Used as input to [`ProbeSet::from_safetensors`] to specify which directions to
/// load and how to configure each probe.  Allows probe configuration to be defined
/// in a config file rather than hardcoded in application code.
///
/// ## Example TOML
///
/// ```toml
/// [[probes]]
/// label = "honesty"
/// probe_layer = 15
/// pooling = "last_token"
///
/// [[probes]]
/// label = "deception"
/// probe_layer = 15
/// pooling = "last_token"
/// steer_alpha = 8.0   # also apply steering at this layer
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProbeConfig {
    /// Label matching a key in the direction vector SafeTensors file.
    pub label: String,

    /// Global transformer block index at which to extract/apply.
    /// Recommended: `num_total_blocks / 2` as a starting point.
    pub probe_layer: usize,

    /// Pooling strategy for sequence dimension reduction.
    /// Defaults to `last_token` — correct for Llama-family models.
    #[serde(default)]
    pub pooling: PoolingStrategy,

    /// If `Some(alpha)`, also register a [`SteeringVector`] at `probe_layer`
    /// with coefficient `alpha`.  Set to `None` for probe-only (no steering).
    pub steer_alpha: Option<f32>,
}

// ── ProbeSet ──────────────────────────────────────────────────────────────────

/// A collection of [`RepEProbe`]s and optional [`SteeringVector`]s, indexed by
/// block index for O(1) lookup during the forward pass.
///
/// This is the primary interface for integrating RepE into [`TransformerShard`].
/// Attach it via [`TransformerShard::with_probes`].
///
/// ## Performance characteristics
///
/// - `has_work_at(block_idx)`: O(1) HashMap lookup
/// - `evaluate(block_idx, hidden)`: O(n_probes_at_block) — typically 1–3 probes
/// - `steer(block_idx, hidden)`: O(n_vectors_at_block) — typically 0–1 vectors
/// - When no probes are registered at a block: zero allocation, near-zero overhead
///
/// The hot path in `ShardBlock::forward()` calls `has_work_at()` first; if it
/// returns false, neither `evaluate` nor `steer` are called.
#[derive(Debug)]
pub struct ProbeSet {
    /// Probes indexed by global block index for O(1) dispatch.
    /// `Vec` per index because multiple directions can probe the same layer
    /// (e.g. honesty and deception both at layer 15).
    probes: HashMap<usize, Vec<RepEProbe>>,

    /// Steering vectors indexed by global block index.
    steering: HashMap<usize, Vec<SteeringVector>>,
}

impl ProbeSet {
    /// Construct an empty `ProbeSet`.
    pub fn new() -> Self {
        Self {
            probes: HashMap::new(),
            steering: HashMap::new(),
        }
    }

    /// Load probes from a SafeTensors file of precomputed direction vectors.
    ///
    /// The SafeTensors file must contain one 1-D float32 tensor per direction,
    /// keyed by label string.  Each tensor must be approximately unit-normalised
    /// (generated by `scripts/compute_directions.py`).
    ///
    /// ## Expected SafeTensors layout
    ///
    /// ```text
    /// "honesty"           → Tensor([4096], F32)  ← PC-1 of honest-deceptive contrasts
    /// "deception"         → Tensor([4096], F32)  ← PC-1 of deceptive-honest contrasts
    /// "refusal"           → Tensor([4096], F32)  ← PC-1 of refusal-compliance contrasts
    /// "authority_framing" → Tensor([4096], F32)  ← PC-1 of authority-neutral contrasts
    /// ```
    ///
    /// `configs` maps each label to its [`ProbeConfig`].  Labels in `configs` that
    /// are not present in the SafeTensors file return `Err(ProbeError::SafeTensors)`.
    #[cfg(feature = "probe-safetensors")]
    pub fn from_safetensors(
        path: impl AsRef<Path>,
        configs: &[ProbeConfig],
        device: &Device,
    ) -> ProbeResult_<Self> {
        use candle_core::safetensors::load;

        let tensors = load(path.as_ref(), device)
            .map_err(|e| ProbeError::SafeTensors(e.to_string()))?;

        let mut set = Self::new();

        for cfg in configs {
            let direction = tensors
                .get(&cfg.label)
                .ok_or_else(|| ProbeError::SafeTensors(format!(
                    "key '{}' not found in SafeTensors file at '{}'",
                    cfg.label,
                    path.as_ref().display()
                )))?
                .clone();

            // Register probe
            let probe = RepEProbe::new(
                cfg.label.clone(),
                direction.clone(),
                cfg.probe_layer,
                cfg.pooling,
            )?;
            set.add_probe(probe);

            // Optionally register steering vector at the same layer
            if let Some(alpha) = cfg.steer_alpha {
                let sv = SteeringVector::new(
                    cfg.label.clone(),
                    direction,
                    alpha,
                    cfg.probe_layer,
                )?;
                set.add_steering(sv);
            }
        }

        Ok(set)
    }

    /// Register a probe.  Multiple probes can be registered at the same block index.
    pub fn add_probe(&mut self, probe: RepEProbe) {
        self.probes
            .entry(probe.probe_layer)
            .or_default()
            .push(probe);
    }

    /// Register a steering vector.
    pub fn add_steering(&mut self, sv: SteeringVector) {
        self.steering
            .entry(sv.steer_layer)
            .or_default()
            .push(sv);
    }

    /// Returns `true` if any probes or steering vectors are registered at `block_idx`.
    ///
    /// Called in `ShardBlock::forward()` before every block.  When this returns
    /// false, neither `evaluate` nor `steer` are called — the RepE path has
    /// effectively zero overhead for blocks with no registered work.
    pub fn has_work_at(&self, block_idx: usize) -> bool {
        self.probes.contains_key(&block_idx)
            || self.steering.contains_key(&block_idx)
    }

    /// Evaluate all probes registered at `block_idx` against `hidden`.
    ///
    /// Returns an empty `Vec` (zero allocation) if no probes are registered at
    /// this block — this is the common case for most blocks in a 32-block model
    /// where only 1–3 layers are probed.
    ///
    /// Called by `ShardBlock::forward()` *after* steering is applied, so scores
    /// reflect the steered hidden state when a `SteeringVector` is also registered
    /// at this block.  This is the correct ordering: measure what the model actually
    /// produces after any intervention, not the pre-intervention state.
    pub fn evaluate(
        &self,
        block_idx: usize,
        hidden: &Tensor,
    ) -> ProbeResult_<Vec<ProbeResult>> {
        let Some(probes) = self.probes.get(&block_idx) else {
            // No probes at this block — return empty vec without allocating
            return Ok(vec![]);
        };

        probes
            .iter()
            .map(|p| p.score(hidden, block_idx))
            .collect()
    }

    /// Apply all steering vectors registered at `block_idx` to `hidden`.
    ///
    /// Returns the modified tensor.  If no steering vectors are registered at this
    /// block, returns a clone of the input unchanged (candle clone is O(1) — it
    /// increments a reference count, no data is copied).
    ///
    /// Called by `ShardBlock::forward()` *before* probe evaluation so that probes
    /// always measure the steered state.  See [`SteeringVector`] ordering note.
    pub fn steer(
        &self,
        block_idx: usize,
        hidden: &Tensor,
    ) -> ProbeResult_<Tensor> {
        let Some(vectors) = self.steering.get(&block_idx) else {
            // No steering at this block — clone is O(1) reference count increment
            return Ok(hidden.clone());
        };

        // Apply steering vectors sequentially.
        // Multiple vectors at the same layer are additive — order doesn't matter
        // because addition is commutative.
        let mut out = hidden.clone();
        for sv in vectors {
            out = sv.apply(&out)?;
        }
        Ok(out)
    }
}

impl Default for ProbeSet {
    fn default() -> Self {
        Self::new()
    }
}

// ── ShardOutput ───────────────────────────────────────────────────────────────

/// Output of a [`TransformerShard`] forward pass, extended with RepE metadata.
///
/// Replaces the bare `Tensor` previously returned by all four `forward_*` methods.
/// When no [`ProbeSet`] is attached to the shard, `probe_results` is always empty
/// and the struct wraps the tensor with zero overhead beyond the allocation of an
/// empty `Vec` (which does not heap-allocate in Rust until elements are pushed).
///
/// ## API response integration
///
/// `ShardOutput` is the source for the `transparency` field in the KwaaiNet
/// OpenAI-compatible API response.  The HTTP handler calls `trust_score()` and
/// `deception_flag()` to populate the response metadata:
///
/// ```json
/// {
///   "choices": [{ "message": { "content": "..." } }],
///   "transparency": {
///     "trust_score": 0.81,
///     "deception_flag": false,
///     "probe_results": [
///       { "label": "honesty", "score": 1.43, "block_idx": 15, "pooling": "last_token" }
///     ]
///   }
/// }
/// ```
#[derive(Debug)]
pub struct ShardOutput {
    /// Hidden states (first/middle nodes) or logits (last node).
    /// Shape: `[1, seq_len, hidden_dim]` or `[1, 1, vocab_size]`.
    pub tensor: Tensor,

    /// RepE probe scores collected across all blocks during this forward pass.
    /// Empty when no [`ProbeSet`] is attached — does not heap-allocate.
    pub probe_results: Vec<ProbeResult>,
}

impl ShardOutput {
    /// Construct with no probe results — the zero-overhead path when no
    /// `ProbeSet` is attached to the shard.
    pub fn plain(tensor: Tensor) -> Self {
        Self { tensor, probe_results: vec![] }
    }

    /// Construct with probe results from a forward pass that had a `ProbeSet` attached.
    pub fn with_probes(tensor: Tensor, probe_results: Vec<ProbeResult>) -> Self {
        Self { tensor, probe_results }
    }

    /// Returns `true` if any deception probe at this forward pass scored above `threshold`.
    ///
    /// Use this in the KwaaiNet trust-gate policy layer to flag responses before delivery.
    /// Recommended threshold: `1.5` — this is approximately one standard deviation above
    /// zero for a unit-normalised direction vector applied to typical Llama-3.1-8B hidden
    /// states.  Tune on your validation set.
    ///
    /// ```rust,ignore
    /// if output.deception_flag(1.5) {
    ///     // Gate or flag response in trust policy layer
    /// }
    /// ```
    pub fn deception_flag(&self, threshold: f32) -> bool {
        self.probe_results
            .iter()
            .any(|r| r.label.contains("deception") && r.score > threshold)
    }

    /// Compute an aggregate trust score in `[0.0, 1.0]` from probe results.
    ///
    /// ## Scoring logic
    ///
    /// - **Honesty probes**: `sigmoid(score)` → high honesty score = high trust
    /// - **Deception probes**: `1 - sigmoid(score)` → high deception score = low trust
    ///
    /// The sigmoid maps the unbounded probe score to `[0, 1]`.  A score of 0.0
    /// maps to `sigmoid(0) = 0.5` (neutral); positive scores push toward 1.0;
    /// negative scores push toward 0.0.
    ///
    /// Deception scores are inverted so that high deception = low trust, which is
    /// the correct semantic: a deception score of +2.0 should produce a trust
    /// contribution near 0.0, not near 1.0.
    ///
    /// The final trust score is the mean across all honesty and deception probe
    /// contributions.  Returns `None` if no honesty or deception probes are present
    /// (e.g. only a refusal probe was configured).
    pub fn trust_score(&self) -> Option<f32> {
        let mut sum = 0.0f32;
        let mut count = 0usize;

        for r in &self.probe_results {
            if r.label.contains("honesty") {
                // High honesty score → high trust: sigmoid maps to (0.5, 1.0) for positive scores
                sum += sigmoid(r.score);
                count += 1;
            } else if r.label.contains("deception") {
                // High deception score → LOW trust: invert so deception +2.0 → trust ~0.12
                sum += 1.0 - sigmoid(r.score);
                count += 1;
            }
            // Other directions (refusal, authority_framing, urgency) are intentionally
            // excluded from the trust score — they are contextual signals, not direct
            // indicators of output trustworthiness.
        }

        if count == 0 {
            None
        } else {
            Some(sum / count as f32)
        }
    }
}

/// Logistic sigmoid function: maps any real number to (0.0, 1.0).
///
/// Used in [`ShardOutput::trust_score`] to convert unbounded probe scores into
/// a probability-like trust contribution.  The sigmoid is preferred over a linear
/// clamp because it is differentiable and handles outlier scores gracefully —
/// a probe score of +10.0 saturates near 1.0 rather than clipping hard.
fn sigmoid(x: f32) -> f32 {
    1.0 / (1.0 + (-x).exp())
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use candle_core::{Device, Tensor};

    fn cpu() -> Device { Device::Cpu }

    /// Build a unit-normalised direction vector pointing along axis 0.
    /// Used across tests as a canonical "honesty direction" with known dot-product behaviour.
    fn unit_direction(dim: usize, device: &Device) -> Tensor {
        let mut data = vec![0.0f32; dim];
        data[0] = 1.0; // unit vector along first axis
        Tensor::from_vec(data, (dim,), device).unwrap()
    }

    #[test]
    fn probe_scores_positive_projection() {
        // A hidden state where every token has activation +2.0 along axis 0
        // should dot-product with the unit direction to give score ≈ 2.0
        let dim = 16usize;
        let dev = cpu();
        let probe = RepEProbe::new("honesty", unit_direction(dim, &dev), 0, PoolingStrategy::Mean).unwrap();

        let mut data = vec![0.0f32; 1 * 4 * dim];
        for t in 0..4 { data[t * dim] = 2.0; }  // axis-0 of each token = 2.0
        let hidden = Tensor::from_vec(data, (1, 4, dim), &dev).unwrap();

        let result = probe.score(&hidden, 0).unwrap();
        // Mean pool → axis-0 = 2.0; dot with unit vec → score = 2.0
        assert!((result.score - 2.0).abs() < 1e-4,
            "expected ~2.0, got {}", result.score);
        assert_eq!(result.label, "honesty");
        assert_eq!(result.block_idx, 0);
    }

    #[test]
    fn probe_scores_last_token_pooling() {
        // Only the last token has a non-zero activation; LastToken pooling should
        // pick it up while Mean would dilute it across 4 tokens.
        let dim = 8usize;
        let dev = cpu();
        let probe = RepEProbe::new("deception", unit_direction(dim, &dev), 3, PoolingStrategy::LastToken).unwrap();

        let mut data = vec![0.0f32; 1 * 4 * dim];
        data[3 * dim] = 5.0; // only last token (index 3), axis-0 = 5.0
        let hidden = Tensor::from_vec(data, (1, 4, dim), &dev).unwrap();

        let result = probe.score(&hidden, 3).unwrap();
        assert!((result.score - 5.0).abs() < 1e-4,
            "expected ~5.0, got {}", result.score);
    }

    #[test]
    fn probe_rejects_non_unit_direction() {
        // A direction vector with norm 2.0 should be rejected to enforce
        // the normalisation invariant.
        let dev = cpu();
        let direction = Tensor::from_vec(vec![2.0f32, 0.0, 0.0], (3,), &dev).unwrap();
        let result = RepEProbe::new("test", direction, 0, PoolingStrategy::Mean);
        assert!(matches!(result, Err(ProbeError::NotNormalised(_, _))),
            "expected NotNormalised error for un-normalised direction");
    }

    #[test]
    fn probe_rejects_shape_mismatch() {
        // Direction dim=8 vs hidden dim=16 should produce a clear error,
        // not a silent wrong result.
        let dim = 8usize;
        let dev = cpu();
        let probe = RepEProbe::new("honesty", unit_direction(dim, &dev), 0, PoolingStrategy::Mean).unwrap();
        let hidden = Tensor::zeros((1, 4, 16), DType::F32, &dev).unwrap();
        let result = probe.score(&hidden, 0);
        assert!(matches!(result, Err(ProbeError::ShapeMismatch { .. })),
            "expected ShapeMismatch error");
    }

    #[test]
    fn steering_vector_applies_correctly() {
        // A steering vector with alpha=3.0 on a unit direction should add 3.0
        // to axis-0 of every token in the hidden state.
        let dim = 4usize;
        let dev = cpu();
        let sv = SteeringVector::new("honesty", unit_direction(dim, &dev), 3.0, 5).unwrap();
        let hidden = Tensor::zeros((1, 2, dim), DType::F32, &dev).unwrap();
        let steered = sv.apply(&hidden).unwrap();

        let vals: Vec<f32> = steered.flatten_all().unwrap().to_vec1().unwrap();
        for t in 0..2 {
            assert!((vals[t * dim] - 3.0).abs() < 1e-5,
                "token {}: axis-0 should be 3.0, got {}", t, vals[t * dim]);
        }
    }

    #[test]
    fn probe_set_evaluate_returns_empty_for_unregistered_block() {
        // Blocks without registered probes must return empty vec — not an error.
        // This is the common case: most blocks in a 32-block model are not probed.
        let set = ProbeSet::new();
        let hidden = Tensor::zeros((1, 4, 8), DType::F32, &cpu()).unwrap();
        let results = set.evaluate(5, &hidden).unwrap();
        assert!(results.is_empty(), "expected empty results for unregistered block");
    }

    #[test]
    fn probe_set_evaluate_fires_at_correct_block() {
        // Probe registered at block 7 should fire only at block 7, not block 6.
        let dim = 8usize;
        let dev = cpu();
        let probe = RepEProbe::new("honesty", unit_direction(dim, &dev), 7, PoolingStrategy::Mean).unwrap();
        let mut set = ProbeSet::new();
        set.add_probe(probe);

        let hidden = Tensor::ones((1, 1, dim), DType::F32, &dev).unwrap();
        let results_at_7 = set.evaluate(7, &hidden).unwrap();
        assert_eq!(results_at_7.len(), 1, "expected 1 result at block 7");
        assert_eq!(results_at_7[0].label, "honesty");

        let results_at_6 = set.evaluate(6, &hidden).unwrap();
        assert!(results_at_6.is_empty(), "expected no results at block 6");
    }

    #[test]
    fn shard_output_deception_flag() {
        // A deception score of 2.5 should raise the flag at threshold 2.0
        // but not at threshold 3.0.
        let dev = cpu();
        let tensor = Tensor::zeros((1, 1, 4), DType::F32, &dev).unwrap();
        let results = vec![ProbeResult {
            label: "deception".into(), score: 2.5,
            block_idx: 15, pooling: PoolingStrategy::LastToken,
        }];
        let output = ShardOutput::with_probes(tensor, results);
        assert!(output.deception_flag(2.0), "should flag at threshold 2.0");
        assert!(!output.deception_flag(3.0), "should not flag at threshold 3.0");
    }

    #[test]
    fn shard_output_trust_score_honesty() {
        // A honesty score of 0.0 → sigmoid(0.0) = 0.5 → trust score = 0.5
        // This validates the sigmoid mapping and averaging logic.
        let dev = cpu();
        let tensor = Tensor::zeros((1, 1, 4), DType::F32, &dev).unwrap();
        let results = vec![ProbeResult {
            label: "honesty".into(), score: 0.0,
            block_idx: 15, pooling: PoolingStrategy::LastToken,
        }];
        let output = ShardOutput::with_probes(tensor, results);
        let ts = output.trust_score().unwrap();
        assert!((ts - 0.5).abs() < 1e-5,
            "expected trust score 0.5 for honesty score 0.0, got {}", ts);
    }
}
