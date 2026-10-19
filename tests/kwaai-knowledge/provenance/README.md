# Provenance study tooling (KwaaiNet 486bcf6) — experiments branch

Measures whether graph facts stay traceable to the chunks that support them across `kwaainet rag dream run` cycles,
and whether an evidence gate applied before or after each cycle leaves fewer unsupported facts. Source-cited
details are in `FINDINGS.md`.

**Safety.** The original KB is only read and copied. `snapshot` copies the registered KB's data directory and
verifies the copy byte for byte. Every run then works on its own copy, and every `kwaainet` subprocess runs with
`KWAAINET_HOME` pointing at a private home whose config resolves the KB name to that copy. Runs fingerprint the
original before and after and fail if it changed.

## Workflow
1. `snapshot` (once): freeze the pre-cycle KB. Both claims (provenance and comprehension) start from this copy.
2. `check-binary`: confirms the binary runs and loads a copy.
3. `check-edit`: confirms the binary reads a graph edited by this tooling. Required before any gate writes, and
   again after any change to the binary, snapshot or tooling.
4. `run`: arms × repeats × cycles. Outputs go to `<out>/<run_id>/`, and nothing is appended to an existing run.
5. `report`: CSV plus a primary-endpoint plot. `label-export` and `label-import` handle the false-rejection sample.

## Arms (default)
| Arm | Each cycle | gate_mode |
|---|---|---|
| `ungated` | dream | none |
| `gate_before` | gate → dream | `written` if the edit check passed for this runtime; otherwise `not_computed` |
| `gate_after` | dream → gate | `written` if the edit check passed; otherwise `virtual` (decisions logged, nothing written, the arm runs ungated) |
| `no_similarity_merge` | dream with `--dedup-threshold 1.01` (exact-name and honorific merges still run) | none |
| `no_prune` | dream with `--prune-threshold 0` | none |
| `no_relations` | dream with `--no-relations` | none |

Repeats default to 3, because the LLM step is nondeterministic.

## Observation phases
The graph is re-read after every operation. At each snapshot cycle a summary row is written for every distinct
state: `pre_gate` (gate_before), `post_gate` (written gates), `post_dream` (all arms), with `end_of_cycle` and
`primary` flags. The primary comparison uses `primary_phase` (default `post_dream`: every arm measured right
after the same operation, so gate_after is never scored in the instant after its own clean-up). The immediate
gate effect is in `gate_immediate` on `post_gate` rows. See `FINDINGS.md` for the trade-off.

## Outputs
- `results.jsonl`: per-fact rows and summary rows (arm, repeat, cycle, phase).
- `events.jsonl`: every claim lost, added, rewritten or consolidated, at every cycle and phase, with `cause`
  and `confidence` (`observed`, `inferred`, `ambiguous`, `unknown`) and the candidate causes when ambiguous.
- `gate_decisions.jsonl`, `cycles.jsonl` (dream args and DreamReport), `RUN.json` (config, runtime binding).
- Summary row, gate-eligible facts (field + relation):
  - `unsupported_present_over_cycle0`: **primary endpoint**;
  - `traceable_over_present`, `unsupported_over_present`, `lost`, `added`, `added_unsupported`;
  - `loss_attribution` as `cause|confidence` counts, and `lost_to_prune_after_gate_isolation`;
  - `records_sharing_content`: records whose text equals another record's (kept apart, never merged).
- `descriptions`: the same measures for description sentences, separately (entity-level evidence, never gated).
- `eval` (end-of-cycle rows, every snapshot by default): `retrieval_keyword_recall` and
  `generation_keyword_recall` from `rag eval`. These are keyword-coverage scores, not Recall@k. Recall@k and
  MRR are `not_computed`.
- `report`: `<csv>`, `<csv stem>.stats.csv` (per arm × phase × cycle: every repeat value, mean, SD, min, max,
  95% t-interval), metadata, and plots of the mean with interval and every repeat.

Lexical-scorer output is labelled **NOT FOR REPORTING**. For research runs set `scorer: nli` and install
`requirements-provenance.txt`.

## WSL / Linux (one command)
Inside the WSL Ubuntu instance that hosts the node, from the repo root:
```bash
bash tests/kwaai-knowledge/provenance/run_wsl_checks.sh D6 llama3.1:8b
```
It runs the unit tests, snapshot, binary check, edit check, a one-cycle lexical smoke run and the report, and
writes `~/kwaai-prov/<timestamp>-results.tar.gz` (no KB copies). Add `WITH_NLI=1` to also test the NLI scorer.

## Windows PowerShell
Run from the repo root, on the experiments branch. Stop any running `kwaainet` processes before taking the snapshot.
```powershell
cd tests\kwaai-knowledge
python -m pip install -r provenance\requirements-provenance.txt
python -m pytest provenance\tests -q

$S = "$env:USERPROFILE\kwaai-prov"            # outside ~/.kwaainet
python -m provenance.check_snapshot  --kb D6 --dest "$S\snapshot-486bcf6" --kwaainet kwaainet --out-json "$S\snapshot.json"
python -m provenance.check_binary    --snapshot "$S\snapshot-486bcf6" --scratch "$S\scratch" --kwaainet kwaainet --out-json "$S\binary.json"
python -m provenance.check_edit_compat --snapshot "$S\snapshot-486bcf6" --scratch "$S\scratch" --kwaainet kwaainet `
    --model llama3.1:8b --inference-url http://localhost:11434 --out-json "$S\edit.json"

# One-snapshot smoke run (1 repeat, cycles 0-1, lexical scorer):
@"
model: llama3.1:8b
inference_url: http://localhost:11434
repeats: 1
snapshots: [0, 1]
edit_compat_result: $($S -replace '\\','/')/edit.json
"@ | Set-Content "$S\smoke.yaml"
python -m provenance.cli run --snapshot "$S\snapshot-486bcf6" --out "$S\runs" --config "$S\smoke.yaml" --max-cycle 1 --run-id smoke
python -m provenance.cli report "$S\runs\smoke\results.jsonl" "$S\smoke.csv"
```
`check_edit_compat` removes one field and deletes one relation on a copy, then requires `rag graph stats`,
`rag graph score --json` and a one-completion dream run to read the edited graph. It exits 0 only on PASS.
INCONCLUSIVE means the binary read the edited graph but the dream cycle completed nothing (check the inference
endpoint). A PASS is bound to the snapshot contents, the executable's bytes and version, the KB config template,
the check protocol and this tooling's writer code; if any of them changes, gates fall back to virtual or
`not_computed` until the check is rerun. Full runs: copy `default.yaml`, set `scorer: nli`, and drop
`--max-cycle`.
