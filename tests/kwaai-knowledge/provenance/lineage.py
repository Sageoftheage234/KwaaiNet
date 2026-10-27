"""Carry claims across one graph transition and attribute every disappearance conservatively.

A transition is one operation on one copy: ``gate`` (this tooling's write) or ``dream`` (one
``kwaainet rag dream run``). Transitions are recorded for every cycle and every phase, not only at
reporting snapshots, so a cause is always inferred from two adjacent states.

Identity (schemas.py): records are matched by record_id; text is never a key. Entity ids are stable in
KwaaiNet except when an entity is deleted, so the only remapping needed is for merges:
graph.rs ``merge_entity_into`` (~L2059-2262) deletes the alias entity, appends its name and aliases to the
canonical entity's ``aliases`` (only if not already present), re-keys its relations onto the canonical id
(combining evidence when the re-keyed relation exists, skipping self-loops at ~L2117), keeps the longer
description, and keeps only the canonical entity's fields.

Attribution confidence:
  observed  - the change is this tooling's own verified write (gate), or a record-level diff with the
              record still present (a field value rewritten in place).
  inferred  - one cause is consistent with the graph evidence and with the DreamReport counts.
  ambiguous - more than one cause is consistent with the evidence; ``candidates`` lists them.
  unknown   - no known KwaaiNet 486bcf6 code path explains the change.
"""
from __future__ import annotations

from dataclasses import dataclass, field, replace

from .facts import field_record_id, relation_record_id, sentence_hash_of
from .schemas import Fact

# graph.rs FAMILIAL_RELS (~L162-177): sanitize_relations (dream step 8, dream.rs ~L1011) deletes these when an
# endpoint is not a Person, and deletes same-gender spouse_of pairs.
FAMILIAL_RELS = frozenset({"parent_of", "child_of", "spouse_of", "sibling_of", "half_sibling_of", "grandparent_of",
                           "grandchild_of", "uncle_of", "aunt_of", "niece_of", "nephew_of", "cousin_of",
                           "foster_parent_of", "foster_child_of"})
CONFIDENCE = ("observed", "inferred", "ambiguous", "unknown")


@dataclass
class EntityFate:
    status: str                 # kept | merged | merged_ambiguous | removed
    target: int | None = None
    confidence: str = "observed"
    candidates: list = field(default_factory=list)


@dataclass
class Step:
    cycle: int
    phase: str                  # "gate" | "dream"
    gate_removed: frozenset = frozenset()     # record ids this tooling deleted in this step (gate only)
    report: dict | None = None                # DreamReport for dream steps
    isolated_by_gate: frozenset = frozenset() # entity ids whose last relation a gate removed


def initial(facts: list[Fact], cycle: int = 0, phase: str = "initial") -> list[Fact]:
    return [replace(f, origin_ids=(f"{f.record_id}@c{cycle}.{phase}",)) for f in facts]


def _lower_aliases(info: dict | None) -> set[str]:
    return {a.lower() for a in (info or {}).get("aliases", [])}


def normalize_name(s: str) -> str:
    """graph.rs ``normalize_name`` (~L699): non-alphanumerics to spaces, collapse whitespace, lowercase."""
    return " ".join("".join(c if c.isalnum() else " " for c in s).split()).lower()


def entity_fates(prev_info: dict, cur_info: dict, step: Step) -> dict[int, EntityFate]:
    """Decide what happened to every entity of the previous state.

    Merge evidence, strongest first:
      1. the entity's name was newly appended to exactly one survivor's ``aliases``;
      2. exactly one survivor has the same normalised name (the exact-name merge tier,
         graph.rs ``find_dedup_candidates_exact`` ~L2896) and its mention_count rose by at least the
         removed entity's mention_count (``merge_entity_into`` adds them);
      3. otherwise, survivors whose mention_count rose by an unexplained amount are listed as ambiguous
         candidates, while the DreamReport says more merges happened than were identified.
    Tier 3 exists because at 486bcf6 ``merge_entity_into`` reads the canonical's aliases from the
    in-memory node, which it never updates (~L2230-2240 vs ~L2265-2272): when several aliases merge into
    one canonical in a cycle, only the last alias name survives in ``aliases``.
    """
    fates: dict[int, EntityFate] = {}
    if step.phase != "dream":
        for e in prev_info:
            fates[e] = EntityFate("kept", e) if e in cur_info else EntityFate("removed", None, "unknown")
        return fates
    gone = [e for e in prev_info if e not in cur_info]
    for e in prev_info:
        if e in cur_info:
            fates[e] = EntityFate("kept", e)
    # unexplained mention-count growth of survivors, consumed as merges are assigned
    growth = {c: ci.get("mention_count", 0) - prev_info[c].get("mention_count", 0)
              for c, ci in cur_info.items() if c in prev_info}
    by_norm: dict[str, list[int]] = {}
    for c, ci in cur_info.items():
        by_norm.setdefault(normalize_name(ci["name"]), []).append(c)

    def assign(e, c, how):
        fates[e] = EntityFate("merged", c, "inferred", [how])
        if c in growth:
            growth[c] -= prev_info[e].get("mention_count", 0)

    pending = []
    for e in gone:
        name = prev_info[e]["name"].lower()
        new_alias = [c for c, ci in cur_info.items()
                     if name in ({a.lower() for a in ci["aliases"]} - _lower_aliases(prev_info.get(c)))]
        if len(new_alias) == 1:
            assign(e, new_alias[0], "alias appended")
        elif len(new_alias) > 1:
            fates[e] = EntityFate("merged_ambiguous", None, "ambiguous", new_alias)
        else:
            pending.append(e)
    still = []
    for e in pending:
        mc = prev_info[e].get("mention_count", 0)
        same = [c for c in by_norm.get(normalize_name(prev_info[e]["name"]), []) if growth.get(c, 0) >= mc]
        if len(same) == 1:
            assign(e, same[0], "exact-name tier, mention_count grew")
        elif len(same) > 1:
            fates[e] = EntityFate("merged_ambiguous", None, "ambiguous", same)
        else:
            still.append(e)
    merged_reported = int((step.report or {}).get("entities_merged", 0) or 0)
    merged_found = sum(1 for f in fates.values() if f.status == "merged")
    pruned_reported = int((step.report or {}).get("entities_pruned", 0) or 0)
    for e in still:
        pi = prev_info[e]
        mc = pi.get("mention_count", 0)
        prune_ok = pi.get("degree", 0) == 0 and pi.get("chunk_links", 0) == 0 and mc <= 1
        old_alias = [c for c, ci in cur_info.items() if pi["name"].lower() in {a.lower() for a in ci["aliases"]}]
        grew = [c for c, g in growth.items() if g >= mc > 0]
        merge_possible = merged_reported > merged_found and (grew or old_alias)
        if merge_possible:
            cands = sorted(set(old_alias) | set(grew), key=str)[:10]
            fates[e] = EntityFate("merged_ambiguous", None, "ambiguous",
                                  cands + (["prune"] if prune_ok else []))
        elif not prune_ok:
            fates[e] = EntityFate("removed", None, "unknown", ["prune preconditions not met in previous state"])
        elif len(still) <= pruned_reported:
            fates[e] = EntityFate("removed", None, "inferred", ["prune"])
        else:
            fates[e] = EntityFate("removed", None, "ambiguous", ["prune", "unreported deletion"])
    return fates


def _mapped_record_id(f: Fact, emap: dict[int, int], removed: frozenset = frozenset()) -> str | None:
    ids = [emap.get(e, e if e not in removed else None) for e in f.entity_ids]
    if any(i is None for i in ids):
        return None
    if f.kind == "field":
        return field_record_id(ids[0], f.field_name or "")
    if f.kind == "relation":
        return relation_record_id(ids[0], ids[1], f.relation_type or "")
    return None  # descriptions are matched by sentence hash below


def transition(prev: list[Fact], prev_info: dict, cur_raw: list[Fact], cur_info: dict, step: Step):
    """Return (current facts with origin_ids, events, fates)."""
    fates = entity_fates(prev_info, cur_info, step)
    emap = {e: f.target for e, f in fates.items() if f.target is not None}
    removed_ids = frozenset(e for e, f in fates.items() if f.target is None)
    cur_by_id = {f.record_id: f for f in cur_raw}
    desc_index: dict[tuple[int, str], list[str]] = {}
    for f in cur_raw:
        if f.kind == "description":
            desc_index.setdefault((f.entity_ids[0], sentence_hash_of(f.record_id)), []).append(f.record_id)
    carried: dict[str, list[str]] = {}     # current record id -> origins carried into it
    preds: dict[str, int] = {}             # current record id -> number of predecessor records
    events: list[dict] = []
    claimed_desc: set[str] = set()

    def ev(kind_, f: Fact, cause, confidence, candidates=(), to=None, **extra):
        events.append({"cycle": step.cycle, "phase": step.phase, "event": kind_, "kind": f.kind,
                       "record_id": f.record_id, "origin_ids": list(f.origin_ids), "cause": cause,
                       "confidence": confidence, "candidates": list(candidates), "to_record_id": to, **extra})

    def carry(f: Fact, rid: str):
        carried.setdefault(rid, []).extend(f.origin_ids)
        preds[rid] = preds.get(rid, 0) + 1

    for p in prev:
        fs = [fates.get(e) for e in p.entity_ids]
        remapped = any(x and x.status == "merged" for x in fs)
        # ---- descriptions: matched by (entity, sentence hash) on the mapped entity
        if p.kind == "description":
            tgt = emap.get(p.entity_ids[0])
            cand = [r for r in desc_index.get((tgt, sentence_hash_of(p.record_id)), []) if r not in claimed_desc] \
                if tgt is not None else []
            if cand:
                claimed_desc.add(cand[0])
                carry(p, cand[0])
                if remapped:
                    ev("rewritten", p, "merge", fs[0].confidence, to=cand[0])
                continue
        else:
            rid = _mapped_record_id(p, emap, removed_ids)
            if p.kind == "relation" and rid is not None and remapped and emap[p.entity_ids[0]] == emap[p.entity_ids[1]]:
                ev("lost", p, "merge", "inferred", note="re-keyed relation became a self-loop; merge skips those")
                continue
            if rid is not None and rid in cur_by_id:
                c = cur_by_id[rid]
                if p.kind == "field" and c.value != p.value:
                    if remapped:
                        ev("lost", p, "merge", "inferred", note="alias field dropped; canonical holds a different value")
                    else:
                        ev("lost", p, "value_rewritten", "observed", to=rid,
                           note=f"value changed in place: {p.value!r} -> {c.value!r}")
                    continue
                carry(p, rid)
                if remapped:
                    ev("rewritten", p, "merge", max((x.confidence for x in fs if x and x.status == "merged"),
                                                   key=CONFIDENCE.index), to=rid)
                continue
        # ---- the claim is gone: attribute it
        if step.phase == "gate":
            if p.record_id in step.gate_removed:
                ev("lost", p, "gate", "observed")
            else:
                ev("lost", p, "unknown", "unknown", note="disappeared during a gate step but was not rejected")
            continue
        report = step.report or {}
        missing = [x for x in fs if x is None or x.target is None]
        if missing:
            fate = missing[0] or EntityFate("removed", None, "unknown")
            if fate.status == "merged_ambiguous":
                ev("lost", p, "merge", "ambiguous", fate.candidates)
            elif p.kind == "relation":
                ev("lost", p, "unknown", "unknown", note="an endpoint was deleted; prune requires no neighbours")
            else:
                gate_iso = p.entity_ids[0] in step.isolated_by_gate
                ev("lost", p, "prune", fate.confidence, fate.candidates, after_gate_isolation=gate_iso)
            continue
        if remapped:
            if p.kind == "relation":
                ev("lost", p, "merge", "ambiguous", ["merge", "sanitize"],
                   note="endpoint merged but the re-keyed relation is absent")
            else:
                ev("lost", p, "merge", "inferred",
                   note="alias field dropped" if p.kind == "field" else "canonical kept the other description")
            continue
        if p.kind == "relation":
            if (p.relation_type or "") in FAMILIAL_RELS:
                ev("lost", p, "sanitize", "inferred")
            else:
                ev("lost", p, "unknown", "unknown", note="no 486bcf6 dream step deletes this relation type")
        elif p.kind == "field":
            ev("lost", p, "rewrite", "ambiguous", ["completion upsert"],
               note="field removed while its entity survived")
        else:
            e = p.entity_ids[0]
            gained = ({a.lower() for a in cur_info.get(e, {}).get("aliases", [])} - _lower_aliases(prev_info.get(e)))
            if gained:
                ev("lost", p, "rewrite", "ambiguous", ["summary completion", "merge (longer alias description kept)"])
            elif int(report.get("entities_summary_completed", 0) or 0) > 0:
                ev("lost", p, "rewrite", "inferred", ["summary completion"])
            else:
                ev("lost", p, "rewrite", "ambiguous", ["summary completion", "unreported"])

    out = []
    for f in cur_raw:
        origins = carried.get(f.record_id)
        if not origins:
            f2 = replace(f, origin_ids=(f"{f.record_id}@c{step.cycle}.{step.phase}",))
            events.append({"cycle": step.cycle, "phase": step.phase, "event": "added", "kind": f.kind,
                           "record_id": f.record_id, "origin_ids": list(f2.origin_ids), "cause": step.phase,
                           "confidence": "observed", "candidates": [], "to_record_id": None})
        else:
            f2 = replace(f, origin_ids=tuple(dict.fromkeys(origins)))
            if preds.get(f.record_id, 0) > 1:
                events.append({"cycle": step.cycle, "phase": step.phase, "event": "consolidated", "kind": f.kind,
                               "record_id": f.record_id, "origin_ids": list(f2.origin_ids), "cause": "merge",
                               "confidence": "inferred", "candidates": [], "to_record_id": None})
        out.append(f2)
    return out, events, fates


def isolated_by_gate(pre_info: dict, post_info: dict, previous: frozenset = frozenset()) -> frozenset:
    """Entities a gate step left with no relations (they become prune-eligible), plus earlier ones still isolated."""
    now = {e for e, i in post_info.items() if i["degree"] == 0 and pre_info.get(e, {}).get("degree", 0) > 0}
    still = {e for e in previous if post_info.get(e, {}).get("degree", 1) == 0}
    return frozenset(now | still)
