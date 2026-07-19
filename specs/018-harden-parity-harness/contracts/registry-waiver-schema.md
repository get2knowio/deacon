# Contract: Parity Registry & Waiver Schemas

**Feature**: 018-harden-parity-harness

## ParityRegistry — `fixtures/parity-corpus/registry.json`

The authoritative enumeration of claimed parity coverage (FR-022). Reviewed as
data; changing it is changing the certification claim.

```json
{
  "live_binaries": [
    { "name": "parity_read_configuration", "kind": "scenario", "docker_required": false },
    { "name": "parity_exec",               "kind": "scenario", "docker_required": true },
    { "name": "parity_build",              "kind": "scenario", "docker_required": true },
    { "name": "parity_up_exec",            "kind": "scenario", "docker_required": true },
    { "name": "parity_observable_state",   "kind": "scenario", "docker_required": true },
    { "name": "parity_state_diff",         "kind": "scenario", "docker_required": true },
    { "name": "parity_corpus_tier1",       "kind": "corpus",   "docker_required": false, "corpus": "tier1" },
    { "name": "parity_corpus_merged",      "kind": "corpus",   "docker_required": false, "corpus": "tier1" },
    { "name": "parity_corpus_errors",      "kind": "corpus",   "docker_required": false, "corpus": "errors" }
  ],
  "internal_consistency_binaries": [
    "consistency_remote_env_flags",
    "consistency_env_probe_flag"
  ],
  "corpora": [
    { "id": "tier1",  "path": "fixtures/parity-corpus",        "min_cases": 20 },
    { "id": "errors", "path": "fixtures/parity-corpus/errors", "min_cases": 9 }
  ]
}
```

Enforced by `parity_registry_check` (every lane, hermetic):

- bidirectional file↔registry match for `parity_*` test sources;
- `[profile.parity]` filter covers exactly `live_binaries`, and none of
  `internal_consistency_binaries` (FR-013, FR-014);
- corpus paths exist; discovered cases ≥ `min_cases` (FR-024);
- source audit: no `#[ignore]`, no `gated(`/`upstream_available(`/`DEACON_PARITY`
  legacy idioms in any parity source (FR-006, FR-023).

Enforced by the aggregator (certification lane): fragment present per
`live_binaries` entry (FR-022).

## Waiver records

One schema, one loader (`parity_harness::waiver`), records adjacent to cases.
Locations: `fixtures/parity-corpus/errors/<case>/expect.json` (corpus-case
scope) and `fixtures/parity-corpus/waivers/*.json` (state-field scope).

```json
{
  "id": "errors/extends-missing",
  "scope": { "kind": "corpus_case", "corpus": "errors", "case": "extends-missing" },
  "expect": { "kind": "deacon-stricter", "signal": ["extends"] },
  "rationale": "Constitution IV fail-fast: reference read-configuration is a lenient parse-and-echo and does not resolve extends; deacon validates eagerly. Characterized divergence, see CLAUDE.md 'Verified Non-Bugs'.",
  "added": "2026-07-19"
}
```

```json
{
  "id": "extends-child-merged",
  "scope": { "kind": "corpus_case", "corpus": "tier1", "case": "extends-child" },
  "expect": { "kind": "reference-stricter", "signal": ["image"] },
  "rationale": "deacon resolves extends eagerly and produces a full merged config; the reference CLI does not resolve extends and errors (exit 1). Ahead-of-spec deacon capability, see REPORT.md 'extends-child' and issue #297.",
  "added": "2026-07-19"
}
```

```json
{
  "id": "state/compose-project-label",
  "scope": { "kind": "state_field", "binary": "parity_observable_state", "fixture": "compose-postgres", "field": "Config.Labels.com.docker.compose.project" },
  "expect": { "kind": "field-divergence", "ours": "deacon-…", "reference": "devcontainer-…" },
  "rationale": "…",
  "added": "2026-07-19"
}
```

Rules:

- `expect.kind` ∈ `both-reject` | `both-accept` | `deacon-stricter` |
  `reference-stricter` | `field-divergence`.
- `deacon-stricter` (deacon rejects, reference accepts) and `reference-stricter`
  (deacon accepts, reference rejects — the inverse ahead-of-spec capability, e.g.
  eager `extends` resolution at merged-config time) both take an optional `signal`
  (informational stderr substrings, not part of the pass/fail decision). In the
  Tier-1 config/merged corpora these two also govern the process-exit-class
  decision: a matching, right-direction `corpus_case` waiver turns a
  deacon-success/oracle-failure (or the inverse) mismatch into a waived pass;
  wrong-direction or missing → the case fails.
- Optional `config` (string): corpus-case input detail — an explicit `--config`
  argument for the case (carried over from the legacy `expect.json` shape). It is
  a modeled, schema-known field passed through to case execution; it plays no
  part in waiver semantics.
- `rationale` non-empty; `id` unique across all records; unknown fields rejected.
- `field` supports exact match or trailing-`*` prefix (carried over from the
  retired Rust matcher semantics).
- **Staleness** (FR-011): each run, every loaded record must match an existing
  case/field AND its expected difference must be observed; otherwise the run
  fails naming the record id. Waived passes list the record ids they consumed
  (FR-010).
- The two retired mechanisms — bare pre-schema `expect.json` and the
  `KNOWN_INTENTIONAL_DIVERGENCES`/`KNOWN_GAPS` consts — are gone; the loader is
  the only reader and validator.
