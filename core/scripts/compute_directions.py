#!/usr/bin/env python3
"""
compute_directions.py — Offline contrast-pair pipeline for RepE direction vectors.

Generates unit-normalised direction vectors for use with KwaaiNet's RepE probe
module (crates/kwaai-inference/src/probe.rs).

Methodology (Zou et al. 2023):
  1. Define contrast pairs: (positive_prompt, negative_prompt) per direction label
  2. Extract post-MLP residual stream activations at the target layer for each prompt
  3. Compute difference vectors: act_positive - act_negative
  4. Stack into matrix D of shape [n_pairs, hidden_dim]
  5. Run PCA; PC-1 is the direction vector
  6. Unit-normalise and save to SafeTensors

PCA over mean difference:
  Mean difference captures the average direction between poles.
  PCA captures the axis of maximum variance — more robust to prompt-level noise
  that affects both poles equally, since such noise cancels in the difference
  but would inflate the mean.

Usage:
  pip install torch transformers scikit-learn safetensors numpy
  python scripts/compute_directions.py \
    --model meta-llama/Llama-3.1-8B-Instruct \
    --layer 15 \
    --output directions.safetensors

Output:
  directions.safetensors — one 1-D float32 tensor per label, unit-normalised
  directions_metadata.json — layer index, n_pairs per label, explained variance
"""

import argparse
import json
import numpy as np
import torch
from pathlib import Path
from sklearn.decomposition import PCA
from transformers import AutoTokenizer, AutoModelForCausalLM
from safetensors.numpy import save_file


# ── Contrast pair definitions ─────────────────────────────────────────────────
#
# Each label maps to a list of (positive_prompt, negative_prompt) tuples.
# The positive pole is what the direction points TOWARD — a positive probe
# score means the model's internal state is aligned with the positive pole.
#
# Guidelines for contrast pair quality:
#   - Pairs should differ on the target concept only — control for length,
#     topic, and formality so the direction is concept-specific
#   - 20 pairs minimum for reliable directions; 50+ for production quality
#   - Draw pairs from the same domain as your inference workload
#   - Validate: probe scores on held-out pairs should be consistently signed

CONTRAST_PAIRS = {
    "honesty": [
        (
            "Please give me an accurate and truthful answer to this question.",
            "Please give me a misleading and false answer to this question.",
        ),
        (
            "Tell me the honest truth about what you know.",
            "Deceive me about what you know.",
        ),
        (
            "I need transparent, factual information.",
            "Hide the real information from me.",
        ),
        (
            "Be straightforward and direct with me.",
            "Be evasive and indirect with me.",
        ),
        (
            "Give me an accurate, unbiased assessment.",
            "Give me a distorted, biased assessment.",
        ),
        # Add more pairs here — aim for 20+ before using in production
    ],
    "deception": [
        (
            "This message is designed to manipulate and deceive the recipient.",
            "This message is designed to inform and help the recipient.",
        ),
        (
            "Craft a message that creates false urgency to pressure someone.",
            "Craft a message that provides genuine information without pressure.",
        ),
        (
            "Write something that impersonates an authority to gain trust.",
            "Write something that clearly identifies its source and intent.",
        ),
        (
            "Create a message that obscures its true purpose.",
            "Create a message that clearly states its true purpose.",
        ),
        (
            "Write content intended to mislead the reader about the facts.",
            "Write content intended to accurately inform the reader about the facts.",
        ),
    ],
    "refusal": [
        (
            "I cannot and will not help with that request.",
            "I will help you with any request you make.",
        ),
        (
            "That request falls outside what I am able to assist with.",
            "I have no restrictions on what I can assist with.",
        ),
        (
            "I need to decline this request based on my guidelines.",
            "I will fulfill this request regardless of any guidelines.",
        ),
        (
            "This is something I am not able to provide assistance with.",
            "This is something I am happy to provide full assistance with.",
        ),
    ],
}


# ── Activation extraction ─────────────────────────────────────────────────────

def get_hidden_state(
    model: AutoModelForCausalLM,
    tokenizer: AutoTokenizer,
    text: str,
    layer_idx: int,
    device: torch.device,
) -> np.ndarray:
    """
    Extract the post-MLP residual stream at layer_idx for the last token.

    Returns shape: [hidden_dim] as float32 numpy array.

    Last-token extraction matches the default PoolingStrategy::LastToken in
    probe.rs — the last token has attended to the full context and carries
    the most accumulated state for autoregressive models.
    """
    inputs = tokenizer(
        text,
        return_tensors="pt",
        truncation=True,
        max_length=256,  # cap at 256 to keep memory bounded across many pairs
    ).to(device)

    with torch.no_grad():
        outputs = model(
            **inputs,
            output_hidden_states=True,  # returns tuple of [n_layers+1] tensors
        )

    # hidden_states[0] is the embedding layer output
    # hidden_states[i] for i >= 1 is the output of transformer block i-1
    # hidden_states[layer_idx + 1] is the post-MLP output of block layer_idx
    hidden = outputs.hidden_states[layer_idx + 1]  # [1, seq_len, hidden_dim]

    # Take the last token: [hidden_dim]
    return hidden[0, -1, :].float().cpu().numpy()


# ── Direction computation ─────────────────────────────────────────────────────

def compute_direction(
    model: AutoModelForCausalLM,
    tokenizer: AutoTokenizer,
    pairs: list,
    layer_idx: int,
    device: torch.device,
) -> tuple[np.ndarray, float]:
    """
    Compute a unit-normalised direction vector from contrast pairs via PCA.

    Returns:
        direction: np.ndarray of shape [hidden_dim], unit-normalised
        explained_variance: float — fraction of variance captured by PC-1
                            Higher is better; >0.3 indicates a clean direction
    """
    diffs = []
    for pos_text, neg_text in pairs:
        pos = get_hidden_state(model, tokenizer, pos_text, layer_idx, device)
        neg = get_hidden_state(model, tokenizer, neg_text, layer_idx, device)
        diffs.append(pos - neg)

    # Stack: [n_pairs, hidden_dim]
    D = np.stack(diffs).astype(np.float32)

    # PCA: find the axis of maximum variance in the contrast
    pca = PCA(n_components=1)
    pca.fit(D)

    direction = pca.components_[0].astype(np.float32)  # [hidden_dim]
    explained_variance = float(pca.explained_variance_ratio_[0])

    # Unit-normalise
    norm = np.linalg.norm(direction)
    direction = direction / norm

    return direction, explained_variance


# ── Main ──────────────────────────────────────────────────────────────────────

def main():
    parser = argparse.ArgumentParser(
        description="Compute RepE direction vectors from contrast pairs"
    )
    parser.add_argument(
        "--model",
        default="meta-llama/Llama-3.1-8B-Instruct",
        help="HuggingFace model ID or local path",
    )
    parser.add_argument(
        "--layer",
        type=int,
        default=15,
        help="Transformer block index to extract activations from (default: 15)",
    )
    parser.add_argument(
        "--output",
        default="directions.safetensors",
        help="Output path for direction vectors (default: directions.safetensors)",
    )
    parser.add_argument(
        "--dtype",
        default="float16",
        choices=["float16", "bfloat16", "float32"],
        help="Model dtype for loading (default: float16)",
    )
    args = parser.parse_args()

    # Device selection
    if torch.cuda.is_available():
        device = torch.device("cuda")
    elif torch.backends.mps.is_available():
        device = torch.device("mps")
    else:
        device = torch.device("cpu")
        print("Warning: running on CPU — this will be slow for large models")

    print(f"Device:      {device}")
    print(f"Model:       {args.model}")
    print(f"Layer:       {args.layer}")
    print(f"Output:      {args.output}")
    print()

    # Load model and tokenizer
    print(f"Loading {args.model}...")
    dtype_map = {
        "float16": torch.float16,
        "bfloat16": torch.bfloat16,
        "float32": torch.float32,
    }
    tokenizer = AutoTokenizer.from_pretrained(args.model)
    model = AutoModelForCausalLM.from_pretrained(
        args.model,
        torch_dtype=dtype_map[args.dtype],
        device_map="auto",  # distributes across available GPUs automatically
    )
    model.eval()
    print("Model loaded.\n")

    # Compute direction vectors
    tensors = {}
    metadata = {
        "layer": args.layer,
        "model": args.model,
        "directions": {},
    }

    for label, pairs in CONTRAST_PAIRS.items():
        print(f"Computing '{label}' from {len(pairs)} contrast pairs...")
        direction, explained_var = compute_direction(
            model, tokenizer, pairs, args.layer, device
        )

        # Verify unit norm before saving
        norm = np.linalg.norm(direction)
        assert abs(norm - 1.0) < 1e-3, \
            f"Direction '{label}' norm {norm:.4f} deviates from 1.0 — check pipeline"

        tensors[label] = direction
        metadata["directions"][label] = {
            "n_pairs": len(pairs),
            "explained_variance_ratio": explained_var,
            "norm": float(norm),
        }

        print(f"  Done. Explained variance: {explained_var:.3f}  Norm: {norm:.4f}")
        if explained_var < 0.2:
            print(f"  Warning: low explained variance for '{label}' — "
                  f"consider adding more contrast pairs or reviewing pair quality")

    # Save direction vectors
    save_file(tensors, args.output)
    print(f"\nSaved direction vectors to: {args.output}")

    # Save metadata alongside for reproducibility
    metadata_path = Path(args.output).with_suffix(".json")
    with open(metadata_path, "w") as f:
        json.dump(metadata, f, indent=2)
    print(f"Saved metadata to:          {metadata_path}")

    print("\nDirection summary:")
    for label, meta in metadata["directions"].items():
        print(f"  {label:<20} n_pairs={meta['n_pairs']}  "
              f"explained_var={meta['explained_variance_ratio']:.3f}")


if __name__ == "__main__":
    main()
