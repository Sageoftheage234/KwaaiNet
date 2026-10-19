"""Bind an edit-compatibility result to the exact runtime it was checked on (audit item 11).

A PASS authorizes written gates only for the same snapshot contents, executable bytes and version, KB config
template, freeze commit, check protocol and writer code. Any mismatch is reported key by key and refused.
"""
from __future__ import annotations

import hashlib
import json
from pathlib import Path

PROTOCOL = "edit-compat/2"
_WRITER_FILES = ("kwaainet_store.py", "gate.py")


def _sha_json(o) -> str:
    return hashlib.sha256(json.dumps(o, sort_keys=True, default=str).encode()).hexdigest()


def writer_code_sha() -> str:
    h = hashlib.sha256()
    for name in _WRITER_FILES:
        h.update((Path(__file__).with_name(name)).read_bytes())
    return h.hexdigest()


def runtime_binding(kw, snap) -> dict:
    ident = kw.identity()
    return {"protocol": PROTOCOL,
            "snapshot_id": snap.snapshot_id,
            "snapshot_files_sha256": _sha_json(snap.meta["files"]),
            "config_template_sha256": _sha_json(snap.meta["config_template"]),
            "freeze_commit": snap.meta.get("freeze_commit"),
            "binary_files": [{"path": f["path"], "sha256": f["sha256"]} for f in ident["files"]],
            "binary_version": ident["version"],
            "writer_code_sha256": writer_code_sha()}


def authorize(result_path: str, binding: dict) -> tuple[bool, str]:
    if not result_path:
        return False, "no edit-compatibility result configured (run check_edit_compat.py)"
    p = Path(result_path)
    if not p.exists():
        return False, f"edit-compatibility result not found: {p}"
    r = json.loads(p.read_text(encoding="utf-8"))
    if r.get("status") != "PASS":
        return False, f"edit-compatibility check status is {r.get('status')}: {r.get('reason', '')}"
    stored = r.get("binding")
    if not isinstance(stored, dict):
        return False, "edit-compatibility result has no runtime binding (older protocol); rerun check_edit_compat"
    bad = sorted(k for k in set(binding) | set(stored) if stored.get(k) != binding.get(k))
    if bad:
        return False, f"edit-compatibility result does not match this runtime: {', '.join(bad)} differ; rerun check_edit_compat"
    return True, "PASS (binding verified)"
