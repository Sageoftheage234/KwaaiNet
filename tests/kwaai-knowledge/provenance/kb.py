"""KB snapshot and per-run isolation. The original KB is only ever read and copied.

How the CLI resolves a KB at 486bcf6 (core/crates/kwaai-cli/src):
* config.rs ``kwaainet_dir()``: ``$KWAAINET_HOME`` if set, else ``~/.kwaainet``; config is ``config.yaml`` there.
* rag_cmd.rs ``load_rag_config_for(kb)``: ``cfg.get_rag_kb(kb)`` from ``rag_kbs`` (legacy ``rag`` for "default").
* config.rs ``RagConfig::data_dir()``: ``rag_data_dir`` if set, else ``<kwaainet_dir>/rag``.

Isolation: every subprocess runs with ``KWAAINET_HOME`` pointing at a private home whose ``config.yaml``
registers the KB name with ``rag_data_dir`` set to that run's copied data directory. The original
config.yaml is read once, at snapshot time, and never written.
"""
from __future__ import annotations

import datetime as _dt
import hashlib
import json
import os
import shutil
from dataclasses import dataclass
from pathlib import Path

import yaml

from .kwaainet_store import KBFiles, find_kb_files

SNAPSHOT_META = "SNAPSHOT.json"


def kwaainet_home(explicit: str | None = None) -> Path:
    if explicit:
        return Path(explicit).expanduser()
    env = os.environ.get("KWAAINET_HOME")
    return Path(env).expanduser() if env else Path.home() / ".kwaainet"


def load_config(home: Path) -> dict:
    p = home / "config.yaml"
    if not p.exists():
        raise FileNotFoundError(f"no KwaaiNet config at {p}")
    return yaml.safe_load(p.read_text(encoding="utf-8")) or {}


def resolve_kb(kb: str, home: Path) -> tuple[dict, str, Path]:
    cfg = load_config(home)
    entry = (cfg.get("rag_kbs") or {}).get(kb)
    if entry is None and kb == "default":
        entry = cfg.get("rag")
    if entry is None:
        raise KeyError(f"KB '{kb}' is not registered in {home / 'config.yaml'}")
    tenant = entry.get("tenant_id")
    if not tenant:
        raise ValueError(f"KB '{kb}' has no tenant_id")
    if entry.get("eve_peer_id"):
        raise ValueError(f"KB '{kb}' uses a remote Eve store (eve_peer_id); copies would share it. Refusing.")
    if entry.get("storage_url") not in (None, "", "local"):
        raise ValueError(f"KB '{kb}' storage_url={entry.get('storage_url')!r} is not local. Refusing.")
    data_dir = Path(entry["rag_data_dir"]) if entry.get("rag_data_dir") else home / "rag"
    return entry, str(tenant), data_dir


def fingerprint(directory: Path) -> dict[str, str]:
    out = {}
    for p in sorted(Path(directory).rglob("*")):
        if p.is_file():
            h = hashlib.sha256()
            with open(p, "rb") as fh:
                for block in iter(lambda: fh.read(1 << 20), b""):
                    h.update(block)
            out[p.relative_to(directory).as_posix()] = h.hexdigest()
    return out


def _inside(child: Path, parent: Path) -> bool:
    child, parent = child.resolve(), parent.resolve()
    return child == parent or parent in child.parents


def _check_no_writer(data_dir: Path) -> list[str]:
    return [p.name for p in data_dir.glob("*-wal") if p.stat().st_size > 0]


@dataclass(frozen=True)
class Snapshot:
    root: Path
    kb: str
    tenant_id: str
    meta: dict

    @property
    def data_dir(self) -> Path:
        return self.root / "data"

    @property
    def snapshot_id(self) -> str:
        return self.meta["snapshot_id"]


def make_snapshot(kb: str, dest: Path, freeze_commit: str, home: str | None = None,
                  binary_version: str | None = None, allow_wal: bool = False) -> Snapshot:
    home_p = kwaainet_home(home)
    entry, tenant, src = resolve_kb(kb, home_p)
    dest = Path(dest)
    if dest.exists():
        raise FileExistsError(f"snapshot destination exists: {dest}")
    if _inside(dest, src) or _inside(src, dest):
        raise ValueError("snapshot destination must not overlap the KB data directory")
    find_kb_files(src, tenant)
    wal = _check_no_writer(src)
    if wal and not allow_wal:
        raise RuntimeError(f"non-empty WAL files {wal} in {src}: stop any running kwaainet process first")
    before = fingerprint(src)
    shutil.copytree(src, dest / "data")
    after_src = fingerprint(src)
    if before != after_src:
        raise RuntimeError("the source KB changed while it was being copied; snapshot discarded")
    copy_fp = fingerprint(dest / "data")
    if copy_fp != before:
        raise RuntimeError("snapshot copy does not match the source byte-for-byte")
    cfg = load_config(home_p)
    cfg.pop("rag", None)
    cfg["rag_kbs"] = {kb: {k: v for k, v in entry.items() if k != "rag_data_dir"}}
    sid = hashlib.sha256(json.dumps(copy_fp, sort_keys=True).encode()).hexdigest()[:16]
    meta = {"snapshot_id": sid, "kb": kb, "tenant_id": tenant, "freeze_commit": freeze_commit,
            "kwaainet_version": binary_version, "source_data_dir": str(src),
            "created_utc": _dt.datetime.now(_dt.timezone.utc).isoformat(),
            "files": copy_fp, "config_template": cfg}
    (dest / SNAPSHOT_META).write_text(json.dumps(meta, indent=2, default=str), encoding="utf-8")
    return Snapshot(dest, kb, tenant, meta)


def load_snapshot(root: Path) -> Snapshot:
    root = Path(root)
    meta = json.loads((root / SNAPSHOT_META).read_text(encoding="utf-8"))
    if fingerprint(root / "data") != meta["files"]:
        raise RuntimeError(f"snapshot {root} was modified after creation")
    return Snapshot(root, meta["kb"], meta["tenant_id"], meta)


@dataclass(frozen=True)
class Workspace:
    root: Path
    kb: str
    tenant_id: str
    protected: tuple[str, ...]

    @property
    def data_dir(self) -> Path:
        return self.root / "data"

    @property
    def home(self) -> Path:
        return self.root / "home"

    @property
    def files(self) -> KBFiles:
        return KBFiles(self.data_dir, self.tenant_id)

    def env(self) -> dict:
        e = dict(os.environ)
        e["KWAAINET_HOME"] = str(self.home)
        return e


def make_workspace(snap: Snapshot, dest: Path) -> Workspace:
    dest = Path(dest)
    if dest.exists():
        raise FileExistsError(dest)
    src_original = Path(snap.meta["source_data_dir"])
    if _inside(dest, src_original):
        raise ValueError("workspace must not be inside the original KB data directory")
    shutil.copytree(snap.data_dir, dest / "data")
    cfg = json.loads(json.dumps(snap.meta["config_template"]))
    cfg["rag_kbs"][snap.kb]["rag_data_dir"] = str((dest / "data").resolve())
    (dest / "home").mkdir(parents=True)
    text = yaml.safe_dump(cfg, sort_keys=False)
    for forbidden in {str(src_original), str(src_original.resolve())}:
        if forbidden and forbidden in text:
            raise RuntimeError("isolated config still references the original KB data directory")
    (dest / "home" / "config.yaml").write_text(text, encoding="utf-8")
    ws = Workspace(dest, snap.kb, snap.tenant_id, (str(src_original), str(snap.root)))
    assert_isolated(ws)
    return ws


def assert_isolated(ws: Workspace) -> None:
    cfg = yaml.safe_load((ws.home / "config.yaml").read_text(encoding="utf-8"))
    rdd = Path(cfg["rag_kbs"][ws.kb]["rag_data_dir"])
    if not _inside(rdd, ws.root):
        raise RuntimeError(f"workspace KB resolves outside the workspace: {rdd}")
    for p in ws.protected:
        if _inside(rdd, Path(p)):
            raise RuntimeError(f"workspace KB resolves inside a protected path: {p}")
