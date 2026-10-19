"""Machine-side checks. Each prints PASS/FAIL/INCONCLUSIVE and writes a JSON result. None touches the original KB."""
from __future__ import annotations

import datetime as _dt
import json
from pathlib import Path

from .driver import Kwaainet, KwaainetError
from .kb import load_snapshot, make_snapshot, make_workspace
from .compat import runtime_binding
from .kwaainet_store import GraphReader, GraphWriter, dumps, loads


def _result(status: str, reason: str, **extra) -> dict:
    return {"status": status, "reason": reason, "checked_utc": _dt.datetime.now(_dt.timezone.utc).isoformat(), **extra}


def _write(path, res: dict) -> dict:
    if path:
        Path(path).write_text(json.dumps(res, indent=2, default=str), encoding="utf-8")
    print(f"{res['status']}: {res['reason']}")
    return res


def check_snapshot(kb: str, dest: str, freeze_commit: str, kwaainet_cmd: str | None, out_json: str | None,
                   home: str | None = None) -> dict:
    version = None
    if kwaainet_cmd:
        try:
            version = Kwaainet(kwaainet_cmd).version()
        except KwaainetError as e:
            return _write(out_json, _result("FAIL", f"binary not usable: {e}"))
    try:
        snap = make_snapshot(kb, Path(dest), freeze_commit, home=home, binary_version=version)
    except Exception as e:  # noqa: BLE001 - report every refusal as FAIL with its reason
        return _write(out_json, _result("FAIL", f"{type(e).__name__}: {e}"))
    md = (f"# Snapshot {snap.snapshot_id}\n\n- KB: `{kb}` (tenant `{snap.tenant_id}`)\n"
          f"- Freeze commit: `{freeze_commit}`\n- kwaainet version: `{version}`\n"
          f"- Source data dir: `{snap.meta['source_data_dir']}` (read only)\n- Created: {snap.meta['created_utc']}\n"
          f"- Files: {len(snap.meta['files'])} (SHA-256 per file in SNAPSHOT.json)\n")
    (Path(dest) / "SNAPSHOT.md").write_text(md, encoding="utf-8")
    return _write(out_json, _result("PASS", f"snapshot {snap.snapshot_id} written to {dest}", snapshot_id=snap.snapshot_id))


def check_binary(kwaainet_cmd: str, snapshot_root: str, scratch: str, out_json: str | None) -> dict:
    kw = Kwaainet(kwaainet_cmd, timeout=600)
    try:
        version = kw.version()
        snap = load_snapshot(Path(snapshot_root))
        ws = make_workspace(snap, Path(scratch) / "check-binary")
        stats = kw.graph_stats(ws)
    except Exception as e:  # noqa: BLE001
        return _write(out_json, _result("FAIL", f"{type(e).__name__}: {e}"))
    if not stats.get("entities"):
        return _write(out_json, _result("FAIL", f"KB copy loaded but graph stats reported no entities: {stats}"))
    return _write(out_json, _result("PASS", f"{version}; KB copy loads ({stats})", version=version,
                                    snapshot_id=snap.snapshot_id, stats=stats))


def _entity_ids_in_score(score: dict) -> set[int]:
    return {int(e["entity_id"]) for e in score.get("entity_scores", []) if "entity_id" in e}


def check_edit_compat(kwaainet_cmd: str, snapshot_root: str, scratch: str, model: str,
                      inference_url: str | None, out_json: str | None) -> dict:
    """Edit a copy exactly as the gate does (one field removal, one relation deletion) and confirm the binary
    reads it with ``rag graph stats``, ``rag graph score --json`` and a one-completion dream cycle.

    PASS is bound to this snapshot, binary, config template, protocol and writer code (compat.py)."""
    kw = Kwaainet(kwaainet_cmd, timeout=1800)
    snap = load_snapshot(Path(snapshot_root))
    sid = snap.snapshot_id
    steps: list[str] = []

    def done(status, reason, **extra):
        return _write(out_json, _result(status, reason, snapshot_id=sid, steps=steps, **extra))

    try:
        binding = runtime_binding(kw, snap)
        ws = make_workspace(snap, Path(scratch) / "check-edit-compat")
        g = ws.files.graph_db
        ents = {k: (v, raw) for k, v, raw in GraphReader(g).entities()}
        rels = {k: (v, raw) for k, v, raw in GraphReader(g).relations()}
        bad = [k for k, (_, raw) in {**ents, **rels}.items() if dumps(loads(raw)) != raw]
        if bad:
            return done("FAIL", f"{len(bad)} records do not round-trip byte-identically")
        steps.append(f"round-trip: {len(ents)} entities and {len(rels)} relations byte-identical")
        stats0, score0 = kw.graph_stats(ws), kw.graph_score(ws)
        if (score0.get("entity_count"), score0.get("relation_count")) != (len(ents), len(rels)):
            return done("FAIL", f"binary counts {score0.get('entity_count')}/{score0.get('relation_count')} differ "
                                f"from this reader's {len(ents)}/{len(rels)}")
        steps.append("graph stats and graph score --json read the unedited copy; counts match this reader")
        rel_target = next(iter(rels), None)
        field_target = next(((k, next(iter(v["fields"]))) for k, (v, _) in ents.items() if v.get("fields")), None)
        if rel_target is None or field_target is None:
            return done("FAIL", "the graph needs at least one relation and one entity field to test both edit types")
        w = GraphWriter(g, ws.protected)
        w.rewrite_unchanged()
        w.delete_relation(*rel_target)
        if not w.remove_field(*field_target):
            w.commit_close()
            return done("FAIL", f"could not remove field {field_target}")
        w.commit_close()
        ents1 = {k: (v, raw) for k, v, raw in GraphReader(g).entities()}
        rels1 = {k for k, _, _ in GraphReader(g).relations()}
        eid, fname = field_target
        if rel_target in rels1 or fname in (ents1[eid][0].get("fields") or {}):
            return done("FAIL", "this reader still sees an edited record")
        untouched = [k for k in ents if k != eid and ents1[k][1] != ents[k][1]]
        if untouched or set(rels) - {rel_target} != rels1:
            return done("FAIL", "the edit changed records other than the two targets")
        steps.append(f"edited copy: relation {rel_target} deleted, field {fname!r} removed from entity {eid}; "
                     f"all other records byte-identical")
        stats1, score1 = kw.graph_stats(ws), kw.graph_score(ws)
        if stats0.get("relations") is not None and stats1.get("relations") != stats0["relations"] - 1:
            return done("FAIL", f"graph stats did not see the relation deletion: {stats0} -> {stats1}")
        if score1.get("relation_count") != len(rels) - 1 or score1.get("entity_count") != len(ents):
            return done("FAIL", f"graph score did not see the edit: {score0.get('relation_count')} -> "
                                f"{score1.get('relation_count')} relations")
        if eid not in _entity_ids_in_score(score1):
            return done("FAIL", f"graph score --json does not list the field-edited entity {eid}")
        steps.append("graph stats and graph score --json read the edited copy, see both edits, and list the "
                     "field-edited entity")
    except Exception as e:  # noqa: BLE001
        return done("FAIL", f"{type(e).__name__}: {e}")
    try:
        dream = kw.dream_run(ws, model, inference_url, {"max_completions": 1})
    except KwaainetError as e:
        return done("INCONCLUSIVE", f"the binary reads the edited graph, but the dream run failed: {e}",
                    stats_before=stats0, stats_after=stats1)
    try:
        ents2 = {k: v for k, v, _ in GraphReader(g).entities()}
        rels2 = {k for k, _, _ in GraphReader(g).relations()}
        score2 = kw.graph_score(ws)
    except Exception as e:  # noqa: BLE001
        return done("FAIL", f"graph unreadable after the dream run: {type(e).__name__}: {e}")
    after = {"field_entity_present": eid in ents2,
             "field_readded": eid in ents2 and fname in (ents2[eid].get("fields") or {}),
             "relation_readded": rel_target in rels2,
             "score_entity_count": score2.get("entity_count"), "score_relation_count": score2.get("relation_count")}
    if (score2.get("entity_count"), score2.get("relation_count")) != (len(ents2), len(rels2)):
        return done("FAIL", "after the dream run the binary and this reader disagree on graph counts", after=after)
    steps.append(f"after a one-completion dream run both this reader and graph score --json read the graph: {after}")
    if not dream.completed_anything:
        return done("INCONCLUSIVE", "dream ran on the edited graph but completed nothing; check the inference "
                                    "endpoint and retry", report=dream.report, after=after)
    return done("PASS", "the binary reads a field edit and a relation edit through graph stats, graph score "
                        "--json and a dream cycle", stats_before=stats0, stats_after=stats1, report=dream.report,
                after=after, binding=binding)
