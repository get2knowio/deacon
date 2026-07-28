# Contract: `conformance certify` — extended for release certification

**Crate**: `deacon-conformance` (dev-only). **Hermetic**: no network, no Docker, no reference oracle
(FR-033a). Certification MUST succeed with the reference implementation absent and the network unavailable,
given otherwise-complete committed evidence (SC-013).

```
cargo run -p deacon-conformance -- certify [--json] [--report-dir <DIR>] [--manifest <FILE>]
                                           [--registry <DIR>] [--today <YYYY-MM-DD>]
```

## New flags

| Flag | Meaning |
|---|---|
| `--report-dir <DIR>` | Emit `certification.json` + `certification.md`. Defaults to not writing. |
| `--manifest <FILE>` | Execution manifest path. Defaults to `target/conformance/execution-manifest.json`. |

There is deliberately **no** flag that skips a blocking condition, downgrades one to a warning, or waives the
manifest requirement (FR-044). Tests construct failing states by pointing `--registry` at fixture trees, the
way every other gate in this system is tested.

## Exit codes

`0` certified · `1` not certified · `2` usage or IO error.

`--report-dir` writes the report on **both** `0` and `1`, so a blocked release still ships the artifact
explaining why. Only a `2` skips the write.

## Blocking conditions

All nine (FR-041), each naming its offending record (FR-042), **all reported in one run** (FR-043):

| # | Condition | Source |
|---|---|---|
| a | unclassified source change | existing V11–V14 / clause classes |
| b | applicable in-profile behavior uncovered | existing `Uncovered` blocker |
| c | stale snapshot | **new** — `snapshot::compare_staleness` over the certified profile |
| d | unknown runner omission | **new** — manifest ∪ replay results vs the applicable set |
| e | expired waiver | **existing** — the V6 disposition gate already blocks |
| f | unresolved gap | existing `Gap` blocker |
| g | incorrect oracle | **new** — recorded oracle ≠ declared stable pin, in snapshot provenance or manifest |
| h | missing required execution | **new** — V35 sub-cases (absent/incomplete/revision/stale) |
| i | silently skipped case | **new** — `V35-unaccounted`, plus any replay result outside the enumeration |

Four are new code; (e) needs tests only, since expired waivers already block through V6.

## Snapshot handling (research D5)

Three outcomes, deliberately not collapsed:

| Condition | Verdict |
|---|---|
| Snapshot present, an evidence-determining input drifted | **blocks** (c) |
| No snapshot for the profile **under certification** | **blocks** (h) |
| No snapshot for some other platform | informational, under `notCertified.noReferenceForPlatform` |

This refines 022's "a snapshot is a reviewed artifact, not a release gate" rather than reversing it. Blocking
on *every* missing snapshot would pressure maintainers to record snapshots to go green — the blessing
pressure this feature exists to remove.

## Report

Shape in data-model §7. Requirements:

- **All sixteen identity/environment/scope/coverage/exception/provenance fields present** (SC-004); a report
  missing any field is an error, not a partial write.
- **Explicit non-extension statement** (FR-035): the scope block names one profile and enumerates what
  certification does *not* cover. Linux/amd64/Docker must not be readable as certifying Podman.
- **Byte-reproducible** (FR-036, SC-005): no timestamps, absolute paths, or hostnames. `evaluationDate` comes
  from `--today`, never the clock. Run twice on different machines → identical bytes.
- **Enumerates what was not certified** (FR-037): inactive profiles, `non-testable`/`not-applicable` units,
  and platforms with no committed snapshot.
- **Refuses an inactive profile** (FR-045): certifying `prof-linux-amd64-podman-0870` exits `1` with an
  explicit refusal, never a vacuous pass over zero applicable units.

## Release wiring

`release.yml`'s `verify` job gains a dependency on a new container-execution job:

```
docker-execution (Docker) ──emits──> execution-manifest.json ──artifact──> verify ──> certify
```

`verify` itself stays free of Docker, network, and Node. If `docker-execution` fails or is skipped, no
manifest artifact exists and `certify` blocks on (h) — the gate holds without `verify` needing to know
anything about job orchestration.
