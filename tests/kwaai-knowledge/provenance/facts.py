"""Atomic fact extraction from a KwaaiNet graph state (read-only)."""
from __future__ import annotations

import hashlib
import re
from collections import Counter

from .kwaainet_store import GraphReader, KBFiles, MetaReader, plain
from .schemas import Fact

_SENT = re.compile(r"(?<=[.!?])\s+(?=[A-Z0-9\"'(])")
GATE_ELIGIBLE_KINDS = ("field", "relation")


def _h(s: str, n: int = 24) -> str:
    return hashlib.sha256(s.encode("utf-8")).hexdigest()[:n]


def normalise(text: str) -> str:
    return " ".join(text.strip().lower().split())


def content_id(kind: str, text: str) -> str:
    """Textual identity. Two different records may share it; it is never used as a key."""
    return _h(kind + "|" + normalise(text))


def field_record_id(eid: int, field: str) -> str:
    return f"F|{eid}|{field}"


def relation_record_id(src: int, dst: int, rel_type: str) -> str:
    return f"R|{src}|{dst}|{rel_type}"


def description_record_id(eid: int, sentence: str, occurrence: int) -> str:
    return f"D|{eid}|{_h(normalise(sentence), 12)}|{occurrence}"


def sentence_hash_of(record_id: str) -> str:
    return record_id.split("|")[2]


def field_text(name: str, field: str, value: str) -> str:
    return f"{name} {field} {value}"


def relation_text(src: str, rel_type: str, dst: str) -> str:
    return f"{src} {rel_type} {dst}"


def description_text(name: str, sentence: str) -> str:
    return f"{name}: {sentence}"


def split_sentences(text: str) -> list[str]:
    return [s.strip() for s in _SENT.split(text.strip()) if s.strip()]


def extract_facts(graph_db) -> tuple[list[Fact], dict]:
    """Return (facts, entity_info) for one graph state.

    Field and relation facts carry their own persisted ``evidence_chunk_ids`` (graph.rs FieldValue,
    RelationRecord) and are gate-eligible. Every non-empty entity description is split into sentences; each
    sentence is a description fact with the entity's ``entity_chunk`` links as evidence
    (evidence_level="entity"), is never gated, and is reported separately. Descriptions are extracted for all
    entities: at ingestion graph.rs keeps the longest of the field-derived, incoming and existing descriptions
    (~L1057-1080), so a field-bearing entity can carry prose that is not derived from its fields.

    entity_info maps entity id -> name, aliases, entity_type, mention_count, degree, chunk_links; the last
    three are the observable prune preconditions (dream.rs ~L974-994).
    """
    g = GraphReader(graph_db)
    ents = {eid: plain(node) for eid, node, _ in g.entities()}
    ec = g.entity_chunks()
    rels = [((s, d, t), plain(r)) for (s, d, t), r, _ in g.relations()]
    degree: Counter = Counter()
    for (s, d, _), _r in rels:
        degree[s] += 1
        degree[d] += 1
    out: list[Fact] = []
    for eid, node in ents.items():
        name = str(node.get("name", eid))
        fields = node.get("fields") or {}
        for fname, fv in fields.items():
            value = str(fv.get("value", ""))
            text = field_text(name, fname, value)
            out.append(Fact(field_record_id(eid, fname), "field", (eid,), text,
                            tuple(int(x) for x in fv.get("evidence_chunk_ids", [])), content_id("field", text),
                            value=value, field_name=fname, entity_names=(name,)))
        desc = str(node.get("description", "") or "").strip()
        if desc:
            seen: Counter = Counter()
            ev = tuple(int(x) for x in ec.get(eid, []))
            for s in split_sentences(desc):
                occ = seen[normalise(s)]
                seen[normalise(s)] += 1
                text = description_text(name, s)
                out.append(Fact(description_record_id(eid, s, occ), "description", (eid,), text, ev,
                                content_id("description", text), value=s, evidence_level="entity",
                                entity_names=(name,), gate_eligible=False, entity_has_fields=bool(fields)))
    for (src, dst, rtype), rel in rels:
        sn = str(ents.get(src, {}).get("name", src))
        dn = str(ents.get(dst, {}).get("name", dst))
        text = relation_text(sn, rtype, dn)
        out.append(Fact(relation_record_id(src, dst, rtype), "relation", (src, dst), text,
                        tuple(int(x) for x in rel.get("evidence_chunk_ids", [])), content_id("relation", text),
                        relation_type=rtype, entity_names=(sn, dn)))
    ids = Counter(f.record_id for f in out)
    dup = [k for k, n in ids.items() if n > 1]
    if dup:
        raise ValueError(f"record ids are not unique in {graph_db}: {dup[:5]}")
    info = {eid: {"name": str(n.get("name", eid)), "aliases": [str(a) for a in n.get("aliases", []) or []],
                  "entity_type": str(n.get("entity_type", "")), "mention_count": int(n.get("mention_count", 0) or 0),
                  "degree": degree.get(eid, 0), "chunk_links": len(ec.get(eid, []))}
            for eid, n in ents.items()}
    return out, info


def evidence_resolution(facts: list[Fact], meta: MetaReader) -> dict:
    available = set(meta.chunks())
    if not available:
        raise ValueError(f"chunk table is empty in {meta.path}")
    ids = [i for f in facts for i in f.evidence_chunk_ids]
    resolved = sum(i in available for i in ids)
    return {"evidence_ids": len(ids), "resolved_evidence_ids": resolved,
            "unresolved_evidence_share": (1 - resolved / len(ids)) if ids else 0.0}


def preflight(kb: KBFiles, max_unresolved_share: float = 0.05) -> dict:
    facts, _ = extract_facts(kb.graph_db)
    er = evidence_resolution(facts, MetaReader(kb.meta_db))
    result = {"field_facts": sum(f.kind == "field" for f in facts),
              "relation_facts": sum(f.kind == "relation" for f in facts),
              "description_facts": sum(f.kind == "description" for f in facts),
              "facts_with_evidence": sum(bool(f.evidence_chunk_ids) for f in facts), **er}
    if result["field_facts"] + result["relation_facts"] == 0:
        raise ValueError(f"preflight failed: no field or relation facts; not a provenance-capable graph: {result}")
    if result["unresolved_evidence_share"] > max_unresolved_share:
        raise ValueError(f"preflight failed: unresolved evidence share {result['unresolved_evidence_share']:.1%} "
                         f"exceeds {max_unresolved_share:.1%}: {result}")
    return result
