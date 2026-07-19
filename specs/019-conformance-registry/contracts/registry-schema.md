# Contract: registry file schemas

Authoritative record shapes are specified in `data-model.md`; this contract fixes the
serialization rules that validation enforces and tests assert.

## General

- Strict JSON (not JSONC). UTF-8, LF line endings, trailing newline.
- Unknown fields in registry records are **rejected** (`deny_unknown_fields`). The
  registry is a deacon-owned modeled format — constitution IV's "strict on mistakes"
  side applies; the "preserve the unmodeled" side is for *devcontainer.json*, not for
  deacon's own data files. Schema evolution goes through `schemaVersion` bumps.
- Every collection file carries `{ "schemaVersion": 1, "records": [ … ] }` except
  per-waiver files, which are a single record object (parity-waiver compatibility).
- IDs: `^(rev|src|dim|chan|prof|bhv|case|gap|wvr|ext)-[a-z0-9]+(-[a-z0-9]+)*$`, prefix
  must match record type (violation V2).
- Records within a file MUST be sorted by `id` (validated — keeps diffs canonical and
  reports deterministic).
- Dates are ISO `YYYY-MM-DD` strings; compared lexicographically.

## Enumerations (closed sets)

| Field | Values |
|-------|--------|
| SourceUnit.inventory | `schema`, `spec`, `cli`, `observed` |
| SourceRevision.kind | `spec`, `schema`, `oracle`, `cli-surface` |
| Behavior.spec | `conformant`, `nonconformant`, `unspecified`, `not-applicable` |
| Behavior.reference | `aligned`, `divergent`, `unknown`, `not-applicable` |
| Behavior.decision | `follow-spec`, `align-with-reference`, `deacon-extension`, `intentional-divergence`, `unresolved-gap` |
| Gap.kind | `coverage`, `knowledge`, `implementation` |
| Waiver.expect.kind | `both-accept`, `both-reject`, `deacon-stricter`, `reference-stricter` (preserved parity schema) |

Any value outside a closed set is a SCHEMA violation with file + JSON-pointer location.

## Waiver-file compatibility

`conformance/registry/waivers/<id>.json` preserves the parity-harness waiver shape
(`scope`, `expect`, `rationale`, `added`) and adds `behaviors` (registry links) and
`expires`. `parity-harness` consumes these records through `deacon-conformance`'s loader;
its `WaiverSet` query API (`get`, `corpus_case`, `corpus_cases`, `state_field_waivers`,
`stale_among`) is unchanged for callers. The legacy locations
`fixtures/parity-corpus/waivers/` and `fixtures/parity-corpus/errors/*/expect.json` are
removed; `parity_registry_check` is updated to enforce the new location structurally.
