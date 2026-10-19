"""python -m provenance.cli <command>  (run from tests/kwaai-knowledge)."""
from __future__ import annotations

import argparse
import json
from pathlib import Path

from . import checks
from .config import load_config
from .labeling import export_blinded, import_labels
from .report import build
from .runner import run
from .schemas import GateDecision


def main(argv=None):
    p = argparse.ArgumentParser(prog="provenance")
    s = p.add_subparsers(dest="cmd", required=True)
    q = s.add_parser("snapshot", help="copy a registered KB into a frozen snapshot (original is read only)")
    q.add_argument("--kb", required=True); q.add_argument("--dest", required=True)
    q.add_argument("--freeze-commit", default="486bcf6"); q.add_argument("--kwaainet", default="kwaainet")
    q.add_argument("--kwaainet-home"); q.add_argument("--out-json")
    q = s.add_parser("check-binary"); q.add_argument("--snapshot", required=True); q.add_argument("--scratch", required=True)
    q.add_argument("--kwaainet", default="kwaainet"); q.add_argument("--out-json")
    q = s.add_parser("check-edit"); q.add_argument("--snapshot", required=True); q.add_argument("--scratch", required=True)
    q.add_argument("--kwaainet", default="kwaainet"); q.add_argument("--model", default="llama3.1:8b")
    q.add_argument("--inference-url"); q.add_argument("--out-json", required=True)
    q = s.add_parser("run"); q.add_argument("--snapshot", required=True); q.add_argument("--out", required=True)
    q.add_argument("--config"); q.add_argument("--max-cycle", type=int); q.add_argument("--run-id")
    q = s.add_parser("report"); q.add_argument("results"); q.add_argument("csv"); q.add_argument("--plots")
    q = s.add_parser("label-export"); q.add_argument("--run", required=True); q.add_argument("--meta-db", required=True)
    q.add_argument("--out", required=True); q.add_argument("--key", required=True); q.add_argument("--n", type=int, default=150)
    q.add_argument("--seed", type=int, default=0)
    q = s.add_parser("label-import"); q.add_argument("labels"); q.add_argument("key"); q.add_argument("--second-rater")
    q.add_argument("--out", required=True); q.add_argument("--seed", type=int, default=0)
    a = p.parse_args(argv)
    if a.cmd == "snapshot":
        r = checks.check_snapshot(a.kb, a.dest, a.freeze_commit, a.kwaainet, a.out_json, a.kwaainet_home)
    elif a.cmd == "check-binary":
        r = checks.check_binary(a.kwaainet, a.snapshot, a.scratch, a.out_json)
    elif a.cmd == "check-edit":
        r = checks.check_edit_compat(a.kwaainet, a.snapshot, a.scratch, a.model, a.inference_url, a.out_json)
    elif a.cmd == "run":
        print(run(load_config(a.config), a.snapshot, a.out, a.run_id, max_cycle=a.max_cycle)); return 0
    elif a.cmd == "report":
        print(build(a.results, a.csv, a.plots)); return 0
    elif a.cmd == "label-export":
        rows = [json.loads(l) for l in (Path(a.run) / "gate_decisions.jsonl").read_text(encoding="utf-8").splitlines()]
        fields = GateDecision.__dataclass_fields__
        uniq = {}
        for r in rows:  # the same claim is judged every cycle; label each distinct (claim, evidence, verdict) once
            d = GateDecision(**{k: (tuple(v) if isinstance(v, list) else v) for k, v in r.items() if k in fields})
            uniq.setdefault((d.text, d.evidence_chunk_ids, d.accepted), d)
        decs = list(uniq.values())
        export_blinded(decs, a.meta_db, a.out, a.key, a.n, a.seed); print(a.out); return 0
    elif a.cmd == "label-import":
        stats = import_labels(a.labels, a.key, a.second_rater, a.seed)
        Path(a.out).write_text(json.dumps(stats, indent=2, default=str), encoding="utf-8"); print(a.out); return 0
    return 0 if r["status"] == "PASS" else 1


if __name__ == "__main__":
    raise SystemExit(main())
