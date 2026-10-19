"""Score each fact against its own evidence chunks and against the whole corpus."""
from __future__ import annotations

import math
import re
from collections import Counter

from .facts import evidence_resolution
from .kwaainet_store import MetaReader
from .schemas import Fact, FactSupport

_TOK = re.compile(r"[a-z0-9]+")


def _tok(s: str) -> list[str]:
    return _TOK.findall(s.lower())


class BM25:
    def __init__(self, docs: dict[int, str], k1: float = 1.5, b: float = 0.75):
        self.ids = list(docs)
        self.toks = {i: _tok(docs[i]) for i in self.ids}
        self.tf = {i: Counter(t) for i, t in self.toks.items()}
        self.avgdl = (sum(len(t) for t in self.toks.values()) / len(self.ids)) if self.ids else 0.0
        df = Counter(w for t in self.toks.values() for w in set(t))
        n = len(self.ids)
        self.idf = {w: math.log(1 + (n - d + 0.5) / (d + 0.5)) for w, d in df.items()}
        self.k1, self.b = k1, b

    def top(self, query: str, n: int) -> list[int]:
        q = set(_tok(query))
        scored = []
        for i in self.ids:
            tf, dl = self.tf[i], len(self.toks[i]) or 1
            s = sum(self.idf.get(w, 0.0) * tf[w] * (self.k1 + 1) /
                    (tf[w] + self.k1 * (1 - self.b + self.b * dl / (self.avgdl or 1))) for w in q if w in tf)
            if s > 0:
                scored.append((s, i))
        scored.sort(key=lambda x: (-x[0], x[1]))
        return [i for _, i in scored[:n]]


class SupportModel:
    """Scores facts, caching by (text, evidence ids). Chunks are read once: a dream cycle does not modify the
    MetaStore, and scores depend only on the fact text, its evidence ids and the chunk texts."""

    def __init__(self, meta: MetaReader, scorer, theta: float = 0.5, top_n: int = 30,
                 max_unresolved_share: float = 0.05):
        self.meta, self.scorer, self.theta, self.top_n = meta, scorer, theta, top_n
        self.max_unresolved_share = max_unresolved_share
        self.chunks = {cid: c.get("text", "") for cid, c in meta.chunks().items()}
        self.bm = BM25(self.chunks)
        self.cache: dict[tuple, tuple[float | None, float]] = {}
        self.pairs_scored = 0

    def score(self, facts: list[Fact]) -> list[FactSupport]:
        er = evidence_resolution(facts, self.meta)
        if er["unresolved_evidence_share"] > self.max_unresolved_share:
            raise ValueError(f"unresolved evidence share {er['unresolved_evidence_share']:.1%} exceeds "
                             f"{self.max_unresolved_share:.1%}")
        todo = {}
        for f in facts:
            key = (f.text, f.evidence_chunk_ids)
            if key not in self.cache and key not in todo:
                todo[key] = f
        pairs, plan = [], []
        for key, f in todo.items():
            linked = [cid for cid in f.evidence_chunk_ids if cid in self.chunks]
            corpus = self.bm.top(f.text, self.top_n)
            plan.append((key, len(pairs), len(linked), len(corpus)))
            pairs += [(self.chunks[c], f.text) for c in linked] + [(self.chunks[c], f.text) for c in corpus]
        scores = self.scorer.score_batch(pairs) if pairs else []
        self.pairs_scored += len(pairs)
        for key, start, nl, nc in plan:
            ls = max(scores[start:start + nl]) if nl else None
            cs = max(scores[start + nl:start + nl + nc], default=0.0)
            self.cache[key] = (ls, cs)
        out = []
        for f in facts:
            ls, cs = self.cache[(f.text, f.evidence_chunk_ids)]
            if not f.evidence_chunk_ids:
                cls = "unlinked"
            elif (ls or 0.0) >= self.theta:
                cls = "traceable"
            elif cs >= self.theta:
                cls = "misattributed"
            else:
                cls = "unsupported"
            out.append(FactSupport(f.record_id, ls, cs, cls, self.scorer.name))
        return out


def score_facts(facts: list[Fact], meta: MetaReader, scorer, theta: float = 0.5, top_n: int = 30,
                max_unresolved_share: float = 0.05) -> list[FactSupport]:
    return SupportModel(meta, scorer, theta, top_n, max_unresolved_share).score(facts)
