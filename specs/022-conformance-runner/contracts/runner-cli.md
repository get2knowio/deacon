# Contract: Runner & Refresh CLI

Dev-only tooling — **not** a shipped `deacon` subcommand (Principle II). Two entry points, split hermetic vs live (research D5).

## 1. Hermetic — `deacon-conformance` bin (`cargo run -p deacon-conformance -- …`)

Extends the existing `conformance` subcommand group (`validate`/`report`/`certify`/`inventory`/`clause`) with `snapshot`:

| Command | Effect | Lane | Writes? |
|---------|--------|------|---------|
| `snapshot check [--case <id>] [--platform <os-arch>]` | recompute hashes + probe env; compare to committed provenance; report stale / no-reference-for-platform | dev-fast / PR | no |
| `snapshot diff <old-dir> <new-dir> [--format json\|md]` | deterministic drift between two snapshot trees | dev-fast | no |

`validate` gains the new case/allowed-difference/snapshot violation classes (continues V-series). `certify` (release gate) surfaces snapshot coverage + any `no-reference-for-platform` as non-blocking info, and blocks on unclassified/gap as today.

### Exit codes (hermetic)

| Code | Meaning |
|------|---------|
| 0 | all checks pass |
| 1 | staleness / validation violation (names the mismatched field/record) |
| 2 | malformed input / dangling reference (fail-loud, FR-003) |

## 2. Live — `parity-harness` `conformance-snapshot` bin (`cargo run -p parity-harness --bin conformance-snapshot -- …`)

Requires the verified pinned oracle + Docker/Node; fail-loud (`HarnessError`) if absent — no skip.

| Command | Effect | Writes? |
|---------|--------|---------|
| `refresh [--case <id>] [--platform <os-arch>]` | run cases against the reference, capture + normalize, write `provenance/raw/normalized` atomically; print review diff | yes (reviewed) |

## 3. Live differential run — `parity_conformance_runner` (nextest binary)

The thin test-binary shell (`crates/deacon/tests/parity_conformance_runner.rs`) that drives the runner over declarative cases under `--profile parity` only. Fail-loud on missing prereqs; never `#[ignore]`.

- Registered in `fixtures/parity-corpus/registry.json`.
- nextest overrides added in **all** profiles: parity `default-filter` allow-list (add its `binary(=…)`), plus the `dev-fast`/`default`/`full`/`ci`/`mvp-integration` exclusions — mirror the CLAUDE.md "3-spot" rule for a docker-exclusive binary. `parity_registry_check` fails if any spot is missing.

## Report shape (FR-041 — deterministic, VI output contract)

Single JSON document on **stdout**; all logs/progress on **stderr** via `tracing`:

```json
{
  "schemaVersion": 1,
  "normalizerVersion": "2",
  "cases": [
    {
      "caseId": "case-up-postcreate-env",
      "oracleType": "live-differential",
      "behaviors": ["bhv-lifecycle-postcreate-env"],
      "channels": [
        { "channel": "chan-exit-code", "outcome": "agree", "detail": null },
        { "channel": "chan-injected-process", "outcome": "allowed-difference",
          "detail": { "observablePath": "chan-injected-process.env.TZ", "waiverId": "wvr-postcreate-tz" } }
      ],
      "overall": "allowed-difference"
    }
  ]
}
```

Records are emitted in declaration order (`Vec`/`IndexMap`, never `BTreeMap`) — VI ordering compliance. Determinism: no timestamps, no absolute paths in the report body (paths are tokenized), matching `deacon-conformance report`'s byte-stable convention.

## Runner exit codes (live)

| Code | Meaning |
|------|---------|
| 0 | every case `overall ∈ { agree, allowed-difference }` |
| 1 | at least one `diverge` (uncharacterized divergence) |
| 3 | at least one `stale` snapshot on a snapshot-oracle case |
| 4 | harness error (missing oracle/Docker/Node, normalization failure) — cause-specific `HarnessError` |

(Codes are distinct so CI can tell an uncharacterized divergence from stale evidence from an environment fault — FR-041, Constitution IV/VI.)
