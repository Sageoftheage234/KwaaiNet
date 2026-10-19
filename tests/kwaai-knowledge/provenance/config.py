from __future__ import annotations

from dataclasses import asdict, dataclass, field
from pathlib import Path

import yaml

DEFAULT_ARMS = [
    {"name": "ungated", "gate": "none", "flags": {}},
    {"name": "gate_after", "gate": "after", "flags": {}},
    {"name": "gate_before", "gate": "before", "flags": {}},
    # --dedup-threshold only controls the embedding-similarity merge tier; exact-name and honorific
    # merges always run (dream.rs ~L940-965). There is no flag that disables every merge.
    {"name": "no_similarity_merge", "gate": "none", "flags": {"dedup_threshold": 1.01}},
    {"name": "no_prune", "gate": "none", "flags": {"prune_threshold": 0.0}},
    {"name": "no_relations", "gate": "none", "flags": {"no_relations": True}},
]


@dataclass
class Config:
    freeze_commit: str = "486bcf6"
    kwaainet_cmd: str = "kwaainet"
    model: str = "llama3.1:8b"
    inference_url: str | None = None
    dream_flags: dict = field(default_factory=dict)
    arms: list = field(default_factory=lambda: [dict(a) for a in DEFAULT_ARMS])
    snapshots: list = field(default_factory=lambda: [0, 1, 2, 4, 8, 12])
    repeats: int = 3
    scorer: str = "lexical"
    nli_model: str = "MoritzLaurer/DeBERTa-v3-large-mnli-fever-anli-ling-wanli"
    nli_window: int = 384
    nli_stride: int = 256
    nli_batch_size: int = 8
    theta: float = 0.5
    theta_gate: float = 0.5
    corpus_top_n: int = 30
    max_unresolved_evidence_share: float = 0.05
    edit_compat_result: str = ""
    eval_enabled: bool = False
    eval_questions: str = "tests/kwaai-knowledge/d6_eval_questions.json"
    eval_cycles: list | None = None      # None = every configured snapshot
    primary_phase: str = "post_dream"    # observation point of the primary comparison (see FINDINGS.md)
    eval_k: int = 20
    eval_mode: str = "iterative"
    eval_semantic_score: bool = True
    subprocess_timeout_s: float = 7200.0
    verify_original_unchanged: bool = True

    def as_dict(self) -> dict:
        return asdict(self)


def load_config(path: str | Path | None) -> Config:
    if not path:
        return Config()
    d = yaml.safe_load(Path(path).read_text(encoding="utf-8")) or {}
    unknown = set(d) - set(Config.__dataclass_fields__)
    if unknown:
        raise ValueError(f"unknown config keys: {sorted(unknown)}")
    cfg = Config(**d)
    validate(cfg)
    return cfg


PRIMARY_PHASES = ("post_dream", "end_of_cycle")


def validate(cfg: Config) -> Config:
    if cfg.primary_phase not in PRIMARY_PHASES:
        raise ValueError(f"primary_phase must be one of {PRIMARY_PHASES}")
    if 0 not in cfg.snapshots:
        raise ValueError("snapshots must include cycle 0 (the shared baseline)")
    if cfg.eval_cycles is not None and not set(cfg.eval_cycles) <= set(cfg.snapshots):
        raise ValueError("eval_cycles must be a subset of snapshots")
    names = [a.get("name") for a in cfg.arms]
    if len(set(names)) != len(names):
        raise ValueError("arm names must be unique")
    for a in cfg.arms:
        if a.get("gate", "none") not in ("none", "before", "after"):
            raise ValueError(f"arm {a.get('name')}: gate must be none|before|after")
    return cfg


def eval_cycles(cfg: Config) -> list[int]:
    return sorted(cfg.snapshots if cfg.eval_cycles is None else cfg.eval_cycles)


def make_scorer(cfg: Config):
    from .scorers import LexicalScorer, NLIScorer
    if cfg.scorer == "lexical":
        return LexicalScorer()
    if cfg.scorer == "nli":
        return NLIScorer(cfg.nli_model, cfg.nli_window, cfg.nli_stride, cfg.nli_batch_size)
    raise ValueError(f"unknown scorer {cfg.scorer!r}")
