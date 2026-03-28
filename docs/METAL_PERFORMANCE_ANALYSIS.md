# Metal GPU Performance Analysis

## The Problem

TransformerShard on Apple Silicon Metal gets **0.0 tok/s decode** (130s per token) while CPU gets **4.8 tok/s** (0.2s per token). Metal is 650x slower for decode. Prefill is the opposite: Metal is 23x faster (582 tok/s vs 25 tok/s).

The Python Petals version (OpenAI-Petal) achieved reasonable performance on MPS/Metal through PyTorch's mature Metal backend.

## Root Cause: Kernel Launch Overhead Dominates Tiny Compute

### Single-token decode math

For a single decode token through one transformer block (Llama 8B, hidden_dim=4096):

| Operation | Matrix size | FLOPs | Metal compute time |
|-----------|-----------|-------|-------------------|
| q/k/v/o projections | [1,4096] × [4096,4096] | 16M each | ~1.6µs each |
| Attention Q×K^T | [1,32,1,128] × [1,32,128,N] | 256K | ~0.03µs |
| gate/up/down MLP | [1,4096] × [4096,11008] | 44M each | ~4.4µs each |

**Total compute per block: ~70µs** at M1's 10 TFLOPS peak.

But each operation is a separate Metal kernel launch:

| Cost | Time |
|------|------|
| Kernel launch overhead | 50–200µs per op |
| 23 operations per block | ~2.3ms dispatch overhead |
| 32 blocks per model | ~74ms dispatch overhead |
| Actual compute | ~2.2ms |
| **GPU utilization** | **~3%** |

For prefill (seq_len=512), the same kernel launches amortize over 512x more compute, so Metal shines. For decode (seq_len=1), overhead dwarfs compute.

### Measured vs expected

| Metric | Expected | Measured | Explanation |
|--------|----------|---------|-------------|
| Metal decode | ~75ms/tok (dispatch-bound) | 130,000ms/tok | 1700x worse than dispatch theory |
| CPU decode | ~200ms/tok | 200ms/tok | Matches expectation |

The 1700x gap between expected dispatch overhead (75ms) and measured (130s) suggests something beyond simple launch overhead — likely candle 0.8.4's Metal backend has synchronization issues, or `to_device(CPU)` forces a full pipeline stall per token.

## Comparison: OpenAI-Petal (PyTorch) vs KwaaiNet (Candle)

| Aspect | OpenAI-Petal (PyTorch/Petals) | KwaaiNet (Candle 0.8.4) |
|--------|-------------------------------|-------------------------|
| **Attention** | `torch.nn.functional.scaled_dot_product_attention()` — has hand-tuned Metal kernels, fuses Q×K^T + scale + mask + softmax + Attn×V into one kernel | Manual: 6 separate ops (matmul, scale, mask, broadcast_add, softmax, matmul) = 6 kernel launches |
| **RoPE** | Framework-optimized, single fused kernel | Manual: 4× `broadcast_mul` + `unsqueeze` = 8+ kernel launches |
| **KV-cache** | Native in-place append in PyTorch | `Tensor::cat()` — allocates new buffer + full copy every token |
| **Kernel fusion** | PyTorch's Metal backend fuses compatible ops automatically | No fusion — each candle op is a standalone kernel |
| **MPS maturity** | 3+ years of Apple + PyTorch collaboration | Candle Metal: community-contributed, limited optimization |
| **BFloat16** | Patched to F16 on MPS (explicit compatibility layer) | Uses F16 directly but no MPS-specific adaptations |
| **Generation loop** | `model.generate()` — framework handles batching internally | Manual token-by-token loop with GPU→CPU transfer per token |

### Key takeaway

PyTorch's `scaled_dot_product_attention` replaces ~6 separate kernel calls with 1 fused kernel. Over 32 blocks × 200 tokens = 6,400 kernel launches saved. Combined with KV-cache optimizations and automatic kernel fusion, PyTorch's Metal path is fundamentally more efficient for autoregressive decode.

## Diagnosis Steps (Pre-requisite)

Before optimizing, we need to measure where the 130s/tok actually goes:

### 1. Instrument forward_full timing

```rust
// In shard.rs forward_full(), around line 670
let fwd_start = std::time::Instant::now();
let x = self.run_blocks(hidden, seq_pos, session_id)?;
eprintln!("run_blocks: {:.1}ms", fwd_start.elapsed().as_secs_f64() * 1000.0);
```

### 2. Instrument per-block timing

```rust
// In shard.rs run_blocks(), around line 586
for (local_idx, block) in self.blocks.iter().enumerate() {
    let block_start = std::time::Instant::now();
    x = block.forward(&x, seq_pos, &mut session.kv[local_idx], &self.rope)?;
    eprintln!("  block {}: {:.1}ms", local_idx, block_start.elapsed().as_secs_f64() * 1000.0);
}
```

### 3. Instrument GPU→CPU transfer

```rust
// In shard_cmd.rs or benchmark, after forward_full returns
let transfer_start = std::time::Instant::now();
let logits_cpu = logits.to_device(&candle_core::Device::Cpu)?;
eprintln!("GPU→CPU: {:.1}ms", transfer_start.elapsed().as_secs_f64() * 1000.0);
```

If `run_blocks` takes 130s → problem is in the Metal forward pass (kernel launches or sync).
If `to_device(CPU)` takes 130s → problem is GPU pipeline stall on transfer.

## Optimization Plan (Tiered)

### Tier 0: Quick diagnostic (1 day)

Add the instrumentation above. Run benchmark. Determine where the 130s goes. This decides which tiers to pursue.

### Tier 1: Eliminate GPU→CPU round-trip for sampling (1 day)

Currently `to_device(CPU)` is called after every `forward_full()` to sample the next token on CPU. Instead:

- **Argmax on GPU**: candle supports `argmax()` on Metal — do greedy decode without CPU transfer.
- **Move sampling to GPU**: `softmax → multinomial` on GPU, only transfer the single `u32` token ID back.
- Expected speedup: eliminates pipeline stall per token. If this is the bottleneck, could be **1000x**.

### Tier 2: CPU decode fallback with Metal prefill (1 day)

Hybrid device strategy matching the workload:
- **Prefill on Metal**: large batch, Metal is 23x faster.
- **Decode on CPU**: seq_len=1, CPU avoids kernel launch overhead.
- Transfer hidden states GPU→CPU once after prefill, then decode entirely on CPU.
- Expected speedup: ~4.8 tok/s decode (CPU speed) with ~582 tok/s prefill (Metal speed).

### Tier 3: Reduce kernel launches via operation fusion (1 week)

Fuse related operations into single Metal kernels:

1. **Fused QKV projection**: one matmul for Q, K, V instead of three separate ones. Save 2 kernel launches per block.
2. **Fused RoPE**: combine unsqueeze + broadcast_mul + sub/add into one custom Metal kernel. Save 6+ launches per block.
3. **Fused attention**: combine Q×K^T + scale + softmax + Attn×V into one kernel (equivalent to PyTorch's `scaled_dot_product_attention`). Save 4 launches per block.
4. **In-place KV-cache**: pre-allocate KV-cache buffer at max_seq_len, write new K/V at the current position instead of `Tensor::cat()`. Eliminates allocation + copy per token.

Total per block: reduce from ~23 launches to ~8. Over 32 blocks: 256 → 96 kernel launches.

### Tier 4: Upgrade candle or use Metal Performance Shaders directly (1-2 weeks)

- **Upgrade candle** to latest version — check if newer candle-core has Metal decode optimizations.
- **Custom MPS kernels**: write Metal shading language kernels for the hot path (attention, RoPE) and call them directly via candle's custom kernel API.
- **Evaluate MLX**: Apple's MLX framework (Rust bindings: `mlx-rs`) is specifically designed for Apple Silicon ML workloads and may have superior Metal performance for transformer inference.

### Tier 5: Consider MLX backend (2-4 weeks)

Apple's MLX framework is purpose-built for Apple Silicon:
- Unified memory (no GPU↔CPU copies)
- Lazy evaluation with automatic kernel fusion
- Optimized transformer primitives
- ~30 tok/s on Llama 8B (M2 Pro) vs our 0.0 tok/s on Metal

Could add MLX as an alternative backend behind the `DeviceType` enum, selected when `--device mlx` or auto-detected on Apple Silicon.

## Files

| File | Relevance |
|------|-----------|
| `kwaai-inference/src/shard.rs:239-358` | ShardBlock::forward — 23 kernel launches per block |
| `kwaai-inference/src/shard.rs:286-292` | KV-cache cat — allocation + copy per token |
| `kwaai-inference/src/shard.rs:98-116` | RoPE — 8+ kernel launches via broadcast_mul |
| `kwaai-inference/src/shard.rs:574-590` | run_blocks — session mutex + block loop |
| `kwaai-cli/src/shard_cmd.rs:732` | GPU→CPU transfer per token |
| `kwaai-cli/src/main.rs:929-1095` | Benchmark — measures the problem |
| `core/Cargo.lock` | candle-core 0.8.4 — consider upgrade |
