# Data Model: Harden the Parity Test Harness

**Feature**: 018-harden-parity-harness | **Date**: 2026-07-19

All persisted shapes are JSON. All writes use the atomic temp-file + `fs::rename`
pattern. Rust types live in `crates/parity-harness` with `serde` derives;
unknown fields in loaded records are rejected (`deny_unknown_fields`) — a typo in
a waiver or registry file must fail loudly, not silently widen a waiver.

## 1. OraclePin — `fixtures/parity-corpus/oracle.json`

The single authoritative pin (research D1). Read via `include_str!` in the
harness (malformed pin = compile-adjacent hard failure in every parity test) and
via `jq` in CI/Makefile.

| Field | Type | Rules |
|---|---|---|
| `package` | string | Exactly `@devcontainers/cli` |
| `version` | string | Exact semver, no range operators. Initial value `0.87.0` |

```json
{ "package": "@devcontainers/cli", "version": "0.87.0" }
```

**Lifecycle**: changed only by a deliberate re-certification PR (bump + full
certification-lane run). Never bumped by dependabot (not an npm manifest).

## 2. VerifiedOracle (in-memory, per test process)

Result of `Oracle::acquire()`; cached in a process-wide `OnceLock`.

| Field | Type | Notes |
|---|---|---|
| `path` | PathBuf | Binary actually invoked (override or PATH resolution) |
| `source` | enum `Override` \| `PathLookup` | Which resolution won (edge case: two oracles resolvable) |
| `version` | string | As reported by `--version`, must equal pin exactly |

**Validation**: version query bounded (2 min); non-zero exit, unparsable output,
or mismatch → `HarnessError::{OracleMissing, OracleUnverifiable, OracleVersionMismatch{found, required, path}}`.
No `VerifiedOracle` ⇒ no comparison can run ⇒ the test fails with the error's
display (FR-002, FR-004).

## 3. ParityRegistry — `fixtures/parity-corpus/registry.json`

Authoritative coverage enumeration (research D5; FR-022).

| Field | Type | Rules |
|---|---|---|
| `live_binaries` | array of LiveBinary | Every oracle-comparing test binary |
| `internal_consistency_binaries` | string[] | Reclassified binaries; MUST NOT appear in the parity profile |
| `corpora` | array of Corpus | Every case corpus with expectations |

**LiveBinary**: `{ "name": "parity_corpus_tier1", "docker_required": false, "kind": "corpus" | "scenario" }`
**Corpus**: `{ "id": "tier1", "path": "fixtures/parity-corpus", "min_cases": 20 }`,
`{ "id": "errors", "path": "fixtures/parity-corpus/errors", "min_cases": 9 }`

**Validation** (structural, every lane — `parity_registry_check`):
- every `live_binaries[].name` ↔ `crates/deacon/tests/<name>.rs` exists (both directions for `parity_*` files);
- nextest `parity` profile filter covers exactly `live_binaries` and none of `internal_consistency_binaries`;
- corpus dirs exist and discovered case count ≥ `min_cases` (FR-024);
- no parity source contains `#[ignore]` or legacy skip idioms (FR-023).

**Validation** (execution, certification lane — aggregator): one report fragment
exists per `live_binaries[].name`; zero unaccounted omissions (SC-003).

## 4. Waiver — `errors/*/expect.json` and `fixtures/parity-corpus/waivers/*.json`

One schema for both existing mechanisms (research D6; FR-010..FR-012). Records
live adjacent to what they waive.

| Field | Type | Rules |
|---|---|---|
| `id` | string | Unique across all waivers; stable slug (e.g. `errors/extends-missing`) |
| `scope` | Scope | What the waiver attaches to |
| `expect` | Expect | The characterized outcome |
| `rationale` | string | Non-empty; why the divergence is intentional (constitution IV link, PR#, etc.) |
| `added` | string (date) | ISO date the waiver was characterized |

**Scope** (tagged union):
- `{ "kind": "corpus_case", "corpus": "errors", "case": "extends-missing" }`
- `{ "kind": "state_field", "binary": "parity_observable_state", "fixture": "<name>", "field": "Config.Env.FOO" | "prefix*" }`

**Expect** (tagged union):
- `{ "kind": "both-reject" }` — both CLIs reject; comparable outcome, pass
- `{ "kind": "both-accept" }` — both accept; values compared normally
- `{ "kind": "deacon-stricter", "signal": ["substr", …]? }` — deacon rejects, oracle accepts; intentional (constitution IV)
- `{ "kind": "field-divergence", "ours": <json>, "reference": <json> }` — a specific normalized-value difference is expected

**State transitions**: `active` (matched case AND observed expected difference) →
pass-with-reference; `stale` (case gone, or expected difference no longer
observed) → run failure naming the record (FR-011). There is no silent state.

**Migration note**: existing `expect.json` files carry `expect` + optional
`signal`/`config`. This feature backfills ALL existing records to the full
schema in one pass (T023: add `id`, `scope`, `rationale`, `added`); the loader
accepts ONLY the full schema (`deny_unknown_fields`) — there is no legacy-shape
tolerance, so a missed backfill fails loudly at load time rather than silently
narrowing coverage. `signal` (stderr substrings, informational) stays as an
optional field of `expect.kind = "deacon-stricter"`; `config` (explicit
`--config` arg) is a schema-known optional field describing case input, not
waiver semantics.

## 5. ReportFragment — `target/parity/report/<binary>.json`

Written atomically by each live parity binary at end of run (research D8).

| Field | Type | Notes |
|---|---|---|
| `binary` | string | Registry name |
| `oracle` | { `version`, `path`, `source` } | From VerifiedOracle (FR-003) |
| `started` / `finished` | string (RFC3339) | Wall-clock timestamps via `chrono` (existing workspace dep) |
| `mode` | `"live"` | Only value today; schema field exists so replay can never masquerade (FR-017) |
| `cases` | CaseResult[] | One per executed comparison |
| `omitted` | { `case`, `reason` }[] | Registered-but-not-run cases with cause (FR-016) |

**CaseResult**:

| Field | Type | Notes |
|---|---|---|
| `case` | string | Case/fixture id |
| `outcome` | `pass` \| `pass-waived` \| `fail` | |
| `cause` | string? | Required when `fail`: `divergence` \| `oracle-failure` \| `oracle-timeout` \| `malformed-output` \| `normalization` \| `fixture-missing` \| `docker-missing` |
| `waivers_applied` | string[] | Waiver `id`s (required non-empty when `pass-waived`) |
| `diff_summary` | string? | Ranked diff (ref-only / deacon-only / value) when divergent |
| `raw` | { `deacon_stdout`, `deacon_stderr`, `oracle_stdout`, `oracle_stderr` } | Relative paths under `target/parity/raw/` (FR-020) |

**Failure to write a fragment fails the binary's test** (FR-018).

## 6. AggregatedReport — `target/parity/parity-report.json`

Produced by the `parity-report` bin; nonzero exit on any gap.

| Field | Type | Notes |
|---|---|---|
| `oracle` | { `pin`, `verified_version`, `path` } | Consistency-checked across all fragments (identical or fail) |
| `binaries` | ReportFragment summaries | Per registered live binary |
| `missing_fragments` | string[] | MUST be empty to pass (FR-022) |
| `stale_waivers` | string[] | MUST be empty to pass (FR-011) |
| `totals` | { `cases`, `passed`, `waived`, `failed`, `omitted` } | `failed` and unexplained `omitted` MUST be 0 |

## 7. Raw output artifacts — `target/parity/raw/<binary>/<case>/`

Four files per comparison: `deacon.stdout`, `deacon.stderr`, `oracle.stdout`,
`oracle.stderr` — verbatim bytes, written for every comparison (pass or fail).
Uploaded whole (`target/parity/`) as the CI artifact. Locatable from
`CaseResult.raw` (SC-006).

## 8. NormalizationProfile (in-memory)

One entry point per comparison type (research D7; FR-019):

| Profile | Input | Rules (summary) |
|---|---|---|
| `config` | read-configuration JSON | unwrap `{configuration}`; drop `configFilePath`; prune nulls/empty containers; sanitize dynamic ids (hex-12 / `${devcontainerId}` → `<ID>`) |
| `merged_config` | `--include-merged-configuration` JSON | `config` rules applied to the `mergedConfiguration` block |
| `container_state` | docker-inspect-derived state | noise-env subtraction; intentional-label prefix subtraction; compose project-prefix stripping; user normalization |

All return `Result<Normalized, HarnessError::Normalization>`; errors are test
failures, never fallbacks (FR-005).

## 9. HarnessError (in-memory, `thiserror`)

`OracleMissing { hint }` · `OracleVersionMismatch { found, required, path }` ·
`OracleUnverifiable { path, cause }` · `OracleFailure { case, status, stderr_path }` ·
`OracleTimeout { case, bound, partial_paths }` · `MalformedOutput { case, cause }` ·
`DockerMissing` · `FixtureMissing { path }` · `Normalization { case, cause }` ·
`WaiverStale { id }` · `WaiverInvalid { path, cause }` · `Report { cause }` ·
`CorpusTooSmall { corpus, found, min }`

Every variant's Display names the cause and, where applicable, the remedy — these
strings are the user-facing failure messages required by FR-005 and asserted by
the fault-injection tests (FR-021).

## Relationships

```text
OraclePin ──verified-by──▶ VerifiedOracle ──recorded-in──▶ ReportFragment ─┐
ParityRegistry ──enumerates──▶ LiveBinary ──writes──▶ ReportFragment ──────┼──▶ AggregatedReport
ParityRegistry ──enumerates──▶ Corpus ──contains──▶ CorpusCase             │        │
Waiver ──scoped-to──▶ CorpusCase / state field                             │        │
CaseResult ──references──▶ Waiver.id, raw artifact paths ◀─────────────────┘        │
AggregatedReport ──validated-against──▶ ParityRegistry (completeness) ──────────────┘
```
