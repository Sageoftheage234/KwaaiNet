#!/usr/bin/env python3
"""Stand-in for the kwaainet binary in tests. Written independently of the provenance package.

It resolves the KB exactly as the real CLI does (KWAAINET_HOME/config.yaml -> rag_kbs[kb].rag_data_dir) and
aborts if that resolves under $FAKE_FORBIDDEN, so any isolation leak fails the test.
Dream behaviour (deterministic, in dream.rs step order): completion adds relation "Bob knows Carol" (unless
--no-relations); merge folds "Groves" into "Leslie Groves" (unless --dedup-threshold > 1), dropping alias fields;
prune deletes every entity with mention_count <= 1, no relations and no chunk links (unless --prune-threshold 0);
sanitize deletes familial relations with a non-Person endpoint.
Env knobs: FAKE_FAIL_DREAM, FAKE_NO_COMPLETIONS, FAKE_NO_REPORT, FAKE_REWRITE_FIELD (Bob.occupation -> physicist).
"""
import json
import os
import sqlite3
import struct
import sys
from pathlib import Path

import yaml

args = sys.argv[1:]


def opt(name, default=None):
    return args[args.index(name) + 1] if name in args else default


def resolve():
    home = Path(os.environ["KWAAINET_HOME"])
    cfg = yaml.safe_load((home / "config.yaml").read_text(encoding="utf-8"))
    kb = opt("--kb", "default")
    entry = cfg["rag_kbs"][kb]
    data = Path(entry["rag_data_dir"]).resolve()
    forbidden = os.environ.get("FAKE_FORBIDDEN")
    if forbidden and (data == Path(forbidden).resolve() or Path(forbidden).resolve() in data.parents):
        print(f"ISOLATION LEAK: {data}", file=sys.stderr)
        sys.exit(97)
    return data, entry["tenant_id"]


def q(n):
    return struct.pack("<q", n)


def uq(b):
    return struct.unpack("<q", bytes(b[:8]))[0]


def enc(o):
    return json.dumps(o, separators=(",", ":"), ensure_ascii=False).encode()


def graph(data, tenant):
    return sqlite3.connect(data / f"graph-{tenant}.db")


def entities(con):
    return {uq(k): json.loads(v) for k, v in con.execute("SELECT key, value FROM entities")}


def relations(con):
    out = {}
    for k, v in con.execute("SELECT key, value FROM relations"):
        out[bytes(k)] = json.loads(v)
    return out


def rkey(s, d, t):
    return q(s) + q(d) + t.encode()


if args[:1] == ["--version"]:
    print("kwaainet 0.0.0-fake")
    sys.exit(0)

if args[:3] == ["rag", "graph", "stats"]:
    data, tenant = resolve()
    con = graph(data, tenant)
    print(f"Entities: {len(entities(con))}\nRelations: {len(relations(con))}")
    sys.exit(0)

if args[:3] == ["rag", "graph", "score"]:
    data, tenant = resolve()
    con = graph(data, tenant)
    ents, rels = entities(con), relations(con)
    rep = {"overall": 0.5, "entity_count": len(ents), "relation_count": len(rels), "by_schema_type": {},
           "unknown_count": 0, "top_issues": [], "generated_at": "fake",
           "entity_scores": [{"entity_id": i, "name": e["name"], "entity_type": e["entity_type"], "schema_type": None,
                              "type_score": 0.5, "summary_score": 0.5, "relation_score": 0.5, "overall": 0.5}
                             for i, e in ents.items()]}
    print(json.dumps(rep, indent=2) if "--json" in args else f"Overall: 50.0% ({len(ents)} entities)")
    sys.exit(0)

if args[:3] == ["rag", "dream", "run"]:
    data, tenant = resolve()
    if os.environ.get("FAKE_FAIL_DREAM"):
        print("inference endpoint unreachable", file=sys.stderr)
        sys.exit(2)
    report = {"timestamp": "fake", "duration_secs": 0.1, "entities_type_completed": 0, "entities_summary_completed": 0,
              "entities_relations_added": 0, "entities_merged": 0, "entities_pruned": 0,
              "score_before": 0.5, "score_after": 0.5, "cycle_errors": []}
    if os.environ.get("FAKE_NO_COMPLETIONS"):
        report["cycle_errors"] = ["completion: timeout"]
    else:
        con = graph(data, tenant)
        ents = entities(con)
        by_name = {e["name"]: i for i, e in ents.items()}
        if "--no-relations" not in args and "Bob" in by_name and "Carol" in by_name:
            s, d = by_name["Bob"], by_name["Carol"]
            con.execute("INSERT OR REPLACE INTO relations VALUES (?,?)", (rkey(s, d, "knows"), enc(
                {"src_id": s, "dst_id": d, "relation_type": "knows", "strength": 0.1, "evidence_chunk_ids": [4],
                 "confidence": 1.0, "method": "LlmOpenExtraction", "source": "llm_open"})))
            report["entities_relations_added"] = 1
        else:
            report["entities_summary_completed"] = 1
        if float(opt("--dedup-threshold", "0.92")) <= 1.0 and "Groves" in by_name and "Leslie Groves" in by_name:
            alias, canon = by_name["Groves"], by_name["Leslie Groves"]
            for k, r in relations(con).items():
                if alias not in (r["src_id"], r["dst_id"]):
                    continue
                con.execute("DELETE FROM relations WHERE key=?", (k,))
                ns = canon if r["src_id"] == alias else r["src_id"]
                nd = canon if r["dst_id"] == alias else r["dst_id"]
                if ns == nd:
                    continue
                nk = rkey(ns, nd, r["relation_type"])
                row = con.execute("SELECT value FROM relations WHERE key=?", (nk,)).fetchone()
                if row:
                    e = json.loads(row[0])
                    e["evidence_chunk_ids"] += [c for c in r["evidence_chunk_ids"] if c not in e["evidence_chunk_ids"]]
                    con.execute("UPDATE relations SET value=? WHERE key=?", (enc(e), nk))
                else:
                    r.update(src_id=ns, dst_id=nd)
                    con.execute("INSERT INTO relations VALUES (?,?)", (nk, enc(r)))
            c = ents[canon]
            c["aliases"] = c.get("aliases", []) + ["Groves"]  # alias fields are NOT carried (as in graph.rs)
            con.execute("UPDATE entities SET value=? WHERE key=?", (enc(c), q(canon)))
            con.execute("DELETE FROM entities WHERE key=?", (q(alias),))
            report["entities_merged"] = 1
        if os.environ.get("FAKE_REWRITE_FIELD") and "Bob" in by_name:
            b = entities(con)[by_name["Bob"]]
            b["fields"]["occupation"]["value"] = "physicist"
            con.execute("UPDATE entities SET value=? WHERE key=?", (enc(b), q(by_name["Bob"])))
        if float(opt("--prune-threshold", "0.3")) > 0:
            linked = {uq(k) for k, _ in con.execute("SELECT key, value FROM entity_chunk")}
            ends = {x for r in relations(con).values() for x in (r["src_id"], r["dst_id"])}
            for i, e in entities(con).items():
                if e.get("mention_count", 0) <= 1 and i not in ends and i not in linked:
                    con.execute("DELETE FROM entities WHERE key=?", (q(i),))
                    report["entities_pruned"] += 1
        familial = {"parent_of", "child_of", "spouse_of", "sibling_of"}
        cur = entities(con)
        for k, r in relations(con).items():
            types = [cur.get(r["src_id"], {}).get("entity_type"), cur.get(r["dst_id"], {}).get("entity_type")]
            if r["relation_type"] in familial and any(t != "Person" for t in types):
                con.execute("DELETE FROM relations WHERE key=?", (k,))
        con.commit()
        con.close()
    if not os.environ.get("FAKE_NO_REPORT"):
        (data / f"dream-report-{tenant}.json").write_text(json.dumps(report), encoding="utf-8")
    print(f"Summary completions: {report['entities_summary_completed']}\nRelations added: {report['entities_relations_added']}")
    sys.exit(0)

if args[:2] == ["rag", "eval"]:
    data, tenant = resolve()
    con = graph(data, tenant)
    nrel = len(relations(con))
    retrieval = min(1.0, nrel / 10)
    out = Path(opt("--output"))
    semantic = "--semantic-score" in args
    rows = (f"| Retrieval recall (token-overlap + semantic) | {retrieval*100:.1f}% ({nrel:.1f}/10) |\n"
            f"| Generation recall (token-overlap + semantic) | 50.0% (5.0/10) |\n") if semantic else \
        "| Overall recall (token-overlap) | 50.0% (5.0/10) |\n"
    out.write_text(f"# Eval\n\n## Summary\n\n| Metric | Value |\n|---|---|\n{rows}\n## Per-question results\n", encoding="utf-8")
    sys.exit(0)

print(f"unknown command {args}", file=sys.stderr)
sys.exit(64)
