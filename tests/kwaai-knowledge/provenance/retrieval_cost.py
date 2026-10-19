"""Parse ``kwaainet rag eval`` reports and compute the gate's retrieval cost.

At 486bcf6, ordinary ``rag eval`` (rag_cmd.rs ~L8422-9360) scores each question's *expected keywords* by token
overlap (optionally with an embedding-similarity fallback). Its summary table has
"Overall recall (token-overlap)"; with ``--semantic-score`` it has
"Retrieval recall (token-overlap + semantic)" (share of expected keywords found in the retrieved chunks) and
"Generation recall (token-overlap + semantic)" (share found in the generated answer).

These are keyword-coverage measures. They are NOT conventional Recall@k or MRR: there are no gold chunk or
entity labels and no ranks. Recall@k and MRR are recorded as not_computed and never substituted or zero-filled.
"""
from __future__ import annotations

import re

NOT_COMPUTED = {
    "recall_at_k": "not_computed: rag eval at 486bcf6 has no gold chunk/entity labels; Recall@k is not computed "
                   "(only rag dream embed-eval computes it, on a different task)",
    "mrr": "not_computed: rag eval at 486bcf6 has no gold chunk/entity labels; MRR is not computed",
}
DEFINITIONS = {
    "retrieval_keyword_recall": "rag eval 'Retrieval recall (token-overlap + semantic)': share of each question's "
                                "expected keywords found in the retrieved chunks, averaged over questions",
    "generation_keyword_recall": "rag eval 'Generation recall' (or 'Overall recall (token-overlap)' without "
                                 "--semantic-score): share of expected keywords found in the generated answer",
}
METRICS = tuple(DEFINITIONS)

_ROW = re.compile(r"^\|\s*(?P<label>[^|]+?)\s*\|\s*(?P<pct>-?[\d.]+)%\s*\((?P<num>[\d.]+)/(?P<den>[\d.]+)\)\s*\|", re.M)
_LABELS = {
    "Retrieval recall (token-overlap + semantic)": "retrieval_keyword_recall",
    "Generation recall (token-overlap + semantic)": "generation_keyword_recall",
    "Overall recall (token-overlap)": "generation_keyword_recall",
}


def parse_eval_report(md: str) -> dict:
    out: dict = {"retrieval_keyword_recall": None, "generation_keyword_recall": None, "scoring": None,
                 **NOT_COMPUTED}
    for m in _ROW.finditer(md):
        key = _LABELS.get(m.group("label").strip())
        if key:
            out[key] = float(m.group("pct")) / 100.0
            out["scoring"] = "token-overlap + semantic" if "semantic" in m.group("label") else "token-overlap"
    if out["generation_keyword_recall"] is None:
        raise ValueError("could not find a recall row in the rag eval report; format differs from 486bcf6")
    if out["retrieval_keyword_recall"] is None:
        out["retrieval_keyword_recall_note"] = "not_computed: run rag eval with --semantic-score to get it"
    return out


def gate_cost(gated: dict, ungated: dict) -> dict:
    """Gated minus ungated for one (cycle, repeat index). Repeats of different arms share only the starting
    snapshot, so matching by repeat index is a convention, not a true pairing; report.py also gives the
    difference of arm means."""
    out = {}
    for k in METRICS:
        a, b = gated.get(k), ungated.get(k)
        out[f"{k}_delta"] = None if a is None or b is None else a - b
    out.update(NOT_COMPUTED)
    return out
