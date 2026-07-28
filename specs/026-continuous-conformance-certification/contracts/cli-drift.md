# Contract: drift detection — hermetic `conformance drift` + live `drift-scan`

Two programs, split on capability. The hermetic half validates and reports on committed observations and
runs in the fast lane; the live half performs the upstream probes.

## Hermetic: `conformance drift <check|report|scaffold>`

**Crate**: `deacon-conformance`. No network, no Docker, no oracle.

```
cargo run -p deacon-conformance -- drift <check|report|scaffold> [--drift <DIR>] [--json]
cargo run -p deacon-conformance -- drift proposal check [--proposal <FILE>]
```

### `drift check`

Reports **V36** violations over `conformance/drift/observations.json`:

```
V36 drf-spec-113500f4-a1b2c3d4 id is not derived from (kind ‖ pinnedRevision ‖ observedRevision)
V36 observations.json lastCompletedRun omits probed kind "cli-surface-change"
```

**Exit codes**: `0` clean · `1` violations · `2` usage/IO.

### `drift proposal check`

Validates an upgrade-proposal bundle: all seven section keys present (FR-029/FR-030) and regeneration
byte-identical (FR-031). A missing key and an empty `entries` array are **different** outcomes — the first is
a violation, the second is a clean finding. Rejecting an incomplete bundle here, in the hermetic lane, is
what keeps FR-030 enforceable on a PR without provisioning two oracles.

### `drift report`

Writes `target/drift/observations.{json,md}`. **Never gates** — exit code reflects writability only.

### `drift scaffold`

Stdout only, `"UNREVIEWED"` sentinels. Never writes.

## Live: `drift-scan`

**Crate**: `parity-harness`. Network via bounded `git` and `npm` subprocesses (research D6). No HTTP client,
no API token, no rate limit.

```
cargo run -p parity-harness --bin drift-scan [--kinds <k1,k2,…>] [--write]
```

**Probes**, one per drift kind:

| Kind | Probe |
|---|---|
| `spec-commit` | `git ls-remote` on the spec repo, compared to the pinned revision |
| `schema-change` | blob-filtered partial clone at HEAD; per-document SHA-256 vs `conformance/schemas/<pin>/manifest.json` |
| `reference-release` | `npm view @devcontainers/cli versions --json` vs the stable pin |
| `cli-surface-change` | `--help` surface of the candidate release vs the recorded CLI-surface revision |
| `upstream-test-or-changelog` | partial clone diff restricted to test and changelog paths |

**Permitted writes** (FR-024a): `conformance/drift/observations.json` and `target/drift/*` — nothing else.
Enforced as a path allow-list checked before any write is published.

**Abort on out-of-scope path** (FR-024b): if a proposed write touches a registry record, a committed
snapshot, or a pin, `drift-scan` exits non-zero naming the attempted path and writes nothing. It MUST NOT
drop the offending paths and commit the remainder — a silently narrowed diff misrepresents what the drift
implies.

**Exit codes**: `0` the scan ran, whatever it found · non-zero **only** on machinery failure (unreachable
upstream, unresolvable pin, unwritable artifact location, out-of-scope write attempt).

**Status reflects whether it ran, never what it found** (FR-026). A scan surfacing all five drift kinds exits
`0`. This is the rule the discovery lane already follows; a finding-dependent status would become a gate the
moment someone wired it into a required check, and upstream moving is not a defect in this repository.

## `oracle-upgrade-propose`

**Crate**: `parity-harness`. Needs network, Docker, and both oracle versions.

```
cargo run -p parity-harness --bin oracle-upgrade-propose --from <version> --to <version>
```

Writes `target/drift/upgrade-proposal.{json,md}` with all seven sections (data-model §5). Writes no pin,
no disposition, and no snapshot — there is no code path from this bin to any of them (FR-028, SC-006).

**Exit codes**: `0` bundle produced · non-zero on machinery failure. Producing a bundle that shows heavy
drift is a success, not a failure — the bundle is the deliverable.
