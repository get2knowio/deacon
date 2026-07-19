# Quickstart: Hardened Parity Harness

**Feature**: 018-harden-parity-harness

## Run live parity locally

```bash
# 1. Provision the pinned oracle (one-time; version comes from the pin file)
npm install -g @devcontainers/cli@"$(jq -r .version fixtures/parity-corpus/oracle.json)"

# 2. Run the whole certified surface (nextest profile + report aggregation)
make test-parity
# equivalent to:
#   cargo nextest run --profile parity
#   cargo run -p parity-harness --bin parity-report

# 3. Inspect results
cat target/parity/parity-report.json          # aggregated report
ls  target/parity/report/                     # per-binary fragments
ls  target/parity/raw/<binary>/<case>/        # raw stdout/stderr, both CLIs
```

Wrong or missing oracle? The run **fails** immediately with the found vs required
version (0.87.0) and the resolution path — that's the feature working. Use
`DEACON_PARITY_DEVCONTAINER=/path/to/devcontainer` to point at a specific binary.

## What runs where

| Lane | Parity content |
|---|---|
| `make test-nextest-fast` / `dev-fast` | `parity_registry_check` + `parity_harness_faults` (hermetic guards) only; live parity not selected |
| `make test-nextest` / default / full / ci | same — live parity visible as *not selected*, never as vacuous green |
| `make test-parity` / `--profile parity` | all registered live binaries + aggregator; requires oracle 0.87.0 (+ Docker for Docker-required binaries) |
| CI `parity / live-certification` (`.github/workflows/parity.yml`) | nightly on main, manual dispatch, and PRs touching parity paths; provisions oracle from the pin, runs the two commands, uploads `target/parity/` |

## Certify against a new oracle version (deliberate re-certification)

1. Edit `fixtures/parity-corpus/oracle.json` (single authoritative pin).
2. Open a PR — the path trigger runs the certification lane with the new version.
3. Triage divergences: fix deacon, or add a waiver record with rationale.
4. Merge only with the lane green.

## Add or change parity coverage

- **New live parity binary**: create `crates/deacon/tests/parity_<name>.rs` using
  `parity_harness::{Oracle, exec, normalize, report}`; register it in
  `fixtures/parity-corpus/registry.json`; add nextest overrides in ALL profiles
  (parity profile + exclusions). `parity_registry_check` fails until all three
  agree.
- **New corpus case**: drop the fixture under the corpus dir; bump `min_cases`
  if you want the floor to rise. Divergent-by-design case → add a waiver record
  next to it (`expect.json` with schema fields incl. `rationale`).
- **Waiver hygiene**: a waiver whose difference stops reproducing fails the run
  as stale — delete or update it in the same PR that changes the behavior.

## Prove the harness can't lie (acceptance suite)

```bash
cargo nextest run -E 'binary(=parity_harness_faults) or binary(=parity_registry_check)'
```

Hermetic; no oracle, Docker, or network needed. Covers: wrong-version stub,
missing oracle, missing docker stub, oracle crash, malformed output, injected
divergence (fail → waived-pass → stale-fail), normalization failure, timeout,
registry↔files↔profile drift, and the no-`#[ignore]`/no-skip-idiom source audit.

## Diagnosing a red certification run

1. `parity-report` output names the gap (failed cases, missing fragments, stale
   waivers).
2. Open the fragment for the failing binary → `cases[].cause` + `diff_summary`.
3. Compare raw outputs (`raw/<binary>/<case>/{deacon,oracle}.stdout`) — verbatim,
   pre-normalization.
4. Real divergence → fix deacon or characterize with a waiver (rationale
   required). Harness/infra cause (`oracle-timeout`, `docker-missing`) → fix the
   environment; these can never be waived.
