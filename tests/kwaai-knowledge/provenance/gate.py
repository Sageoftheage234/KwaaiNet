"""Evidence gate: accept a field or relation fact only if its own evidence chunks entail it.

decide() never writes. apply() writes rejections into a graph DB *copy*; the runner calls it only after an
edit-compatibility result bound to this snapshot, binary and writer code has passed (see runner).
Description sentences are not gate-eligible: KwaaiNet stores no per-sentence evidence for them.
"""
from __future__ import annotations

from .facts import GATE_ELIGIBLE_KINDS
from .kwaainet_store import GraphWriter
from .schemas import Fact, FactSupport, GateDecision


def decide(facts: list[Fact], supports: list[FactSupport], theta_gate: float) -> list[GateDecision]:
    out = []
    for f, s in zip(facts, supports):
        if f.kind not in GATE_ELIGIBLE_KINDS:
            continue
        accept = (s.linked_support or 0.0) >= theta_gate
        out.append(GateDecision(f.record_id, accept, s.linked_support, theta_gate, f.kind, f.text,
                                f.evidence_chunk_ids, f.entity_ids, f.relation_type, f.field_name, f.origin_ids))
    return out


def apply(decisions: list[GateDecision], graph_db_copy, protected=()) -> dict:
    w = GraphWriter(graph_db_copy, protected)
    removed = {"relations": 0, "fields": 0}
    try:
        for d in decisions:
            if d.accepted:
                continue
            if d.kind == "relation":
                removed["relations"] += w.delete_relation(d.entity_ids[0], d.entity_ids[1], d.relation_type or "")
            elif d.kind == "field":
                removed["fields"] += int(w.remove_field(d.entity_ids[0], d.field_name or ""))
    finally:
        w.commit_close()
    return removed
