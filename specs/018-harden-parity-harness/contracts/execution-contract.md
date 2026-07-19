# Contract: Parity Execution (nextest-only)

**Feature**: 018-harden-parity-harness

This is the single sanctioned way parity checks execute. Anything else (direct
`cargo test`, ad-hoc scripts) is a contract violation; the registry check and
code review enforce it.

## Entry points

| Entry point | Definition | Gating logic allowed |
|---|---|---|
| `cargo nextest run --profile parity` | The contract | All of it (in harness crate) |
| `cargo run -p parity-harness --bin parity-report` | Aggregation + completeness gate | Registry/fragment validation only |
| `make test-parity` (and `test-parity-all`) | Thin alias: the two commands above, in order | NONE — pure delegation |
| `.github/workflows/parity.yml` | Provision oracle from pin, then the two commands above, then artifact upload | Provisioning only; verification stays in harness |

## nextest profile contract (`.config/nextest.toml`)

- `[profile.parity]` — NEW. `default-filter` selects exactly the registry's
  `live_binaries` (by `binary(=name)` clauses). Test-group assignments: container
  scenario binaries → `parity` group (serial-ish, max-threads=2); config-only
  corpus binaries → `parity-cli` group. Slow-timeouts: 15 m for ALL live parity
  binaries, including the corpus runners — the 2 min bound is per CLI
  *invocation* (enforced inside the harness), while one corpus `#[test]` runs
  ~23 cases × 2 CLIs, so per-test timeouts must be sized for the whole sweep.
- `default`, `full`, `docker`, `ci`, `mvp-integration` profiles — live parity
  binaries are REMOVED from selection (`default-filter` exclusion), so these
  lanes truthfully show them as not run. `dev-fast` already excludes them.
- `parity_registry_check` and `parity_harness_faults` are hermetic and MUST be
  selected in default/dev-fast/full/ci profiles (they guard the trust property on
  every PR) and are NOT part of `[profile.parity]`'s live set.
- Renamed `consistency_*` binaries get ordinary (non-parity) group assignments in
  ALL profiles, per the repository's 3-spot rule per binary.

## Environment variables

| Variable | Meaning | Default |
|---|---|---|
| `DEACON_PARITY_DEVCONTAINER` | Path override for the oracle binary | unset → resolve `devcontainer` on PATH |
| `DEACON_PARITY_DOCKER` | Path override for the docker CLI (fault-injection seam) | unset → `docker` on PATH |
| `DEACON_PARITY_REPORT_DIR` | Override for the artifact root | `<workspace_root>/target/parity/` — anchored to the workspace root (derived from `CARGO_MANIFEST_DIR`), NOT the process CWD (cargo-test CWD is the package dir); the aggregator resolves identically |

The deacon binary under test is passed to the harness explicitly: test binaries
supply `env!("CARGO_BIN_EXE_deacon")` (only the test crate can expand it); the
harness never guesses a `target/…/deacon` path.
| `DEACON_PARITY` | **RETIRED** — removed entirely; selection is by profile | — |
| `DEACON_PARITY_UPSTREAM_READ_CONFIGURATION` | **RETIRED** — arg template moves into harness with fixed default | — |

## Pass/fail semantics (normative)

A parity test (or corpus case) reports **pass** iff: oracle verified as the
pinned version AND the comparison executed to completion AND (outputs equivalent
under the shared normalization OR every difference is covered by an active
waiver, referenced in the report).

**Fail**, with cause-specific message, on ANY of: oracle missing / wrong version /
unverifiable; Docker missing (Docker-required checks); fixture missing; oracle
crash (where success expected); oracle timeout (2 min config-only, 15 min
lifecycle); malformed/unparseable oracle output; normalization failure; unwaived
difference (ref-only, deacon-only, or value mismatch); stale or invalid waiver;
corpus below registered minimum case count; report/artifact write failure.

**Never permitted**: silent early-return, `#[ignore]`, skip-to-pass conversion,
falling back to raw comparison on normalization failure, or any pass without a
`VerifiedOracle`.

## Exit-code contract

| Command | 0 | non-0 |
|---|---|---|
| `cargo nextest run --profile parity` | every selected test passed under the semantics above | any test failed (nextest propagates) |
| `parity-report` bin | all registered fragments present, zero failures, zero stale waivers, zero unexplained omissions, consistent oracle across fragments | any gap; message enumerates it |
| `make test-parity` | both above succeeded | first failing step |

## CI status naming (FR-017)

The certification workflow's check name identifies liveness explicitly:
`parity / live-certification`. If a snapshot/replay mode is ever introduced it
MUST use a distinct check name (`parity / replay-*`) and `mode` value in reports;
the `mode` field exists in the report schema from day one.
