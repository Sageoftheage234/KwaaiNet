"""Arms × repeats × cycles over isolated copies of one shared KB snapshot.

Every cycle is a sequence of operations on the arm's own copy:
    ungated / ablations:  dream
    gate_before:          gate -> dream
    gate_after:           dream -> gate      (virtual: dream -> decisions logged, nothing written)
The graph is re-read after every operation and each transition is attributed (lineage.py), so causes are
inferred from adjacent states. At reporting snapshots a summary row is written for every distinct state
(audit item 2):
    pre_gate   - before the gate (gate_before only; for gate_after the post_dream state is the pre-gate state)
    post_gate  - immediately after a written gate
    post_dream - immediately after the dream run
``primary`` marks the preregistered observation point (config.primary_phase). The default, post_dream,
compares every arm right after the same operation, so a gate_after arm is never measured in the instant
after its own clean-up; end_of_cycle compares each protocol's final state. Both are always recorded.
"""
from __future__ import annotations

import datetime as _dt
import hashlib
import json
from collections import Counter
from dataclasses import asdict
from pathlib import Path

from . import gate as gate_mod
from .compat import authorize, runtime_binding
from .config import Config, eval_cycles, make_scorer, validate
from .driver import Kwaainet
from .facts import extract_facts
from .kb import fingerprint, load_snapshot, make_workspace
from .kwaainet_store import MetaReader
from .lineage import Step, initial, isolated_by_gate, transition
from .support import SupportModel

CLASSES = ("traceable", "misattributed", "unsupported", "unlinked")
OPS = {"none": ("dream",), "before": ("gate", "dream"), "after": ("dream", "gate")}
GATE_ELIGIBLE = ("field", "relation")


def _run_id(cfg: Config) -> str:
    stamp = _dt.datetime.now(_dt.timezone.utc).strftime("%Y%m%dT%H%M%SZ")
    h = hashlib.sha256(json.dumps(cfg.as_dict(), sort_keys=True, default=str).encode()).hexdigest()[:8]
    return f"{stamp}-{h}"


class _Arm:
    def __init__(self, spec: dict, cfg: Config, edit_ok: bool):
        self.name = spec["name"]
        self.gate = spec.get("gate", "none")
        self.flags = {**cfg.dream_flags, **spec.get("flags", {})}
        if self.gate == "none":
            self.mode = "none"
        elif edit_ok:
            self.mode = "written"
        else:
            self.mode = "virtual" if self.gate == "after" else "not_computed"


def _group_metrics(facts, cls, c0: set[str], lost_events: dict) -> dict:
    """Metrics for one group of facts against its cycle-0 origins."""
    present = {o for f in facts for o in f.origin_ids}
    n0 = len(c0)
    unsupported = [f for f in facts if cls[f.record_id] == "unsupported"]
    traceable = [f for f in facts if cls[f.record_id] == "traceable"]
    added = [f for f in facts if not set(f.origin_ids) & c0]
    lost = c0 - present
    bad = {o for f in unsupported for o in f.origin_ids if o in c0} | lost
    attribution: Counter = Counter()
    gate_isolation = 0
    for o in lost:
        e = lost_events.get(o)
        if e is None:
            attribution[("unknown", "unknown")] += 1
            continue
        attribution[(e["cause"], e["confidence"])] += 1
        gate_isolation += bool(e.get("after_gate_isolation"))
    cids = Counter(f.content_id for f in facts)
    n = len(facts)
    return {"cycle0_facts": n0, "present_facts": n, "cycle0_origins_present": len(present & c0),
            "lost": len(lost), "added": len(added),
            "added_unsupported": sum(cls[f.record_id] == "unsupported" for f in added),
            "unsupported_present_over_cycle0": len(unsupported) / n0 if n0 else None,
            "unsupported_over_present": len(unsupported) / n if n else None,
            "traceable_over_present": len(traceable) / n if n else None,
            "lost_over_cycle0": len(lost) / n0 if n0 else None,
            "unsupported_or_lost_over_cycle0": len(bad) / n0 if n0 else None,
            "counts": {c: sum(cls[f.record_id] == c for f in facts) for c in CLASSES},
            "loss_attribution": {f"{c}|{k}": v for (c, k), v in sorted(attribution.items())},
            "lost_to_prune_after_gate_isolation": gate_isolation,
            "records_sharing_content": sum(v for v in cids.values() if v > 1)}


def run(cfg: Config, snapshot_root: str | Path, out_root: str | Path, run_id: str | None = None,
        kwaainet: Kwaainet | None = None, max_cycle: int | None = None) -> Path:
    validate(cfg)
    snap = load_snapshot(Path(snapshot_root))
    out = Path(out_root) / (run_id or _run_id(cfg))
    out.mkdir(parents=True, exist_ok=False)
    results = out / "results.jsonl"
    kw = kwaainet or Kwaainet(cfg.kwaainet_cmd, cfg.subprocess_timeout_s)
    scorer = make_scorer(cfg)
    binding = runtime_binding(kw, snap)
    edit_ok, edit_reason = authorize(cfg.edit_compat_result, binding)
    original = Path(snap.meta["source_data_dir"])
    orig_fp = fingerprint(original) if cfg.verify_original_unchanged and original.exists() else None
    maxc = max_cycle if max_cycle is not None else max(cfg.snapshots)
    snaps = [k for k in cfg.snapshots if k <= maxc]
    evals = set(eval_cycles(cfg)) if cfg.eval_enabled else set()
    base = {"git_commit": cfg.freeze_commit, "snapshot_id": snap.snapshot_id, "run_id": out.name,
            "config": cfg.as_dict(), "scorer_name": scorer.name, "edit_compat": edit_reason,
            "primary_phase": cfg.primary_phase}
    (out / "RUN.json").write_text(json.dumps({**base, "runtime_binding": binding}, indent=2, default=str),
                                  encoding="utf-8")

    def emit(fh, row):
        fh.write(json.dumps(row, default=str) + "\n")
        fh.flush()

    with open(results, "x", encoding="utf-8") as fh, \
            open(out / "cycles.jsonl", "x", encoding="utf-8") as cyc, \
            open(out / "events.jsonl", "x", encoding="utf-8") as evfh, \
            open(out / "gate_decisions.jsonl", "x", encoding="utf-8") as gfh:
        for rep in range(cfg.repeats):
            for spec in cfg.arms:
                arm = _Arm(spec, cfg, edit_ok)
                tag = {"arm": arm.name, "repeat": rep}
                if arm.mode == "not_computed":
                    for k in snaps:
                        emit(fh, {**base, **tag, "record_type": "summary", "cycle": k, "phase": "all",
                                  "status": "not_computed", "gate": arm.gate, "gate_mode": arm.mode,
                                  "reason": edit_reason})
                    continue
                ws = make_workspace(snap, out / "workspaces" / f"{arm.name}-r{rep}")
                model = SupportModel(MetaReader(ws.files.meta_db), scorer, cfg.theta, cfg.corpus_top_n,
                                     cfg.max_unresolved_evidence_share)
                raw, info = extract_facts(ws.files.graph_db)
                facts = initial(raw)
                c0 = {g: {o for f in facts if (f.kind in GATE_ELIGIBLE) == (g == "gate_eligible") for o in f.origin_ids}
                      for g in ("gate_eligible", "description")}
                lost_events: dict[str, dict] = {}
                totals: Counter = Counter()
                isolated: frozenset = frozenset()
                st = {"facts": facts, "info": info}

                def record(evs):
                    for e in evs:
                        emit(evfh, {**tag, **e})
                        totals[e["event"]] += 1
                        if e["event"] == "lost":
                            for o in e["origin_ids"]:
                                lost_events.setdefault(o, e)

                def measure(k, phase, end, extra=None):
                    fs = st["facts"]
                    sup = model.score(fs)
                    cls = {s.record_id: s.support_class for s in sup}
                    ge = [f for f in fs if f.kind in GATE_ELIGIBLE]
                    de = [f for f in fs if f.kind == "description"]
                    primary = k == 0 or (phase == "post_dream" if cfg.primary_phase == "post_dream" else end)
                    row = {**base, **tag, "record_type": "summary", "status": "ok", "cycle": k, "phase": phase,
                           "end_of_cycle": end, "primary": primary, "gate": arm.gate, "gate_mode": arm.mode,
                           "flags": arm.flags,
                           **_group_metrics(ge, cls, c0["gate_eligible"], lost_events),
                           "lineage_totals": dict(totals),
                           "descriptions": {**_group_metrics(de, cls, c0["description"], lost_events),
                                            "evidence_level": "entity", "gated": False,
                                            "entities_with_description": len({f.entity_ids[0] for f in de}),
                                            "entities": len(st["info"]),
                                            "sentences_on_field_bearing_entities": sum(bool(f.entity_has_fields) for f in de)},
                           **(extra or {})}
                    if end and k in evals:
                        row["eval"] = kw.rag_eval(ws, cfg.eval_questions, out / f"eval-{arm.name}-r{rep}-c{k}.md",
                                                  cfg.model, cfg.inference_url, cfg.eval_k, cfg.eval_mode,
                                                  cfg.eval_semantic_score)
                    for f, s in zip(fs, sup):
                        emit(fh, {"record_type": "fact", "run_id": out.name, **tag, "cycle": k, "phase": phase,
                                  "fact": asdict(f), "support": asdict(s)})
                    emit(fh, row)
                    return row

                def decide(k):
                    sup = model.score(st["facts"])
                    dec = gate_mod.decide(st["facts"], sup, cfg.theta_gate)
                    cls = {s_.record_id: s_.support_class for s_ in sup}
                    for d in dec:
                        emit(gfh, {**tag, "cycle": k,
                                   "phase": "before_dream" if arm.gate == "before" else "after_dream",
                                   "written": arm.mode == "written", "support_class": cls[d.record_id], **asdict(d)})
                    rejected = {d.record_id for d in dec if not d.accepted}
                    immediate = {"rejected": len(rejected),
                                 "rejected_by_class": dict(Counter(cls[r] for r in rejected))}
                    return dec, rejected, cls, immediate

                def virtual(k):
                    _, rejected, cls, immediate = decide(k)
                    n0 = len(c0["gate_eligible"])
                    left = sum(1 for f in st["facts"] if f.kind in GATE_ELIGIBLE
                               and cls[f.record_id] == "unsupported" and f.record_id not in rejected)
                    return {**immediate,
                            "unsupported_present_over_cycle0_if_removed_now": left / n0 if n0 else None,
                            "interpretation": "immediate counterfactual: what this one gate would remove from this "
                                              "state. Nothing was written and every cycle of this arm ran ungated, "
                                              "so this is not evidence about later cycles."}

                if 0 in snaps:
                    measure(0, "initial", True)
                for k in range(1, maxc + 1):
                    snap_k = k in snaps
                    ops = OPS[arm.gate]
                    for i, op in enumerate(ops):
                        end = i == len(ops) - 1
                        if op == "dream":
                            dream = kw.dream_run(ws, cfg.model, cfg.inference_url, arm.flags)
                            emit(cyc, {**tag, "cycle": k, "args": dream.args, "report": dream.report,
                                       "completed_anything": dream.completed_anything,
                                       "destructive_steps_skipped": dream.destructive_steps_skipped,
                                       "notes": dream.notes})
                            raw, info = extract_facts(ws.files.graph_db)
                            facts, evs, _ = transition(st["facts"], st["info"], raw, info,
                                                       Step(k, "dream", report=dream.report, isolated_by_gate=isolated))
                            record(evs)
                            isolated = frozenset(e for e in isolated if info.get(e, {}).get("degree", 1) == 0)
                            st.update(facts=facts, info=info)
                            extra = {"dream_report": dream.report,
                                     "destructive_steps_skipped": dream.destructive_steps_skipped}
                            if arm.mode == "virtual":
                                v = virtual(k)  # decisions are logged every cycle for the labelling sample
                                if snap_k:
                                    extra["virtual_gate_immediate"] = v
                            if arm.gate == "after" and arm.mode == "written":
                                extra["is_pre_gate_state"] = True
                            if snap_k:
                                measure(k, "post_dream", end, extra)
                        elif arm.mode == "written":
                            if snap_k and arm.gate == "before":
                                measure(k, "pre_gate", False)
                            dec, rejected, _, immediate = decide(k)
                            pre_info = st["info"]
                            removed = gate_mod.apply(dec, ws.files.graph_db, ws.protected)
                            raw, info = extract_facts(ws.files.graph_db)
                            expected = {f.record_id: f for f in st["facts"] if f.record_id not in rejected}
                            got = {f.record_id: f for f in raw}
                            if set(got) != set(expected) or any(got[r].text != expected[r].text for r in got):
                                raise RuntimeError(f"gate write mismatch at {arm.name} r{rep} c{k}: the graph after "
                                                   f"the gate is not the graph before it minus the rejected records")
                            facts, evs, _ = transition(st["facts"], st["info"], raw, info,
                                                       Step(k, "gate", frozenset(rejected)))
                            record(evs)
                            isolated = isolated_by_gate(pre_info, info, isolated)
                            st.update(facts=facts, info=info)
                            if snap_k:
                                measure(k, "post_gate", end, {"gate_immediate": {
                                    **immediate, **{f"removed_{x}": n for x, n in removed.items()},
                                    "entities_isolated_by_gate": len(isolated)}})
    if orig_fp is not None and fingerprint(original) != orig_fp:
        raise RuntimeError(f"the original KB at {original} changed during the run; results are void")
    return results

