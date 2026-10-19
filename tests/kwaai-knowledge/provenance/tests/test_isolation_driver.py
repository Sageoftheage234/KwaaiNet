import json

import pytest
import yaml

from provenance import checks
from provenance.driver import Kwaainet, KwaainetError
from provenance.kb import fingerprint, load_snapshot, make_snapshot, make_workspace
from provenance.retrieval_cost import gate_cost, parse_eval_report


def test_snapshot_copies_without_touching_original(kwaainet_home, tmp_path):
    _, data = kwaainet_home
    before = fingerprint(data)
    snap = make_snapshot("D6t", tmp_path / "s", "486bcf6")
    assert fingerprint(data) == before
    assert snap.meta["files"] == before
    assert "rag_data_dir" not in snap.meta["config_template"]["rag_kbs"]["D6t"]
    with pytest.raises(FileExistsError):
        make_snapshot("D6t", tmp_path / "s", "486bcf6")


def test_snapshot_refuses_overlap_and_remote_storage(kwaainet_home, tmp_path):
    home, data = kwaainet_home
    with pytest.raises(ValueError, match="overlap"):
        make_snapshot("D6t", data / "inside", "486bcf6")
    cfg = yaml.safe_load((home / "config.yaml").read_text())
    cfg["rag_kbs"]["D6t"]["storage_url"] = "http://eve:7432"
    (home / "config.yaml").write_text(yaml.safe_dump(cfg))
    with pytest.raises(ValueError, match="not local"):
        make_snapshot("D6t", tmp_path / "s2", "486bcf6")


def test_workspace_isolation_every_command_hits_the_copy(snapshot, fake_cmd, tmp_path, kwaainet_home):
    _, data = kwaainet_home
    ws = make_workspace(snapshot, tmp_path / "ws")
    cfg = yaml.safe_load((ws.home / "config.yaml").read_text())
    assert cfg["rag_kbs"]["D6t"]["rag_data_dir"] == str((tmp_path / "ws" / "data").resolve())
    assert str(data) not in (ws.home / "config.yaml").read_text()
    kw = Kwaainet(fake_cmd, timeout=60)
    before = fingerprint(data)
    assert kw.graph_stats(ws) == {"entities": 8, "relations": 6}
    assert kw.graph_score(ws)["relation_count"] == 6
    res = kw.dream_run(ws, "llama3.1:8b", None, {})
    assert res.report["entities_merged"] == 1 and res.report["entities_pruned"] == 1 and res.completed_anything
    assert kw.graph_stats(ws)["entities"] == 6          # merge + prune happened in the copy
    assert fingerprint(data) == before                   # original untouched
    load_snapshot(snapshot.root)                         # snapshot untouched (raises if modified)


def test_fake_binary_detects_a_leak(kwaainet_home, fake_cmd, snapshot, tmp_path, monkeypatch):
    ws = make_workspace(snapshot, tmp_path / "ws")
    monkeypatch.setenv("FAKE_FORBIDDEN", str(ws.data_dir))  # pretend the copy were the original
    with pytest.raises(KwaainetError, match="exit 97"):
        Kwaainet(fake_cmd).graph_stats(ws)


def test_driver_errors_skip_inference_and_stale_report(snapshot, fake_cmd, tmp_path, monkeypatch):
    ws = make_workspace(snapshot, tmp_path / "ws")
    kw = Kwaainet(fake_cmd)
    monkeypatch.setenv("FAKE_FAIL_DREAM", "1")
    with pytest.raises(KwaainetError, match="exit 2"):
        kw.dream_run(ws, "m", None, {})
    monkeypatch.delenv("FAKE_FAIL_DREAM")
    monkeypatch.setenv("FAKE_NO_COMPLETIONS", "1")
    res = kw.dream_run(ws, "m", None, {})
    assert not res.completed_anything and res.destructive_steps_skipped
    monkeypatch.delenv("FAKE_NO_COMPLETIONS")
    monkeypatch.setenv("FAKE_NO_REPORT", "1")
    assert ws.files.dream_report.exists()                # left over from the previous cycle
    with pytest.raises(KwaainetError, match="wrote no report"):
        kw.dream_run(ws, "m", None, {})                  # the stale report is deleted, never reused
    with pytest.raises(ValueError, match="unknown dream flag"):
        kw.dream_run(ws, "m", None, {"bogus": 1})


def test_dream_flags_forwarded(snapshot, fake_cmd, tmp_path):
    ws = make_workspace(snapshot, tmp_path / "ws")
    res = Kwaainet(fake_cmd).dream_run(ws, "llama3.1:8b", "http://x:11434",
                                       {"dedup_threshold": 1.01, "prune_threshold": 0.0, "no_relations": True,
                                        "max_completions": 7, "relation_summary": False})
    a = res.args
    assert a[:5] == ["rag", "dream", "run", "--kb", "D6t"]
    assert a[a.index("--dedup-threshold") + 1] == "1.01" and a[a.index("--prune-threshold") + 1] == "0.0"
    assert "--no-relations" in a and "--relation-summary" not in a and a[a.index("--inference-url") + 1] == "http://x:11434"
    assert res.report["entities_merged"] == 0 and res.report["entities_pruned"] == 0


def test_eval_parsing_both_formats_and_gate_cost():
    sem = "| Retrieval recall (token-overlap + semantic) | 72.5% (29.0/40) |\n| Generation recall (token-overlap + semantic) | 60.0% (24.0/40) |"
    tok = "| Overall recall (token-overlap) | 55.0% (22.0/40) |"
    a, b = parse_eval_report(sem), parse_eval_report(tok)
    assert a["retrieval_keyword_recall"] == pytest.approx(0.725) and a["generation_keyword_recall"] == pytest.approx(0.60)
    assert b["retrieval_keyword_recall"] is None and "not_computed" in b["retrieval_keyword_recall_note"]
    assert a["recall_at_k"].startswith("not_computed") and a["mrr"].startswith("not_computed")
    c = gate_cost(a, {"retrieval_keyword_recall": 0.8, "generation_keyword_recall": 0.6})
    assert c["retrieval_keyword_recall_delta"] == pytest.approx(-0.075)
    assert c["generation_keyword_recall_delta"] == pytest.approx(0.0)
    assert c["recall_at_k"].startswith("not_computed")
    with pytest.raises(ValueError):
        parse_eval_report("no table here")


def test_checks_pass_fail_inconclusive(kwaainet_home, fake_cmd, tmp_path, monkeypatch):
    out = tmp_path / "snap.json"
    r = checks.check_snapshot("D6t", str(tmp_path / "s"), "486bcf6", " ".join(fake_cmd), str(out))
    assert r["status"] == "PASS" and (tmp_path / "s" / "SNAPSHOT.md").exists()
    assert checks.check_binary(fake_cmd, str(tmp_path / "s"), str(tmp_path / "b"), None)["status"] == "PASS"
    e = checks.check_edit_compat(fake_cmd, str(tmp_path / "s"), str(tmp_path / "e"), "m", None, str(tmp_path / "e.json"))
    assert e["status"] == "PASS" and e["stats_after"]["relations"] == e["stats_before"]["relations"] - 1
    steps = " ".join(e["steps"])
    assert "graph score --json" in steps and "field" in steps and "relation" in steps
    assert e["binding"]["protocol"] == "edit-compat/2" and e["binding"]["snapshot_id"] == r["snapshot_id"]
    assert e["binding"]["binary_files"] and e["binding"]["writer_code_sha256"]
    monkeypatch.setenv("FAKE_NO_COMPLETIONS", "1")
    e2 = checks.check_edit_compat(fake_cmd, str(tmp_path / "s"), str(tmp_path / "e2"), "m", None, None)
    assert e2["status"] == "INCONCLUSIVE"
    monkeypatch.setenv("FAKE_FAIL_DREAM", "1")
    assert checks.check_edit_compat(fake_cmd, str(tmp_path / "s"), str(tmp_path / "e3"), "m", None, None)["status"] == "INCONCLUSIVE"
    assert checks.check_snapshot("NOPE", str(tmp_path / "s3"), "486bcf6", None, None)["status"] == "FAIL"


def test_edit_check_fails_when_the_binary_does_not_see_the_edit(snapshot, fake_cmd, tmp_path):
    """A binary that keeps reporting the first counts it saw (e.g. reads a stale cache) must not PASS."""
    shim, cache = tmp_path / "stale.py", tmp_path / "cache.json"
    shim.write_text("import json, os, subprocess, sys\n"
                    f"cp = subprocess.run([sys.executable, {str(fake_cmd[1])!r}] + sys.argv[1:], capture_output=True, text=True)\n"
                    "out = cp.stdout\n"
                    "if sys.argv[1:4] == ['rag', 'graph', 'score']:\n"
                    f"    c = {str(cache)!r}\n"
                    "    if os.path.exists(c): out = open(c).read()\n"
                    "    else: open(c, 'w').write(out)\n"
                    "sys.stdout.write(out); sys.stderr.write(cp.stderr); sys.exit(cp.returncode)\n")
    e = checks.check_edit_compat([fake_cmd[0], str(shim)], str(snapshot.root), str(tmp_path / "e"), "m", None, None)
    assert e["status"] == "FAIL" and "graph score did not see the edit" in e["reason"]


def test_quoted_windows_style_command_paths_are_unquoted():
    k = Kwaainet('"C:\\Program Files\\kwaainet\\kwaainet.exe" --flag')
    assert k.cmd == ["C:\\Program Files\\kwaainet\\kwaainet.exe", "--flag"]
