# Contract: `conformance` binary (dev-only, `crates/conformance/`)

Package `deacon-conformance` (`publish = false`). Invoked as
`cargo run -p deacon-conformance -- <subcommand>`. Not part of the `deacon` CLI surface
(constitution II — contributor tooling, not a consumer command).

## Global flags

| Flag | Default | Meaning |
|------|---------|---------|
| `--registry <dir>` | `conformance/registry` | registry root (tests point this at fixtures) |
| `--today <YYYY-MM-DD>` | current UTC date | injected "today" for waiver-expiry evaluation (deterministic tests) |

## `conformance validate`

Structural validation (violation classes V1–V10 + SCHEMA, per data-model.md).

- **stdout**: nothing on success (text mode); with `--json`, a single JSON document:
  `{ "ok": bool, "violations": [ { "code": "V5", "record": "bhv-…", "message": "…" } ] }`
  (violations sorted by code, then record ID).
- **stderr**: diagnostics/logs only.
- **Exit codes**: `0` valid; `1` one or more violations (all violations reported, not
  first-failure); `2` usage/IO error (unreadable registry root).

## `conformance report`

Requires a valid registry (runs validation first; violations → exit 1, no report).

- Writes `report.json` and `report.md` to `--out-dir` (default `target/conformance/`).
- **Determinism contract**: byte-identical `report.json` for identical registry content —
  all collections ID-sorted, no timestamps, no absolute paths, no environment data
  (SC-004). `report.md` is derived from the same ordered data.
- **Exit codes**: `0` reports written; `1` registry invalid; `2` usage/IO error.

## `conformance certify`

Strict certification for the active profile (FR-025). Runs validation, then evaluates:
fails iff any gap record exists OR any in-profile behavior is uncovered.

- **stdout** (`--json`): `{ "certified": bool, "profile": "prof-…", "blocking": [ { "kind": "gap"|"uncovered", "id": "…" } ], "waived": [ "wvr-…" ] }`.
- **Exit codes**: `0` certified; `1` not certified (blocking items listed) or registry
  invalid; `2` usage/IO error.
- Wired as a blocking step in `.github/workflows/release.yml` (verify job). Per-PR CI
  runs only `validate` (via the hermetic `registry_valid` test).

## Output-stream contract

Follows constitution VI: JSON mode emits exactly one JSON document on stdout, everything
else on stderr; text mode keeps human-readable results on stdout, diagnostics on stderr.
