"""Identity, lineage and attribution on hand-built before/after graph states (exact 486bcf6 format)."""
from collections import Counter

from conftest import node, write_graph

from provenance.facts import extract_facts
from provenance.lineage import Step, initial, isolated_by_gate, transition


def state(tmp_path, name, ents, rels=(), links=None):
    db = tmp_path / f"{name}.db"
    write_graph(db, {i: e for i, e in enumerate(ents)}, list(rels), links or {})
    return extract_facts(db)


def ids(n):
    return int(n["id"])


def lost(events):
    return {(e["kind"], e["cause"], e["confidence"]) for e in events if e["event"] == "lost"}


def test_identical_text_in_distinct_records_is_never_collapsed(tmp_path):
    sp = node("Smith", {"role": ("founder", [1])}, etype="Person")
    so = node("Smith", {"role": ("founder", [1])}, etype="Organization")
    al = node("Alice")
    facts, info = state(tmp_path, "a", [sp, so, al], [(sp, "knows", al, [1]), (so, "knows", al, [1])])
    ge = [f for f in facts if f.kind in ("field", "relation")]
    assert len(ge) == 4 and len({f.record_id for f in ge}) == 4
    assert Counter(f.content_id for f in ge).most_common(1)[0][1] == 2   # same text, two records each
    prev = initial(facts)
    assert len({o for f in prev for o in f.origin_ids}) == len(prev)
    # The Organization "Smith" disappears; the Person "Smith" with identical text must keep its own origins.
    cur_raw, cur_info = state(tmp_path, "b", [sp, al], [(sp, "knows", al, [1])])
    cur, events, _ = transition(prev, info, cur_raw, cur_info, Step(1, "dream", report={"entities_pruned": 1}))
    kept = {f.record_id: f.origin_ids for f in cur}
    for f in prev:
        if ids(sp) in f.entity_ids and f.kind != "description":
            assert kept[f.record_id] == f.origin_ids
    gone = [e for e in events if e["event"] == "lost"]
    assert {e["record_id"] for e in gone} == {f.record_id for f in prev if ids(so) in f.entity_ids}
    # so had a relation, so prune preconditions fail: never reported as a confident prune
    assert all(e["confidence"] == "unknown" for e in gone)


def test_merge_consolidates_many_to_one_without_dropping_origins(tmp_path):
    canon, alias, al = node("John Smith"), node("Smith"), node("Alice")
    other = node("Smith", etype="Organization")       # same name as the alias, different record: must not move
    prev_f, prev_i = state(tmp_path, "a", [
        node("John Smith", {"role": ("founder", [1])}),
        node("Smith", {"role": ("founder", [2]), "hobby": ("chess", [3])}), al, other],
        [(canon, "knows", al, [1]), (alias, "knows", al, [2]), (other, "knows", al, [4])])
    prev = initial(prev_f)
    merged = node("John Smith", {"role": ("founder", [1])}, aliases=["Smith"])
    cur_raw, cur_i = state(tmp_path, "b", [merged, al, other], [(merged, "knows", al, [1, 2]), (other, "knows", al, [4])])
    cur, events, fates = transition(prev, prev_i, cur_raw, cur_i, Step(1, "dream", report={"entities_merged": 1}))
    assert fates[ids(alias)].status == "merged" and fates[ids(alias)].target == ids(canon)
    assert fates[ids(other)].status == "kept"
    by_id = {f.record_id: f for f in cur}
    rel = by_id[f"R|{ids(canon)}|{ids(al)}|knows"]
    fld = by_id[f"F|{ids(canon)}|role"]
    assert len(rel.origin_ids) == 2 and len(fld.origin_ids) == 2           # both claims still present
    assert by_id[f"R|{ids(other)}|{ids(al)}|knows"].origin_ids == \
        next(f.origin_ids for f in prev if f.record_id == f"R|{ids(other)}|{ids(al)}|knows")
    kinds = Counter(e["event"] for e in events)
    assert kinds["consolidated"] == 2 and kinds["rewritten"] == 2
    assert lost(events) == {("field", "merge", "inferred")}                  # alias hobby: alias fields are dropped
    prev_origins = {o for f in prev for o in f.origin_ids}
    lost_origins = {o for e in events if e["event"] == "lost" for o in e["origin_ids"]}
    assert prev_origins == {o for f in cur for o in f.origin_ids} | lost_origins


def test_field_value_rewritten_in_place_is_observed_and_starts_a_new_origin(tmp_path):
    prev_f, prev_i = state(tmp_path, "a", [node("Bob", {"occupation": ("scientist", [4])})])
    prev = initial(prev_f)
    cur_raw, cur_i = state(tmp_path, "b", [node("Bob", {"occupation": ("physicist", [4])})])
    cur, events, _ = transition(prev, prev_i, cur_raw, cur_i, Step(1, "dream", report={}))
    assert lost(events) == {("field", "value_rewritten", "observed")}
    f = next(f for f in cur if f.kind == "field")
    assert f.origin_ids == (f"{f.record_id}@c1.dream",) and f.origin_ids != prev[0].origin_ids


def test_relation_losses_with_live_endpoints(tmp_path):
    al, acme = node("Alice"), node("Acme", etype="Organization")
    prev_f, prev_i = state(tmp_path, "a", [al, acme], [(al, "parent_of", acme, [3]), (al, "likes", acme, [3])])
    cur_raw, cur_i = state(tmp_path, "b", [al, acme])
    _, events, _ = transition(initial(prev_f), prev_i, cur_raw, cur_i, Step(1, "dream", report={}))
    assert lost(events) == {("relation", "sanitize", "inferred"), ("relation", "unknown", "unknown")}


def test_prune_confidence_follows_preconditions_and_report_counts(tmp_path):
    ghost, keep = node("Ghost", {"planet": ("Pluto", [])}), node("Keep")
    linked = node("Linked", {"x": ("y", [1])})
    prev_f, prev_i = state(tmp_path, "a", [ghost, keep, linked], links={1: [linked]})
    prev = initial(prev_f)
    cur_raw, cur_i = state(tmp_path, "b", [keep])
    _, ev1, _ = transition(prev, prev_i, cur_raw, cur_i, Step(1, "dream", report={"entities_pruned": 2}))
    by = {e["record_id"]: (e["cause"], e["confidence"]) for e in ev1}
    assert by[f"F|{ids(ghost)}|planet"] == ("prune", "inferred")
    assert by[f"F|{ids(linked)}|x"] == ("prune", "unknown")       # had chunk links: prune cannot have removed it
    _, ev2, _ = transition(prev, prev_i, cur_raw, cur_i, Step(1, "dream", report={"entities_pruned": 0}))
    assert {e["record_id"]: e["confidence"] for e in ev2}[f"F|{ids(ghost)}|planet"] == "ambiguous"


def test_ambiguous_merge_is_not_reported_as_inferred(tmp_path):
    a, b, s = node("Ann Smith"), node("Bea Smith"), node("Smith", {"role": ("x", [1])})
    prev_f, prev_i = state(tmp_path, "a", [a, b, s])
    cur_raw, cur_i = state(tmp_path, "b", [node("Ann Smith", aliases=["Smith"]), node("Bea Smith", aliases=["Smith"])])
    _, events, fates = transition(initial(prev_f), prev_i, cur_raw, cur_i, Step(1, "dream", report={"entities_merged": 1}))
    assert fates[ids(s)].status == "merged_ambiguous"
    assert lost(events) == {("field", "merge", "ambiguous")}


def test_gate_step_losses_are_observed_only_when_rejected(tmp_path):
    al, bob = node("Alice"), node("Bob")
    prev_f, prev_i = state(tmp_path, "a", [al, bob], [(al, "knows", bob, [1]), (al, "likes", bob, [1])])
    cur_raw, cur_i = state(tmp_path, "b", [al, bob])
    rejected = frozenset({f"R|{ids(al)}|{ids(bob)}|knows"})
    _, events, _ = transition(initial(prev_f), prev_i, cur_raw, cur_i, Step(1, "gate", rejected))
    assert lost(events) == {("relation", "gate", "observed"), ("relation", "unknown", "unknown")}


def test_gate_isolation_then_prune_is_flagged(tmp_path):
    dana, bob = node("Dana", {"hobby": ("chess", [8])}), node("Bob")
    pre_f, pre_i = state(tmp_path, "a", [dana, bob], [(dana, "mentors", bob, [6])])
    post_f, post_i = state(tmp_path, "b", [dana, bob])
    iso = isolated_by_gate(pre_i, post_i)
    assert ids(dana) in iso
    facts, _, _ = transition(initial(pre_f), pre_i, post_f, post_i,
                             Step(1, "gate", frozenset({f"R|{ids(dana)}|{ids(bob)}|mentors"})))
    cur_raw, cur_i = state(tmp_path, "c", [bob])
    _, events, _ = transition(facts, post_i, cur_raw, cur_i,
                              Step(2, "dream", report={"entities_pruned": 1}, isolated_by_gate=iso))
    e = next(e for e in events if e["event"] == "lost")
    assert (e["cause"], e["confidence"], e["after_gate_isolation"]) == ("prune", "inferred", True)


def test_descriptions_cover_field_bearing_entities_and_repeated_sentences(tmp_path):
    facts, _ = state(tmp_path, "a", [node("Bob", {"occupation": ("scientist", [4])},
                                          description="Bob is kind. Bob is kind. He paints.")])
    d = [f for f in facts if f.kind == "description"]
    assert len(d) == 3 and len({f.record_id for f in d}) == 3
    assert all(not f.gate_eligible and f.entity_has_fields and f.evidence_level == "entity" for f in d)
