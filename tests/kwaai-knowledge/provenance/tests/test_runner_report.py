import csv
import json
import shutil

import pytest

from provenance import checks, cli
from provenance.compat import authorize, runtime_binding
from provenance.config import Config, load_config
from provenance.driver import Kwaainet
from provenance.kb import fingerprint
from provenance.labeling import import_labels
from provenance.metrics import bootstrap_ci, cohens_kappa, false_rejection_rate
from provenance.report import build, describe
from provenance.runner import run


def cfg(**kw):
    base = dict(repeats=1, snapshots=[0, 1, 2], scorer="lexical")
    base.update(kw)
    return Config(**base)


def summaries(path):
    return [r for r in (json.loads(l) for l in path.read_text().splitlines()) if r["record_type"] == "summary"]


def ok_rows(path):
    return {(r["arm"], r["repeat"], r["cycle"], r["phase"]): r for r in summaries(path) if r["status"] == "ok"}


@pytest.fixture
def edit_ok(snapshot, kw, tmp_path):
    p = tmp_path / "edit.json"
    e = checks.check_edit_compat(kw.cmd, str(snapshot.root), str(tmp_path / "edit"), "m", None, str(p))
    assert e["status"] == "PASS"
    return p


def test_without_edit_check_gate_after_is_virtual_and_gate_before_not_computed(snapshot, kw, tmp_path, kwaainet_home):
    _, data = kwaainet_home
    before = fingerprint(data)
    res = run(cfg(), snapshot.root, tmp_path / "out", "r1", kw, max_cycle=2)
    rows = summaries(res)
    gb = [r for r in rows if r["arm"] == "gate_before"]
    assert gb and all(r["status"] == "not_computed" and "edit-compatibility" in r["reason"] for r in gb)
    ga = [r for r in rows if r["arm"] == "gate_after" and r["cycle"] > 0]
    assert ga and {r["phase"] for r in ga} == {"post_dream"} and all(r["gate_mode"] == "virtual" for r in ga)
    v = ga[0]["virtual_gate_immediate"]
    assert "not evidence about later cycles" in v["interpretation"] and v["rejected"] > 0
    assert "virtual_gate" not in ga[0] and "unsupported_present_over_cycle0_if_removed_now" in v
    decs = [json.loads(l) for l in (res.parent / "gate_decisions.jsonl").read_text().splitlines()]
    assert decs and not any(d["written"] for d in decs)
    assert fingerprint(data) == before
    with pytest.raises(FileExistsError):
        run(cfg(), snapshot.root, tmp_path / "out", "r1", kw, max_cycle=1)


def test_every_phase_is_recorded_and_one_row_per_cycle_is_primary(snapshot, kw, tmp_path, edit_ok):
    res = run(cfg(edit_compat_result=str(edit_ok)), snapshot.root, tmp_path / "out", "r2", kw, max_cycle=2)
    rows = ok_rows(res)
    phases = lambda arm, k: {p for (a, _, c, p) in rows if a == arm and c == k}  # noqa: E731
    assert phases("gate_before", 1) == {"pre_gate", "post_gate", "post_dream"}
    assert phases("gate_after", 1) == {"post_dream", "post_gate"}
    assert phases("ungated", 1) == {"post_dream"}
    for arm in ("ungated", "gate_before", "gate_after"):
        for k in (0, 1, 2):
            assert sum(r["primary"] for (a, _, c, _), r in rows.items() if a == arm and c == k) == 1
            assert sum(r["end_of_cycle"] for (a, _, c, _), r in rows.items() if a == arm and c == k) == 1
    ga_dream, ga_gate = rows[("gate_after", 0, 1, "post_dream")], rows[("gate_after", 0, 1, "post_gate")]
    assert ga_dream["primary"] and ga_dream["is_pre_gate_state"] and not ga_dream["end_of_cycle"]
    assert ga_gate["end_of_cycle"] and not ga_gate["primary"]
    assert rows[("gate_before", 0, 1, "post_dream")]["primary"] and rows[("gate_before", 0, 1, "post_dream")]["end_of_cycle"]
    # Immediate gate effect is reported separately from the carried-over effect.
    gi = ga_gate["gate_immediate"]
    assert gi["rejected"] == gi["removed_relations"] + gi["removed_fields"] > 0
    assert ga_gate["unsupported_present_over_cycle0"] == 0.0
    # The artefact the audit warns about: right after its own gate, gate_after looks clean; after the next
    # dream it has regenerated unsupported content. The primary (post_dream) row shows the latter.
    assert rows[("gate_after", 0, 2, "post_dream")]["unsupported_present_over_cycle0"] > ga_gate["unsupported_present_over_cycle0"]
    res2 = run(cfg(edit_compat_result=str(edit_ok), primary_phase="end_of_cycle"), snapshot.root, tmp_path / "out",
               "r2b", kw, max_cycle=1)
    rows2 = ok_rows(res2)
    assert rows2[("gate_after", 0, 1, "post_gate")]["primary"] and not rows2[("gate_after", 0, 1, "post_dream")]["primary"]


def test_attribution_end_to_end_including_gate_induced_prune(snapshot, kw, tmp_path, edit_ok):
    res = run(cfg(edit_compat_result=str(edit_ok)), snapshot.root, tmp_path / "out", "r3", kw, max_cycle=2)
    rows = ok_rows(res)
    u1 = rows[("ungated", 0, 1, "post_dream")]
    # Groves.project dropped by merge; Ghost.planet pruned; Alice parent_of Acme removed by sanitize.
    assert u1["loss_attribution"] == {"merge|inferred": 1, "prune|inferred": 1, "sanitize|inferred": 1}
    assert u1["lineage_totals"]["rewritten"] == 2 and u1["lineage_totals"]["consolidated"] == 1
    assert u1["lost"] == 3 and u1["added"] == 1 and u1["added_unsupported"] == 1
    gb_post = rows[("gate_before", 0, 1, "post_gate")]
    assert set(gb_post["loss_attribution"]) == {"gate|observed"}
    gb1 = rows[("gate_before", 0, 1, "post_dream")]
    assert gb1["lost_to_prune_after_gate_isolation"] == 1             # Dana.hobby: supported, lost because the gate isolated Dana
    assert gb1["loss_attribution"]["prune|inferred"] >= 1
    assert rows[("no_similarity_merge", 0, 1, "post_dream")]["lineage_totals"].get("rewritten", 0) == 0
    assert "prune|inferred" not in rows[("no_prune", 0, 1, "post_dream")]["loss_attribution"]
    events = [json.loads(l) for l in (res.parent / "events.jsonl").read_text().splitlines()]
    assert all(e["confidence"] in ("observed", "inferred", "ambiguous", "unknown") for e in events)
    assert not [e for e in events if e["arm"] == "ungated" and e["event"] == "lost" and e["confidence"] == "unknown"]


def test_value_rewrite_is_attributed_as_observed(snapshot, kw, tmp_path, monkeypatch):
    monkeypatch.setenv("FAKE_REWRITE_FIELD", "1")
    res = run(cfg(arms=[{"name": "ungated", "gate": "none", "flags": {}}]), snapshot.root, tmp_path / "out", "r4",
              kw, max_cycle=1)
    r = ok_rows(res)[("ungated", 0, 1, "post_dream")]
    assert r["loss_attribution"]["value_rewritten|observed"] == 1


def test_descriptions_are_reported_separately_and_never_counted_as_supported(snapshot, kw, tmp_path):
    res = run(cfg(arms=[{"name": "ungated", "gate": "none", "flags": {}}]), snapshot.root, tmp_path / "out", "r5",
              kw, max_cycle=1)
    r0 = ok_rows(res)[("ungated", 0, 0, "initial")]
    assert r0["cycle0_facts"] == 12 and sum(r0["counts"].values()) == r0["present_facts"] == 12
    d = r0["descriptions"]
    assert d["cycle0_facts"] == 3 and d["gated"] is False and d["evidence_level"] == "entity"
    assert d["entities_with_description"] == 2 and d["entities"] == 8
    assert sum(d["counts"].values()) == d["present_facts"] == 3


def test_report_stats_cover_all_repeats_and_eval_runs_at_every_snapshot(snapshot, kw, tmp_path):
    res = run(cfg(repeats=2, eval_enabled=True, eval_questions="q.json"), snapshot.root, tmp_path / "out", "r6",
              kw, max_cycle=2)
    out = tmp_path / "r.csv"
    build(res, out, tmp_path / "plots")
    rows = list(csv.DictReader(out.open()))
    ev = [r for r in rows if r["status"] == "ok" and r["retrieval_keyword_recall"] != ""]
    assert {(r["arm"], r["cycle"]) for r in ev if r["arm"] == "ungated"} == {("ungated", "0"), ("ungated", "1"), ("ungated", "2")}
    assert all(r["end_of_cycle"] == "True" for r in ev)
    assert all(r["recall_at_k"].startswith("not_computed") for r in ev)
    assert any(r["retrieval_keyword_recall_delta_vs_ungated_same_repeat_index"] != "" for r in ev if r["arm"] != "ungated")
    stats = list(csv.DictReader((tmp_path / "r.stats.csv").open()))
    prim = [s for s in stats if s["metric"] == "unsupported_present_over_cycle0" and s["primary"] == "True"]
    assert prim and all(s["n_repeats"] == "2" and len(json.loads(s["values"])) == 2 for s in prim)
    assert any(s["metric"].endswith("_difference_of_means_vs_ungated") for s in stats)
    assert (tmp_path / "plots" / "primary-endpoint.png").exists()
    assert (tmp_path / "plots" / "keyword-recall-delta.png").exists()
    meta = json.loads((tmp_path / "r.csv.metadata.json").read_text())
    assert meta["snapshot_id"] == snapshot.snapshot_id and "expected keywords" in json.dumps(meta["keyword_recall_definitions"])
    assert all(r["scorer_status"] == "NOT FOR REPORTING" for r in rows)


def test_describe_uses_t_interval():
    d = describe([0.1, 0.2, 0.3])
    assert d["n"] == 3 and d["mean"] == pytest.approx(0.2) and d["sd"] == pytest.approx(0.1)
    assert d["ci95_high"] - d["mean"] == pytest.approx(4.303 * 0.1 / 3 ** 0.5)
    assert describe([0.5])["ci95_low"] is None


def test_authorization_is_bound_to_the_runtime(snapshot, kw, tmp_path, edit_ok, fake_cmd):
    binding = runtime_binding(kw, snapshot)
    assert authorize(str(edit_ok), binding)[0]
    other = tmp_path / "other_fake.py"
    shutil.copy(fake_cmd[1], other)
    other.write_text(other.read_text() + "\n# a different build\n")
    ok, why = authorize(str(edit_ok), runtime_binding(Kwaainet([fake_cmd[0], str(other)], 60), snapshot))
    assert not ok and "binary_files" in why
    stored = json.loads(edit_ok.read_text())
    for key, val in (("protocol", "edit-compat/1"), ("snapshot_files_sha256", "0" * 64), ("writer_code_sha256", "x")):
        bad = tmp_path / f"bad-{key}.json"
        bad.write_text(json.dumps({**stored, "binding": {**stored["binding"], key: val}}))
        ok, why = authorize(str(bad), binding)
        assert not ok and key in why
    legacy = tmp_path / "legacy.json"
    legacy.write_text(json.dumps({"status": "PASS", "snapshot_id": snapshot.snapshot_id}))
    assert "older protocol" in authorize(str(legacy), binding)[1]
    res = run(cfg(edit_compat_result=str(edit_ok)), snapshot.root, tmp_path / "out", "r7",
              Kwaainet([fake_cmd[0], str(other)], 60), max_cycle=1)
    assert all(r["status"] == "not_computed" for r in summaries(res) if r["arm"] == "gate_before")


def test_label_export_deduplicates_repeated_judgements(snapshot, kw, tmp_path):
    res = run(cfg(), snapshot.root, tmp_path / "out", "r8", kw, max_cycle=2)
    out, key = tmp_path / "l.csv", tmp_path / "k.csv"
    cli.main(["label-export", "--run", str(res.parent), "--meta-db", str(snapshot.data_dir / next(
        p.name for p in snapshot.data_dir.iterdir() if p.name.endswith(".db") and not p.name.startswith("graph-"))),
              "--out", str(out), "--key", str(key), "--n", "150"])
    keys = list(csv.DictReader(key.open()))
    assert keys and len({(k["record_id"], k["accepted"]) for k in keys}) == len(keys)


def test_config_validation(tmp_path):
    p = tmp_path / "c.yaml"
    for text, msg in (("bogus_key: 1\n", "unknown config keys"), ("arms: [{name: x, gate: sideways}]\n", "gate must be"),
                      ("primary_phase: whenever\n", "primary_phase"), ("snapshots: [1, 2]\n", "cycle 0"),
                      ("eval_cycles: [5]\n", "subset")):
        p.write_text(text)
        with pytest.raises(ValueError, match=msg):
            load_config(p)


def test_metrics_and_labeling(tmp_path):
    assert false_rejection_rate(["supported", "supported", "not_supported"], [False, True, False]) == 0.5
    assert cohens_kappa(["a", "a", "b", "b"], ["a", "a", "b", "b"]) == 1
    lo, hi = bootstrap_ci([0, 1, 1, 0], lambda x: sum(x) / len(x), seed=7, n_boot=100)
    assert 0 <= lo <= hi <= 1
    key, lab = tmp_path / "k.csv", tmp_path / "l.csv"
    key.write_text("sample_id,accepted,record_id\nS1,False,a\nS2,True,b\nS3,False,c\n")
    lab.write_text("sample_id,label\nS1,supported\nS2,supported\nS3,not_supported\n")
    s = import_labels(lab, key, lab, 7)
    assert s["false_rejection_rate"] == 0.5 and s["gate_precision"] == 0.5 and s["cohens_kappa"] == 1.0


def test_a_gate_write_that_does_not_land_aborts_the_run(snapshot, kw, tmp_path, edit_ok, monkeypatch):
    from provenance import gate as gate_mod
    monkeypatch.setattr(gate_mod, "apply", lambda decisions, db, protected=(): {"relations": 0, "fields": 0})
    with pytest.raises(RuntimeError, match="gate write mismatch"):
        run(cfg(edit_compat_result=str(edit_ok), arms=[{"name": "gate_before", "gate": "before", "flags": {}}]),
            snapshot.root, tmp_path / "out", "r9", kw, max_cycle=1)
