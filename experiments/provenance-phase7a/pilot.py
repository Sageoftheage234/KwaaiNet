#!/usr/bin/env python3
"""Development-only snapshot preparation and diagnostic scoring; no reference annotations."""
import argparse
from collections import Counter
from datetime import datetime, timezone
import hashlib
import json
from pathlib import Path
import shutil
import sqlite3

BASE = Path(__file__).resolve().parent
TENANT = 'f89d5add-4335-4b23-be54-47820a20f155'
LABELS = ['supported', 'unsupported', 'unresolved']


def sha(data):
    return hashlib.sha256(data).hexdigest()


def write(path, value):
    with path.open('x') as f:
        json.dump(value, f, indent=2, ensure_ascii=False)
        f.write('\n')


def identity(domain, parts):
    # Exact v1 domain-separated length-prefix encoding, checked by native verifier.
    return sha(domain.encode() + b''.join(len(p.encode()).to_bytes(8, 'big') + p.encode() for p in parts))


def read_table(path, table):
    # Snapshot copies have no WAL. Immutable mode prevents SQLite sidecar writes.
    with sqlite3.connect(path.as_uri() + '?mode=ro&immutable=1', uri=True) as conn:
        return [(k, json.loads(v)) for k, v in conn.execute('select key,value from ' + table)]


def check(snapshot):
    manifest = json.loads((snapshot / 'snapshot.json').read_text())
    for rel, info in manifest['files'].items():
        assert sha((snapshot / rel).read_bytes()) == info['sha256'], rel
    payload = json.dumps(manifest['files'], sort_keys=True, separators=(',', ':')).encode()
    assert manifest['snapshot_id'] == 'phase7a-development-v1-' + sha(payload)
    return manifest


def prepare():
    source = Path('/home/kwaai/.kwaainet/rag/prov-pilot2')
    lifecycle = Path('/home/kwaai/provenance/prov-pilot2')
    originals = Path('/home/kwaai/dreamrag-corpus/corpuss_final_review/Manhattan Project/documents')
    target = BASE / 'development-v1'
    target.mkdir()  # Never overwrite a previous snapshot.
    copies = []
    for name in [TENANT + '.db', 'graph-' + TENANT + '.db', 'metadata.db']:
        p = source / name
        assert not Path(str(p) + '-wal').exists(), 'Live WAL: obtain a consistent read-only backup first'
        copies.append((p, target / 'store' / name))
    copies += [(p, target / 'lifecycle' / p.name) for p in sorted(lifecycle.iterdir()) if p.is_file()]
    copies += [(originals / name, target / 'originals' / name) for name in
               ['Einstein-Szilard Letter.txt', 'Franck Report.html', 'Potsdam Declaration.html']]
    files = {}
    for src, dst in copies:
        before = sha(src.read_bytes())
        dst.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(src, dst)
        assert before == sha(dst.read_bytes()) == sha(src.read_bytes())
        files[str(dst.relative_to(target))] = dict(source=str(src), sha256=before, bytes=dst.stat().st_size)
    # Recheck the entire source set after copying, not just each file locally.
    for info in files.values():
        assert sha(Path(info['source']).read_bytes()) == info['sha256']
    snapshot_id = 'phase7a-development-v1-' + sha(json.dumps(files, sort_keys=True, separators=(',', ':')).encode())
    write(target / 'snapshot.json', dict(schema_version=1, snapshot_id=snapshot_id,
          purpose='development feasibility only; not publication or held-out data', tenant=TENANT,
          captured_utc=datetime.now(timezone.utc).isoformat(), files=files,
          consistency='No WAL observed; source hashes stable across copy; not an atomic multi-file historical snapshot'))
    chunks = {int.from_bytes(k[16:], 'little', signed=True): v for k, v in read_table(target / 'store' / (TENANT + '.db'), 'chunks')}
    entities = {v['id']: v for _, v in read_table(target / 'store' / ('graph-' + TENANT + '.db'), 'entities')}
    facts = []
    for e in entities.values():
        for field, v in e.get('fields', {}).items():
            if v['value'].strip():
                facts.append(('entity_field', 'entity:' + str(e['id']), field, v['value'], sorted(set(v['evidence_chunk_ids']))))
    for _, r in read_table(target / 'store' / ('graph-' + TENANT + '.db'), 'relations'):
        facts.append(('relation', 'entity:' + str(r['src_id']), r['relation_type'], 'entity:' + str(r['dst_id']), sorted(set(r['evidence_chunk_ids']))))

    def request(kind, subject, predicate, obj, refs):
        claim = dict(id=identity('kwaai-provenance:claim:v1', [kind, subject, predicate, obj]),
                     kind=kind, subject=subject, predicate=predicate, object=obj)
        evidence = []
        for cid in refs:
            content_hash = sha(chunks[cid]['text'].encode())
            sid = 'kb:' + TENANT
            evidence.append(dict(id=identity('kwaai-provenance:evidence:v1', ['original', sid, content_hash, str(cid)]),
                                 kind='original', source_id=sid, content_hash=content_hash, chunk_id=cid, parent_ids=[]))
        bindings = {v: entities[int(v[7:])]['name'] for v in [subject, obj] if v.startswith('entity:')}
        return dict(claim=claim, evidence=evidence, entity_bindings=bindings, metadata={'snapshot_id': snapshot_id, 'purpose': 'development diagnostic'})

    natural = [request(*f) for f in facts]
    write(target / 'natural-claims.json', natural)
    # Deterministic convenience sample: one short field, one short relation, then another field.
    selected = []
    for kind in ['entity_field', 'relation', 'entity_field']:
        candidates = sorted((r for r in natural if r['claim']['kind'] == kind and len(r['evidence']) == 1
                             and r['claim']['id'] not in {s['claim']['id'] for s in selected}), key=lambda r: r['claim']['id'])
        selected.append(candidates[0])
    write(target / 'natural-diagnostic-sample.json', selected)
    # Constructed challenge expectations are authored checks, NEVER independent ground truth.
    specs = [
        ('supported', 'Einstein–Szilard letter', 'is dated', 'August 2nd, 1939', 4367917624060246291, 'The passage explicitly prints this date.'),
        ('unsupported', 'Einstein–Szilard letter', 'is dated', 'August 2nd, 1940', 4367917624060246291, 'The printed 1939 date contradicts 1940.'),
        ('unresolved', 'Albert Einstein', 'was born in', 'Ulm', 4367917624060246291, 'The supplied passage gives no birthplace.'),
        ('supported', 'United States', 'has uranium ores described as', 'very poor ores in moderate quantities', 6503319673937401246, 'Explicit description in this passage.'),
        ('unsupported', 'United States', 'has uranium ores described as', 'very rich ores in enormous quantities', 6503319673937401246, 'Very poor and moderate explicitly contradict rich and enormous.'),
        ('unresolved', 'Canada', 'has an annual uranium production of', '1000 tonnes', 6503319673937401246, 'No annual production amount is supplied.'),
        ('supported', 'Belgian Congo', 'is described as', 'the most important source of uranium', 6503319673937401246, 'Explicit statement about the most important source.'),
        ('unsupported', 'Belgian Congo', 'is described as', 'having no uranium', 6503319673937401246, 'Being the most important source contradicts having none.'),
        ('unresolved', 'Belgian Congo', 'has uranium ore priced at', 'ten dollars per tonne', 6503319673937401246, 'No price is supplied.'),
    ]
    challenges = [dict(example_id=f'challenge-{i+1:02}', origin='constructed_diagnostic',
                       expectation_author='Codex diagnostic author', independent_ground_truth=False,
                       expected_status=label, expectation_rationale=reason,
                       request=request('entity_field', sub, pred, obj, [cid]))
                  for i, (label, sub, pred, obj, cid, reason) in enumerate(specs)]
    write(target / 'constructed-challenges.json', challenges)
    with (target / 'requests.jsonl').open('x') as out:
        for r in selected + [c['request'] for c in challenges]:
            out.write(json.dumps(r, ensure_ascii=False) + '\n')
    derived = {p.name: sha(p.read_bytes()) for p in target.iterdir() if p.is_file() and p.name != 'snapshot.json'}
    write(target / 'diagnostics.json', dict(schema_version=1, snapshot_id=snapshot_id, files=derived,
          natural_unlabeled=3, constructed=9, expectations_per_class=3,
          annotation_status='No independent annotations; authored expectations only'))
    print(snapshot_id)


def matrix(pairs):
    counts = {a: {b: 0 for b in LABELS} for a in LABELS}
    for a, b in pairs:
        counts[a][b] += 1
    return counts


def score(path):
    snapshot = BASE / 'development-v1'
    check(snapshot)
    diag = json.loads((snapshot / 'diagnostics.json').read_text())
    for name, digest in diag['files'].items():
        assert sha((snapshot / name).read_bytes()) == digest
    # Off-diagonal fixture checks scorer orientation, unresolved inclusion, and totals.
    test = matrix([('supported', 'unsupported'), ('unsupported', 'unresolved'), ('unresolved', 'supported')])
    assert test['supported']['unsupported'] == test['unsupported']['unresolved'] == test['unresolved']['supported'] == 1
    rows = [json.loads(l) for l in path.read_text().splitlines()]
    assert rows[0]['record'] == 'verification_run' and rows[-1]['record'] == 'verification_summary'
    assert rows[0]['startup_error'] is None, 'No successful live verifier connection'
    assert rows[0]['manifest_sha256'] == sha((snapshot / 'requests.jsonl').read_bytes())
    assessments = {r['request']['claim']['id']: r for r in rows[1:-1]}
    requests = [json.loads(l) for l in (snapshot / 'requests.jsonl').read_text().splitlines()]
    assert len(assessments) == len(requests) == 12
    for req in requests:
        row = assessments[req['claim']['id']]
        assert row['request'] == req
        assert row['assessment']['result']['claim_id'] == req['claim']['id']
        assert row['assessment']['result']['evidence_ids'] == [e['id'] for e in req['evidence']]
        assert [s['reference'] for s in row['assessment']['sources']] == req['evidence']
        for source in row['assessment']['sources']:
            assert sha(source['passage']['text'].encode()) == source['reference']['content_hash'] == source['retrieved_hash']
    statuses = Counter(r['assessment']['result']['status'] for r in assessments.values())
    assert rows[-1]['claims_evaluated'] == 12
    for label in LABELS:
        assert rows[-1][label] == statuses[label]
    pairs, errors, failures = [], [], []
    for c in json.loads((snapshot / 'constructed-challenges.json').read_text()):
        a = assessments[c['request']['claim']['id']]['assessment']
        if a['failure'] is not None:
            failures.append(dict(example_id=c['example_id'], failure=a['failure']))
            continue  # Infrastructure failure is NOT semantic uncertainty.
        pairs.append((c['expected_status'], a['result']['status']))
        if pairs[-1][0] != pairs[-1][1]:
            errors.append(dict(example_id=c['example_id'], expected=pairs[-1][0], actual=pairs[-1][1], rationale=a['rationale']))
    report = dict(purpose='Development diagnostics; no independent ground truth or publication accuracy',
                  results_sha256=sha(path.read_bytes()), three_class_matrix_rows_expected_columns_observed=matrix(pairs),
                  semantic_completions=len(pairs), diagnostic_agreements=sum(a == b for a, b in pairs),
                  diagnostic_mismatches=errors, diagnostic_failures=failures,
                  all_failures=dict(Counter(r['assessment']['failure'] for r in assessments.values() if r['assessment']['failure'])),
                  natural_claims_unscored=3, scorer_self_check='passed: deliberately off-diagonal three-class fixture')
    write(path.with_suffix('.scores.json'), report)
    print(json.dumps(report, indent=2))


if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('command', choices=['prepare', 'check', 'score'])
    parser.add_argument('results', nargs='?', type=Path)
    args = parser.parse_args()
    if args.command == 'prepare':
        prepare()
    elif args.command == 'check':
        print(check(BASE / 'development-v1')['snapshot_id'])
    else:
        score(args.results)
