"""Synthetic KwaaiNet KB in the exact 486bcf6 on-disk format, plus a fake kwaainet binary."""
import hashlib
import sqlite3
import struct
import sys
import uuid
from pathlib import Path

import pytest
import yaml

sys.path.insert(0, str(Path(__file__).resolve().parents[2]))  # tests/kwaai-knowledge

from provenance.kwaainet_store import RawNum, dumps  # noqa: E402

TENANT = "3f1c2a5e-8b7d-4c6e-9a0b-1d2e3f405060"
FAKE = Path(__file__).with_name("fake_kwaainet.py")
CHUNKS = {1: "Leslie Groves was a general.", 2: "Groves led the Manhattan Project.", 3: "Alice works at Acme.",
          4: "Bob lives in Paris.", 5: "Carol is a scientist. She works in Paris.", 6: "Neptune is a planet.",
          7: "Bob is a scientist.", 8: "Dana plays chess."}
GRAPH_DDL = """
CREATE TABLE entities (key BLOB NOT NULL PRIMARY KEY, value BLOB NOT NULL) WITHOUT ROWID;
CREATE TABLE relations (key BLOB NOT NULL PRIMARY KEY, value BLOB NOT NULL) WITHOUT ROWID;
CREATE TABLE chunk_entity (key BLOB NOT NULL PRIMARY KEY, value BLOB NOT NULL) WITHOUT ROWID;
CREATE TABLE entity_chunk (key BLOB NOT NULL PRIMARY KEY, value BLOB NOT NULL) WITHOUT ROWID;
CREATE TABLE chunk_mentions (key BLOB NOT NULL PRIMARY KEY, value BLOB NOT NULL) WITHOUT ROWID;
CREATE TABLE entity_timeline_v1 (key BLOB NOT NULL PRIMARY KEY, value BLOB NOT NULL) WITHOUT ROWID;
CREATE TABLE interactions (key BLOB NOT NULL PRIMARY KEY, value BLOB NOT NULL) WITHOUT ROWID;
CREATE TABLE metadata (key TEXT NOT NULL PRIMARY KEY, value TEXT NOT NULL);"""
META_DDL = """
CREATE TABLE chunks (key BLOB NOT NULL PRIMARY KEY, value BLOB NOT NULL) WITHOUT ROWID;
CREATE TABLE docs (key BLOB NOT NULL PRIMARY KEY, value BLOB NOT NULL) WITHOUT ROWID;
CREATE TABLE sync (key BLOB NOT NULL PRIMARY KEY, value BLOB NOT NULL) WITHOUT ROWID;
CREATE TABLE summary_nodes (key BLOB NOT NULL PRIMARY KEY, value BLOB NOT NULL) WITHOUT ROWID;"""


def q(n):
    return struct.pack("<q", n)


def eid(name, etype="Person"):
    return struct.unpack("<q", hashlib.sha256(f"{name.lower()}::{etype}".encode()).digest()[:8])[0]


def node(name, fields=None, description="", mention_count=1, etype="Person", aliases=()):
    # Field order and float spellings mirror serde_json output of EntityNode (graph.rs ~L228-280).
    return {"id": RawNum(str(eid(name, etype))), "name": name, "entity_type": etype, "description": description,
            "embedding": [RawNum("0.1"), RawNum("1e-5"), RawNum("-0.33333334")], "mention_count": RawNum(str(mention_count)),
            "first_chunk_id": RawNum("1"), "aliases": list(aliases), "schema_type": None,
            "fields": {k: {"value": v, "evidence_chunk_ids": [RawNum(str(c)) for c in ev], "confidence": RawNum("0.33333334")}
                       for k, (v, ev) in (fields or {}).items()},
            "confidence": RawNum("0.0"), "extraction_confidence": RawNum("1.0")}


def write_graph(db: Path, ents: dict, rels: list, links: dict):
    """ents: {key: node dict}; rels: [(src node, type, dst node, evidence)]; links: {chunk id: [node dict]}."""
    g = sqlite3.connect(db)
    g.executescript(GRAPH_DDL)
    for v in ents.values():
        g.execute("INSERT INTO entities VALUES (?,?)", (q(int(v["id"])), dumps(v)))
    by_entity = {}
    for cid, nodes in links.items():
        g.execute("INSERT INTO chunk_entity VALUES (?,?)", (q(cid), dumps([RawNum(str(int(n["id"]))) for n in nodes])))
        for n in nodes:
            by_entity.setdefault(int(n["id"]), []).append(cid)
    for e, cids in by_entity.items():
        g.execute("INSERT INTO entity_chunk VALUES (?,?)", (q(e), dumps([RawNum(str(c)) for c in cids])))
    for s_, t, d, ev in rels:
        si, di = int(s_["id"]), int(d["id"])
        rel = {"src_id": RawNum(str(si)), "dst_id": RawNum(str(di)), "relation_type": t, "strength": RawNum("0.1"),
               "evidence_chunk_ids": [RawNum(str(c)) for c in ev], "confidence": RawNum("1.0"),
               "method": "LlmOpenExtraction", "source": "llm_open"}
        g.execute("INSERT INTO relations VALUES (?,?)", (q(si) + q(di) + t.encode(), dumps(rel)))
    g.commit(); g.close()


def write_meta(db: Path, chunks: dict = CHUNKS):
    m = sqlite3.connect(db)
    m.executescript(META_DDL)
    for cid, text in chunks.items():
        meta = {"doc_name": f"d{cid}.txt", "chunk_index": RawNum("0"), "text": text, "surrounding": "", "page_num": None,
                "ingested_at": "2026-09-01T00:00:00Z", "section_name": None, "skip_extraction": False,
                "section_note": None, "section_type": "main"}
        m.execute("INSERT INTO chunks VALUES (?,?)", (uuid.UUID(TENANT).bytes + q(cid), dumps(meta)))
    m.commit(); m.close()


def build_kb(data_dir: Path):
    data_dir.mkdir(parents=True)
    n = {
        "Leslie Groves": node("Leslie Groves", {"occupation": ("general", [1])}, mention_count=3),
        "Groves": node("Groves", {"project": ("Manhattan Project", [2])}, mention_count=2),
        "Alice": node("Alice", {"employer": ("Acme", [3])}, mention_count=2),
        "Bob": node("Bob", {"occupation": ("scientist", [4])}, mention_count=2),
        "Carol": node("Carol", description="Carol is a scientist. She works in Paris.", mention_count=2),
        "Ghost": node("Ghost", {"planet": ("Pluto", [])}, mention_count=1),
        # Dana has no chunk links and one unsupported relation: a written gate isolates her, the next dream
        # prunes her, and her supported field is lost to a gate-induced prune.
        "Dana": node("Dana", {"hobby": ("chess", [8])}, mention_count=1),
        "Acme": node("Acme", description="Acme is a company.", mention_count=2, etype="Organization"),
    }
    links = {1: [n["Leslie Groves"]], 2: [n["Groves"]], 3: [n["Alice"], n["Acme"]], 4: [n["Bob"]], 5: [n["Carol"]]}
    rels = [(n["Groves"], "works_with", n["Alice"], [2]), (n["Leslie Groves"], "works_with", n["Alice"], [1]),
            (n["Groves"], "mentors", n["Carol"], [2]), (n["Leslie Groves"], "mentors", n["Bob"], [1]),
            (n["Dana"], "mentors", n["Bob"], [6]),
            # familial relation with an Organization endpoint: sanitize_relations deletes it
            (n["Alice"], "parent_of", n["Acme"], [3])]
    write_graph(data_dir / f"graph-{TENANT}.db", n, rels, links)
    write_meta(data_dir / f"{TENANT}.db")


@pytest.fixture
def kwaainet_home(tmp_path, monkeypatch):
    """A registered KB 'D6t' in its own KWAAINET_HOME, like a real install."""
    home = tmp_path / "orig-home"
    data = home / "rag" / "D6t"
    build_kb(data)
    cfg = {"inference_url": "http://localhost:11434",
           "rag_kbs": {"D6t": {"tenant_id": TENANT, "embed_model": "nomic-embed-text", "embed_dim": 768,
                               "inference_url": "http://localhost:11434", "top_k": 5, "rag_data_dir": str(data)}}}
    (home / "config.yaml").write_text(yaml.safe_dump(cfg), encoding="utf-8")
    monkeypatch.setenv("KWAAINET_HOME", str(home))
    monkeypatch.setenv("FAKE_FORBIDDEN", str(data))
    return home, data


@pytest.fixture
def fake_cmd():
    return [sys.executable, str(FAKE)]


@pytest.fixture
def kw(fake_cmd):
    """A driver with a short timeout, so a hung subprocess fails the test instead of stalling the suite."""
    from provenance.driver import Kwaainet
    return Kwaainet(fake_cmd, timeout=60)


@pytest.fixture
def snapshot(kwaainet_home, tmp_path):
    from provenance.kb import make_snapshot
    return make_snapshot("D6t", tmp_path / "snap", "486bcf6")
