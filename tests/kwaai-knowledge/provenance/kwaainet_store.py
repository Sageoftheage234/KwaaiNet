"""Read and (copy-only) write access to a KwaaiNet KB data directory at commit 486bcf6.

On-disk format, verified against the frozen source:

* Graph DB ``graph-<tenant>.db`` (core/crates/kwaai-rag/src/graph.rs, GraphStore::open, ~L885-909):
  tables ``entities``, ``relations``, ``chunk_entity``, ``entity_chunk``, ``chunk_mentions``,
  ``entity_timeline_v1``, ``interactions`` are ``(key BLOB PRIMARY KEY, value BLOB) WITHOUT ROWID``;
  ``metadata`` is ``(key TEXT, value TEXT)``.
* Entity key: ``id.to_le_bytes()`` (8 bytes, little-endian i64); value: ``serde_json::to_vec(EntityNode)``
  (graph.rs ~L1129-1132).
* Relation key: ``src LE i64 || dst LE i64 || relation_type UTF-8`` (graph.rs ``relation_key``, ~L6108);
  value: ``serde_json::to_vec(RelationRecord)``.
* ``entity_chunk`` / ``chunk_entity``: 8-byte LE id -> JSON list of i64 (graph.rs ``link_chunk``, ~L1461).
* MetaStore ``<tenant>.db`` (core/crates/kwaai-rag/src/meta_store.rs, ~L53-97): ``chunks`` key is
  ``tenant_uuid.as_bytes() (16) || chunk_id LE i64 (8)``; value ``serde_json::to_vec(ChunkMeta)``.

JSON values are parsed with a raw-preserving codec so unmodified numbers re-serialize byte-identically,
and records the gate never touches are never rewritten.
"""
from __future__ import annotations

import json
import re
import sqlite3
import struct
from dataclasses import dataclass
from pathlib import Path
from typing import Any, Iterator

UUID_RE = re.compile(r"^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}$")


# ── raw-preserving JSON codec ────────────────────────────────────────────────────
class RawNum:
    """A JSON number kept as its original text so re-encoding is byte-identical."""

    __slots__ = ("raw",)

    def __init__(self, raw: str):
        self.raw = raw

    def __float__(self) -> float:
        return float(self.raw)

    def __int__(self) -> int:
        return int(self.raw)

    def __eq__(self, other: object) -> bool:
        if isinstance(other, RawNum):
            return self.raw == other.raw
        if isinstance(other, (int, float)):
            return float(self.raw) == float(other)
        return NotImplemented

    def __hash__(self) -> int:
        return hash(self.raw)

    def __repr__(self) -> str:
        return f"RawNum({self.raw})"


def loads(data: bytes | str) -> Any:
    if isinstance(data, (bytes, bytearray)):
        data = data.decode("utf-8")
    return json.loads(data, parse_float=RawNum, parse_int=RawNum)


def _enc_str(s: str) -> str:
    # serde_json escapes only '"', '\\' and control chars (short forms for \b\f\n\r\t, else \u00xx
    # lowercase); non-ASCII is emitted raw. json.dumps(ensure_ascii=False) matches that set.
    return json.dumps(s, ensure_ascii=False)


def dumps(obj: Any) -> bytes:
    """Compact serde_json-compatible encoding (no whitespace), preserving RawNum text."""

    def enc(o: Any) -> str:
        if isinstance(o, RawNum):
            return o.raw
        if o is None:
            return "null"
        if o is True:
            return "true"
        if o is False:
            return "false"
        if isinstance(o, str):
            return _enc_str(o)
        if isinstance(o, int):
            return str(o)
        if isinstance(o, float):
            return json.dumps(o)
        if isinstance(o, dict):
            return "{" + ",".join(_enc_str(str(k)) + ":" + enc(v) for k, v in o.items()) + "}"
        if isinstance(o, (list, tuple)):
            return "[" + ",".join(enc(v) for v in o) + "]"
        raise TypeError(f"cannot encode {type(o).__name__}")

    return enc(obj).encode("utf-8")


def plain(o: Any) -> Any:
    """Convert RawNum-bearing structures to ordinary Python numbers (for analysis only)."""
    if isinstance(o, RawNum):
        return int(o.raw) if re.fullmatch(r"-?\d+", o.raw) else float(o.raw)
    if isinstance(o, dict):
        return {k: plain(v) for k, v in o.items()}
    if isinstance(o, list):
        return [plain(v) for v in o]
    return o


# ── keys ──────────────────────────────────────────────────────────────────────────
def entity_key(eid: int) -> bytes:
    return struct.pack("<q", eid)


def decode_i64(key: bytes) -> int:
    return struct.unpack("<q", bytes(key[:8]))[0]


def relation_key(src: int, dst: int, rel_type: str) -> bytes:
    return struct.pack("<q", src) + struct.pack("<q", dst) + rel_type.encode("utf-8")


def decode_relation_key(key: bytes) -> tuple[int, int, str]:
    key = bytes(key)
    return decode_i64(key[:8]), decode_i64(key[8:16]), key[16:].decode("utf-8")


# ── locating stores ───────────────────────────────────────────────────────────────
@dataclass(frozen=True)
class KBFiles:
    data_dir: Path
    tenant_id: str

    @property
    def graph_db(self) -> Path:
        return self.data_dir / f"graph-{self.tenant_id}.db"

    @property
    def meta_db(self) -> Path:
        return self.data_dir / f"{self.tenant_id}.db"

    @property
    def dream_report(self) -> Path:
        # rag_cmd.rs ~L7954: data_dir.join(format!("dream-report-{tenant_id}.json"))
        return self.data_dir / f"dream-report-{self.tenant_id}.json"


def find_kb_files(data_dir: str | Path, tenant_id: str | None = None) -> KBFiles:
    data_dir = Path(data_dir)
    if tenant_id is None:
        graphs = sorted(p for p in data_dir.glob("graph-*.db") if UUID_RE.match(p.stem[len("graph-"):]))
        if len(graphs) != 1:
            raise ValueError(f"expected exactly one graph-<tenant>.db in {data_dir}, found {len(graphs)}; pass tenant_id")
        tenant_id = graphs[0].stem[len("graph-"):]
    kb = KBFiles(data_dir, tenant_id)
    for p in (kb.graph_db, kb.meta_db):
        if not p.exists():
            raise FileNotFoundError(p)
    if (data_dir / f"graph-{tenant_id}.redb").exists() or (data_dir / f"{tenant_id}.redb").exists():
        raise ValueError(f"legacy redb store present in {data_dir}; this tooling reads SQLite stores only")
    return kb


def _connect_ro(path: Path) -> sqlite3.Connection:
    return sqlite3.connect(f"{Path(path).resolve().as_uri()}?mode=ro", uri=True)


# ── readers ───────────────────────────────────────────────────────────────────────
class GraphReader:
    """Read-only view of a graph DB. Never opens the file for writing."""

    def __init__(self, graph_db: str | Path):
        self.path = Path(graph_db)

    def _rows(self, table: str) -> list[tuple[bytes, bytes]]:
        con = _connect_ro(self.path)
        try:
            return [(bytes(k), bytes(v)) for k, v in con.execute(f"SELECT key, value FROM {table}")]
        finally:
            con.close()

    def entities(self) -> Iterator[tuple[int, dict, bytes]]:
        for k, v in self._rows("entities"):
            yield decode_i64(k), loads(v), v

    def relations(self) -> Iterator[tuple[tuple[int, int, str], dict, bytes]]:
        for k, v in self._rows("relations"):
            yield decode_relation_key(k), loads(v), v

    def entity_chunks(self) -> dict[int, list[int]]:
        out = {}
        for k, v in self._rows("entity_chunk"):
            if len(k) == 8:
                out[decode_i64(k)] = [int(x) for x in loads(v)]
        return out


class MetaReader:
    """Read-only view of a MetaStore DB (chunk text and metadata)."""

    def __init__(self, meta_db: str | Path):
        self.path = Path(meta_db)
        self._chunks: dict[int, dict] | None = None

    def chunks(self) -> dict[int, dict]:
        if self._chunks is None:
            con = _connect_ro(self.path)
            try:
                rows = con.execute("SELECT key, value FROM chunks").fetchall()
            finally:
                con.close()
            out = {}
            for k, v in rows:
                k = bytes(k)
                if len(k) != 24:
                    continue
                out[decode_i64(k[16:])] = plain(loads(v))
            self._chunks = out
        return self._chunks

    def text(self, chunk_id: int) -> str | None:
        c = self.chunks().get(chunk_id)
        return None if c is None else c.get("text", "")


# ── writer (copies only) ──────────────────────────────────────────────────────────
class GraphWriter:
    """Edits a graph DB copy in place. Refuses to open any path in ``protected``."""

    def __init__(self, graph_db: str | Path, protected: list[str | Path] = ()):
        self.path = Path(graph_db).resolve()
        for p in protected:
            p = Path(p).resolve()
            if self.path == p or p in self.path.parents:
                raise PermissionError(f"refusing to write {self.path}: inside protected path {p}")
        self.con = sqlite3.connect(self.path)

    def delete_relation(self, src: int, dst: int, rel_type: str) -> int:
        cur = self.con.execute("DELETE FROM relations WHERE key = ?", (relation_key(src, dst, rel_type),))
        return cur.rowcount

    def remove_field(self, eid: int, field_name: str) -> bool:
        row = self.con.execute("SELECT value FROM entities WHERE key = ?", (entity_key(eid),)).fetchone()
        if row is None:
            return False
        node = loads(row[0])
        fields = node.get("fields") or {}
        if field_name not in fields:
            return False
        del fields[field_name]
        self.con.execute("UPDATE entities SET value = ? WHERE key = ?", (dumps(node), entity_key(eid)))
        return True

    def rewrite_unchanged(self) -> int:
        """Decode and re-encode every entity and relation without changes (no-op compatibility probe)."""
        n = 0
        for table in ("entities", "relations"):
            for k, v in self.con.execute(f"SELECT key, value FROM {table}").fetchall():
                self.con.execute(f"UPDATE {table} SET value = ? WHERE key = ?", (dumps(loads(v)), k))
                n += 1
        return n

    def commit_close(self) -> None:
        self.con.commit()
        self.con.close()
