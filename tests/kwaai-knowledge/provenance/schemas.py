from __future__ import annotations

from dataclasses import dataclass

# Three identities are kept apart (audit item 3):
#   record_id  - where the claim is persisted: unique within one graph state.
#                field "F|<entity id>|<field name>", relation "R|<src id>|<dst id>|<type>",
#                description sentence "D|<entity id>|<sentence hash>|<occurrence>".
#   content_id - hash of kind + normalised text. Equal for textually identical claims held by different records
#                (e.g. two same-named entities); reported, never used as a key.
#   origin_ids - the historical claims a record carries. A record created at cycle c, phase p gets
#                "<record_id>@c<c>.<p>". A record that absorbs others in a merge carries all their origins.


@dataclass(frozen=True)
class Fact:
    """One atomic, checkable claim read from a graph state.

    kind: "field" or "relation" (own persisted evidence ids; gate-eligible) or "description" (one sentence of
    the entity description; evidence is the entity's chunk links; never gated, reported separately).
    """

    record_id: str
    kind: str
    entity_ids: tuple[int, ...]
    text: str
    evidence_chunk_ids: tuple[int, ...]
    content_id: str
    value: str = ""
    evidence_level: str = "fact"
    relation_type: str | None = None
    field_name: str | None = None
    entity_names: tuple[str, ...] = ()
    origin_ids: tuple[str, ...] = ()
    gate_eligible: bool = True
    entity_has_fields: bool | None = None


@dataclass(frozen=True)
class FactSupport:
    record_id: str
    linked_support: float | None
    corpus_support: float | None
    support_class: str  # traceable | misattributed | unsupported | unlinked
    scorer: str


@dataclass(frozen=True)
class GateDecision:
    record_id: str
    accepted: bool
    linked_support: float | None
    threshold: float
    kind: str
    text: str
    evidence_chunk_ids: tuple[int, ...] = ()
    entity_ids: tuple[int, ...] = ()
    relation_type: str | None = None
    field_name: str | None = None
    origin_ids: tuple[str, ...] = ()
