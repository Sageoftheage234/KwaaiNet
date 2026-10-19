import hashlib
import shutil

import pytest
from conftest import TENANT, eid

from provenance.facts import extract_facts, preflight
from provenance.gate import apply, decide
from provenance.kwaainet_store import GraphReader, GraphWriter, MetaReader, dumps, find_kb_files, loads
from provenance.lineage import initial
from provenance.scorers import LexicalScorer
from provenance.support import score_facts


def sha(p):
    return hashlib.sha256(p.read_bytes()).hexdigest()


def test_codec_round_trip_is_byte_identical():
    raw = ('{"id":-42,"name":"Zoë \\"Z\\" Ünal","embedding":[0.1,1e-5,-0.33333334,1.0],"x":null,'
           '"t":true,"s":"a\\nb\\tc\\u0001","o":{"k":[]}}').encode()
    assert dumps(loads(raw)) == raw


def test_every_fixture_record_round_trips(kwaainet_home):
    _, data = kwaainet_home
    kb = find_kb_files(data)
    g = GraphReader(kb.graph_db)
    for _, v, raw in list(g.entities()) + list(g.relations()):
        assert dumps(v) == raw


def test_adapter_fact_counts_and_evidence_levels(kwaainet_home):
    _, data = kwaainet_home
    facts, info = extract_facts(find_kb_files(data).graph_db)
    kinds = [f.kind for f in facts]
    assert kinds.count("field") == 6 and kinds.count("relation") == 6 and kinds.count("description") == 3
    assert len({f.record_id for f in facts}) == len(facts)
    desc = {f.text: f for f in facts if f.kind == "description"}
    assert desc["Carol: She works in Paris."].evidence_chunk_ids == (5,)
    assert desc["Acme: Acme is a company."].evidence_chunk_ids == (3,)
    assert all(f.evidence_level == "entity" and not f.gate_eligible for f in desc.values())
    assert all(f.gate_eligible and f.evidence_level == "fact" for f in facts if f.kind != "description")
    assert info[eid("Carol")]["name"] == "Carol"
    assert info[eid("Dana")]["degree"] == 1 and info[eid("Dana")]["chunk_links"] == 0


def test_support_classes_cover_all_four(kwaainet_home):
    _, data = kwaainet_home
    kb = find_kb_files(data)
    facts, _ = extract_facts(kb.graph_db)
    sup = {f.text: s.support_class for f, s in zip(facts, score_facts(facts, MetaReader(kb.meta_db), LexicalScorer(), 0.5))}
    assert sup["Leslie Groves occupation general"] == "traceable"
    assert sup["Bob occupation scientist"] == "misattributed"   # cited chunk 4 says Paris; chunk 7 supports it
    assert sup["Ghost planet Pluto"] == "unlinked"
    assert sup["Groves mentors Carol"] == "unsupported"         # no chunk shares more than 1 of 3 tokens


def test_preflight_rejects_unresolved_evidence(kwaainet_home, tmp_path):
    _, data = kwaainet_home
    copy = tmp_path / "c"
    shutil.copytree(data, copy)
    kb = find_kb_files(copy)
    key, node, _ = next(GraphReader(kb.graph_db).entities())
    node["fields"]["x"] = {"value": "y", "evidence_chunk_ids": [999, 998, 997, 996, 995, 994], "confidence": 1}
    w = GraphWriter(kb.graph_db)
    w.con.execute("UPDATE entities SET value=? WHERE key=?", (dumps(node), key.to_bytes(8, "little", signed=True)))
    w.commit_close()
    with pytest.raises(ValueError, match="unresolved evidence share"):
        preflight(kb, 0.05)


def test_virtual_gate_writes_nothing_and_apply_edits_only_matching_rows(kwaainet_home, tmp_path):
    _, data = kwaainet_home
    copy = tmp_path / "c"
    shutil.copytree(data, copy)
    kb = find_kb_files(copy)
    facts, _ = extract_facts(kb.graph_db)
    facts = initial(facts)
    sup = score_facts(facts, MetaReader(kb.meta_db), LexicalScorer(), 0.5)
    before = sha(kb.graph_db)
    dec = decide(facts, sup, 0.5)
    assert sha(kb.graph_db) == before
    untouched = {k: raw for k, _, raw in GraphReader(kb.graph_db).entities()}
    removed = apply(dec, kb.graph_db, protected=[data])
    rej = [d for d in dec if not d.accepted]
    assert removed["relations"] == sum(d.kind == "relation" for d in rej) > 0
    assert removed["fields"] == sum(d.kind == "field" for d in rej) == 2   # Bob (cites chunk 4) and Ghost (no evidence)
    left = {(s, d, t) for (s, d, t), _, _ in GraphReader(kb.graph_db).relations()}
    assert left == {(d.entity_ids[0], d.entity_ids[1], d.relation_type) for d in dec if d.kind == "relation" and d.accepted}
    after = {k: raw for k, _, raw in GraphReader(kb.graph_db).entities()}
    assert after[eid("Leslie Groves")] == untouched[eid("Leslie Groves")]   # accepted record byte-identical
    assert loads(after[eid("Bob")])["fields"] == {}


def test_writer_refuses_protected_paths(kwaainet_home):
    _, data = kwaainet_home
    with pytest.raises(PermissionError):
        GraphWriter(data / f"graph-{TENANT}.db", protected=[data])
