# Phase 5 integration boundary: Phase 6 adapter version 1

This is a Phase 6 normalized import format, **not** a proposed final Phase 5
schema. No Phase 5 production code or gate type is imported or duplicated.
`DecisionAdapter::normalize(raw)` is the extension point. `FixtureAdapter` only
reads the strict Phase 6 envelope; it must not be pointed directly at unknown
Phase 5 JSONL or historical Phase 4 lineage records.

The envelope requires `adapter_version: 1`, a nonempty `producer_schema`,
`corpus_version`, `model_identifier`, `seed`, `repetition` (zero-based), `cycle`
(one-based) and `records`. Full examples are committed under `fixtures/`.
Each observation carries exact claim ID/content, original evidence references,
optional verifier judgment, separately optional traceability and authentication
assessments, optional authentication audit basis and optional paired timings.
Unknown fields are rejected. Future adapter versions require explicit migration.
A missing judgment remains null; `unresolved` is an explicit verifier outcome.

A future Phase 5 adapter must:

1. Identify and validate the actual producer schema and preserve the raw export.
2. Recover original claims/evidence only from authoritative retained snapshots;
   fail or leave unknown fields unknown when records alone are insufficient.
3. Validate provenance against the producer's authoritative lineage/signatures;
   never equate reference presence or a matching hash with authentication.
4. Bind corpus, model, seed and cycle to upstream execution metadata. Record an
   auditable authentication method/record identity for any authentication result.
5. Normalize verifier judgments without converting them into independent labels.
6. Document and test the mapping with actual producer fixtures before use.

`HypotheticalPolicy` is scoped to this offline crate. The provisional analysis
mapping is baseline/allow = accept all; reject_unsupported = reject only explicit
unsupported; require_supported = accept only explicit supported. These names
prepare comparisons with Phase 5 Allow, RejectUnsupported and RequireSupported.
They make no assertion about Phase 5's final unresolved/error handling. If the
final producer semantics differ, version this mapping and add compatibility tests
before comparing results. Decisions are counterfactual acceptance flags; no graph
writes or enforcement callbacks exist in this crate. Actual intervention must
use a separately authorized graph experiment, not an option on this replay CLI.

Run/checkpoint schema 2 and evaluator implementation identity are independent of
this adapter-version-1 format. This remediation does not change producer fields
or provisional policy semantics. Adapter/schema source definitions are included
in the evaluator fingerprint through `src/lib.rs`; when a real adapter adds a
source module or schema file, extend the fingerprint coverage list as well.
