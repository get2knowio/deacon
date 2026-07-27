# Contract: `coverage` command group

**Command**: `cargo run -p deacon-conformance -- coverage <generate|check|report|scaffold>`

Dev-only (Principle II). MUST NOT appear in the shipped `deacon` CLI; `parity_registry_check`
asserts this on every PR. Hermetic: no network, no Docker, no reference oracle.

## Global flags

| Flag | Meaning |
|---|---|
| `--registry <dir>` | Registry root; defaults to `conformance/registry`. Fixture registries use this |
| `--today <YYYY-MM-DD>` | Evaluation date for waiver expiry; defaults to the system date |

## `coverage generate`

Regenerates `conformance/obligations/obligations.json` from the scenario model, the
applicability rules, the high-risk triples, and the behavior records.

| Property | Contract |
|---|---|
| Output | Exactly one file, written atomically (temp + `fs::rename`) |
| Determinism | Same inputs → byte-identical output, on any machine |
| Ordering | Units sorted by `id` |
| Side effects | **Never** writes a disposition file, a case, or a report |
| Exit `0` | Generation succeeded |
| Exit `1` | Model integrity failure (V26) — reported before any write |

`--out <path>` redirects the output for inspection without touching the committed file.

## `coverage check`

Regenerates **in memory** and byte-compares against the committed file. This is the CLI face
of the hermetic determinism test.

| Exit | Meaning |
|---|---|
| `0` | Committed obligations match a fresh regeneration |
| `1` | V27 — drift. Names the first differing unit id and whether it is added, removed, or changed |

## `coverage report`

Writes the four report families to `target/conformance/` (git-ignored).

| Artifact | Contents |
|---|---|
| `coverage-pairwise.{json,md}` | Every valid pair, its bucket, and its covering case or disposition |
| `coverage-triples.{json,md}` | Every selected triple, its covering case or blocking gap, and its `reason` |
| `coverage-operations.{json,md}` | Per operation: input classes and observables exercised |
| `coverage-observables.{json,md}` | Per channel: case count and the fields compared |

| Property | Contract |
|---|---|
| Read-only | MUST NOT record, refresh, or repair evidence (FR-063) |
| Byte-stable | No timestamps, no absolute paths, no run-dependent ordering (FR-062) |
| Exit `0` | Reports written; **exit code does not depend on coverage** — reporting is not a gate |
| Exit `1` | The registry failed to load, or the model is invalid |

`--out-dir <dir>` redirects output.

## `coverage scaffold`

Emits skeleton `odp-` disposition records to **stdout** for every undispositioned obligation,
each carrying an `"UNREVIEWED"` sentinel the loader rejects.

| Property | Contract |
|---|---|
| Never writes the registry | stdout only, always |
| Sentinel | `"rationale": "UNREVIEWED"` — a scaffold committed unedited fails V29 |
| Exit `0` | Even when nothing is undispositioned (empty output is a valid answer) |

## Gating summary

| Command | Runs in | Blocks |
|---|---|---|
| `coverage check` | every PR, via a hermetic test | V27 drift |
| `validate` (extended) | every PR, via `registry_valid` | V26 – V29 |
| `certify` (extended) | release path | V28, V29, plus existing gaps/uncovered |
| `coverage report` | on demand + certification lane | nothing — it reports |

The split matters: **reporting never gates and gating never reports**. A command that both
measured coverage and decided the build's fate would make widening the report the cheapest
way to go green.
