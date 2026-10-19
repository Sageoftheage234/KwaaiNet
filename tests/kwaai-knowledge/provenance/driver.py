"""Subprocess driver for the ``kwaainet`` binary, always against an isolated Workspace."""
from __future__ import annotations

import hashlib
import json
import shlex
import shutil
import subprocess
from dataclasses import dataclass, field
from pathlib import Path

from .kb import Workspace, assert_isolated
from .retrieval_cost import parse_eval_report

DREAM_FLAGS = {  # cli.rs DreamAction::Run at 486bcf6 (~L1784-1830)
    "threshold": "--threshold",
    "dedup_threshold": "--dedup-threshold",
    "max_completions": "--max-completions",
    "workers": "--workers",
    "prune_threshold": "--prune-threshold",
    "inference_urls": "--inference-urls",
}
DREAM_SWITCHES = {"no_relations": "--no-relations", "relation_summary": "--relation-summary"}


class KwaainetError(RuntimeError):
    pass


@dataclass
class DreamResult:
    report: dict
    stdout: str
    stderr: str
    args: list[str]
    completed_anything: bool = False
    destructive_steps_skipped: bool = False
    notes: list[str] = field(default_factory=list)


class Kwaainet:
    def __init__(self, cmd: str | list[str], timeout: float = 3600.0):
        # posix=False keeps Windows backslashes; strip the quotes it leaves around paths with spaces.
        self.cmd = [c[1:-1] if len(c) > 1 and c[0] == c[-1] and c[0] in "\"'" else c
                    for c in shlex.split(cmd, posix=False)] if isinstance(cmd, str) else list(cmd)
        self.timeout = timeout

    def run(self, args: list[str], ws: Workspace | None = None, check: bool = True) -> subprocess.CompletedProcess:
        env = None
        if ws is not None:
            assert_isolated(ws)
            env = ws.env()
        try:
            cp = subprocess.run(self.cmd + args, env=env, capture_output=True, text=True,
                                encoding="utf-8", errors="replace", timeout=self.timeout)
        except FileNotFoundError as e:
            raise KwaainetError(f"kwaainet binary not found: {self.cmd[0]}") from e
        except subprocess.TimeoutExpired as e:
            raise KwaainetError(f"timed out after {self.timeout}s: {' '.join(args)}") from e
        if check and cp.returncode != 0:
            raise KwaainetError(f"exit {cp.returncode}: {' '.join(args)}\nstderr: {cp.stderr[-2000:]}")
        return cp

    def version(self) -> str:
        cp = self.run(["--version"])
        return (cp.stdout or cp.stderr).strip()

    def graph_stats(self, ws: Workspace) -> dict:
        cp = self.run(["rag", "graph", "stats", "--kb", ws.kb], ws)
        out = {}
        for line in cp.stdout.splitlines():
            for key in ("Entities", "Relations"):
                if line.strip().startswith(key + ":"):
                    try:
                        out[key.lower()] = int(line.split(":", 1)[1].split()[0].replace(",", ""))
                    except (ValueError, IndexError):
                        pass
        return out

    def graph_score(self, ws: Workspace) -> dict:
        """``rag graph score --json`` (cli.rs ~L2167-2175, rag_cmd.rs ~L5014-5023): read-only GraphHealthReport."""
        cp = self.run(["rag", "graph", "score", "--kb", ws.kb, "--json"], ws)
        text = cp.stdout
        start = text.find("{")
        if start < 0:
            raise KwaainetError(f"rag graph score --json printed no JSON: {text[-500:]!r}")
        try:
            return json.loads(text[start:])
        except json.JSONDecodeError as e:
            raise KwaainetError(f"rag graph score --json output is not valid JSON: {e}") from e

    def identity(self) -> dict:
        """What the edit-compatibility result is bound to: the resolved executable(s) and their hashes."""
        parts = []
        for i, part in enumerate(self.cmd):
            p = shutil.which(part) if i == 0 else None
            path = Path(p) if p else Path(part)
            if path.is_file():
                parts.append({"arg": i, "path": str(path.resolve()),
                              "sha256": hashlib.sha256(path.read_bytes()).hexdigest()})
        if not parts:
            raise KwaainetError(f"cannot resolve the kwaainet executable from {self.cmd!r}")
        return {"cmd": list(self.cmd), "files": parts, "version": self.version()}

    def dream_run(self, ws: Workspace, model: str, inference_url: str | None = None,
                  flags: dict | None = None) -> DreamResult:
        report_path = ws.files.dream_report
        if report_path.exists():
            report_path.unlink()  # a stale report must never be read as this cycle's result
        args = ["rag", "dream", "run", "--kb", ws.kb, "--model", model]
        if inference_url:
            args += ["--inference-url", inference_url]
        for k, v in (flags or {}).items():
            if k in DREAM_SWITCHES:
                if v:
                    args.append(DREAM_SWITCHES[k])
            elif k in DREAM_FLAGS:
                if v is not None:
                    args += [DREAM_FLAGS[k], str(v)]
            else:
                raise ValueError(f"unknown dream flag {k!r}")
        cp = self.run(args, ws)
        if not report_path.exists():
            raise KwaainetError(f"dream run exited 0 but wrote no report at {report_path}")
        report = json.loads(report_path.read_text(encoding="utf-8"))
        res = DreamResult(report, cp.stdout, cp.stderr, args)
        res.completed_anything = any(report.get(k, 0) > 0 for k in
                                     ("entities_type_completed", "entities_summary_completed", "entities_relations_added"))
        # dream.rs ~L900-935: merge/prune/sanitize are skipped only when nothing completed AND a completion
        # failed or a cycle error occurred. The report carries counts, not the branch taken, so infer it:
        errors = report.get("cycle_errors") or []
        if not res.completed_anything and errors:
            if report.get("entities_merged", 0) == 0 and report.get("entities_pruned", 0) == 0:
                res.destructive_steps_skipped = True
                res.notes.append("inferred: no completions and cycle errors -> dedup/prune/sanitize skipped")
            else:
                res.notes.append("no completions with errors, but merges/prunes were reported; not a skip")
        return res

    def rag_eval(self, ws: Workspace, questions: str | Path, out_md: Path, model: str,
                 inference_url: str | None = None, k: int = 20, mode: str = "iterative",
                 semantic_score: bool = True) -> dict:
        args = ["rag", "eval", "--kb", ws.kb, "--questions", str(questions), "--model", model,
                "-k", str(k), "--mode", mode, "--output", str(out_md)]
        if inference_url:
            args += ["--inference-url", inference_url]
        if semantic_score:
            args.append("--semantic-score")
        self.run(args, ws)
        if not Path(out_md).exists():
            raise KwaainetError(f"rag eval wrote no report at {out_md}")
        return parse_eval_report(Path(out_md).read_text(encoding="utf-8"))
