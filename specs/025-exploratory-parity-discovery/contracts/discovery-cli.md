# Contract: Discovery Command Surface

**Feature**: `025-exploratory-parity-discovery`

Every command here is **development-only**. None is a `deacon` subcommand, and
`parity_registry_check` asserts on every PR that `deacon --help` gains nothing from any of them
(FR-059).

## The exit-status rule (applies to all commands below)

> A discovery command's exit status reflects **whether it ran**, never **what it found**.

This is the `coverage report` discipline extended to the whole surface (FR-058). A campaign that
finds 40 differences exits `0`; a campaign that cannot verify the oracle exits non-zero. The one
exception is stated explicitly per command and is always a *machinery* failure, never a finding.

Rationale: any command whose status depends on its findings becomes a gate the moment someone
wires it into CI, and a stochastic gate makes green non-reproducible. Keeping status
machinery-only is what lets the discovery lane be added to CI safely.

---

## Hermetic commands — `cargo run -p deacon-conformance -- discovery <cmd>`

No network, no Docker, no oracle. Safe in any lane, though FR-055 still keeps them out of the PR
lane's *selection*.

### `discovery check`

Validates the discovery data root. The hermetic gate that blocks a PR.

| | |
|---|---|
| **Reads** | `conformance/discovery/{findings,campaigns,corpus}.json`, `conformance/registry/` (to resolve `promotedTo` and pins) |
| **Writes** | nothing — read-only by construction |
| **stdout** | violation report (text) or a single JSON document under `--format json` |
| **Exit `0`** | no D1–D5 violations |
| **Exit `1`** | one or more violations, all reported in a single run |

Reports **all** violations in one pass rather than stopping at the first, matching `validate`.

### `discovery report`

Renders the queue and the campaign history.

| | |
|---|---|
| **Writes** | `target/discovery/queue.{json,md}` — git-ignored, byte-stable, no timestamps or absolute paths |
| **Exit `0`** | artifacts written |
| **Exit `1`** | could not write |

**Never gates.** Exit status reflects only whether the files were produced. A queue holding fifty
untriaged findings still exits `0`.

Content: findings by state and classification, the **counted untriaged bucket** (FR-029), the
no-longer-reproducing set with the campaign that last observed each (FR-033), promoted findings
with their cases, and per-campaign suppression counts (FR-034b).

### `discovery triage <fnd-id> --classification <c> [--notes <s>]`

Records a reviewer's classification. The **only** writer of `classification`.

| | |
|---|---|
| **Writes** | `conformance/discovery/findings.json` (atomic) |
| **Exit `0`** | classification recorded |
| **Exit `1`** | unknown finding, invalid classification, or a finding already in a terminal state |

Interactive-equivalent, not automated: no campaign invokes it. FR-028's "exactly one
classification" is enforced here and re-checked by `discovery check` (D2), so a hand edit that
bypasses the command still fails.

### `discovery split <fnd-id>`

Splits a signature-merged finding whose witnesses turn out to have different causes (FR-032).
Children carry `splitFrom`; the deduplication rule must never re-merge them.

### `discovery scaffold <fnd-id>`

Emits to **stdout** a skeleton behavior + case + fixture layout for promoting a finding, with
`UNREVIEWED` sentinels the registry loader rejects.

| | |
|---|---|
| **Writes** | **nothing** — stdout only |
| **Exit `0`** | skeleton emitted |
| **Exit `1`** | finding is non-promotable (`normalizer-defect` / `fixture-defect`, FR-035) or unknown |

Stdout-only is the same discipline as `inventory scaffold` and `clause scaffold`: **generation
never writes a hand-authored file**. Promotion is a human editing the registry with this output
as a starting point, which is what makes FR-036 hold — there is no code path from a finding to a
registry write.

---

## Live commands — `cargo run -p parity-harness --bin <bin>`

Require the verified pinned oracle; some additionally require Docker or network. All fail loudly
on a missing or mismatched prerequisite (FR-003) — never a silent skip.

### `discovery-campaign --seed <hex> --tier <t> [--budget-seconds <n>] [--lane <l>]`

Runs one campaign.

| | |
|---|---|
| **Requires** | verified pinned oracle (all tiers except `metamorphic`); Docker (`container-differential`); network (`corpus`) |
| **Writes** | `conformance/discovery/{findings,campaigns}.json` (atomic); `target/discovery/candidates/<fnd-id>/` |
| **Never writes** | anything under `conformance/registry/`, `conformance/snapshots/`, or `conformance/obligations/` (FR-036) |
| **stdout** | single JSON campaign outcome |
| **stderr** | progress and diagnostics via `tracing` |
| **Exit `0`** | the campaign ran to completion or to budget exhaustion — **regardless of findings** |
| **Exit `1`** | prerequisite failure, normalization failure, or an unwritable data root |

`--seed` is **required**, not defaulted. A defaulted seed would let a campaign run without its
reproducibility input being a conscious choice, and FR-001 depends on the seed being recorded
rather than inferred.

Tiers (research D10): `metamorphic` needs nothing external; `config-differential` is the
**nightly** scheduled tier; `corpus` is the **weekly** scheduled tier in the network-backed lane;
`container-differential` is invoked-only.

`corpus` has a scheduled cadence of its own rather than being invocation-only, because its whole
purpose is to be an ecological canary — and a canary that sings only when asked is not one. Weekly
rather than nightly because the corpus changes only when someone re-pins it, so nightly runs would
mostly re-confirm the previous night at network cost.

### `discovery-proof`

The FR-042a pipeline proof. Injects a known difference at the sealed evidence-source boundary and
requires it to traverse generation → comparison → minimization → candidate → classification →
promotable.

| | |
|---|---|
| **Writes** | `target/discovery/proof.json` |
| **Exit `0`** | every injected difference traversed the full pipeline |
| **Exit `1`** | any injected difference failed to surface, **or** any injection was inapplicable |

This is the one command whose status depends on an outcome — and it is not a finding-dependent
status. It asserts a property of the machinery, so a non-zero exit means the pipeline is broken,
which is exactly the thing that should fail a lane.

An injection that never landed exits `1` as `InjectionInapplicable` rather than being counted as
"nothing found" — a mis-authored proof record must never masquerade as a working pipeline
(inherited from `inject.rs`, research D7).

---

## Make targets

| Target | Runs |
|---|---|
| `make test-discovery` | `cargo nextest run --profile discovery`, then `discovery report` |
| `make test-discovery-proof` | `discovery-proof` |
| `make test-discovery-check` | `discovery check` (hermetic; also runs in the fast lane) |

## Lane wiring (FR-055 – FR-057)

`[profile.discovery]`'s `default-filter` is an **explicit `binary(=…)` allow-list**, never a
`discovery_*` glob. The glob would capture the hermetic guard `discovery_hermetic` and silently
remove it from the fast lane — the exact mistake the parity profile already documents having made
with `parity_harness_faults` and `parity_registry_check`.

Every live discovery binary is excluded from the `default-filter` of all six existing profiles
(`default`, `dev-fast`, `full`, `ci`, `mvp-integration`, `parity`). Exclusion from `parity` is
deliberate: the two lanes answer different questions and a campaign would exceed the parity lane's
budget.

`parity_registry_check` is extended to enforce registry ↔ `tests/*.rs` ↔ `.config/nextest.toml`
agreement for the discovery lane, so a binary cannot be added without its wiring. That check
already fails loudly on drift; FR-057 describes its shape exactly, so extending it is strictly
better than a parallel checker.
