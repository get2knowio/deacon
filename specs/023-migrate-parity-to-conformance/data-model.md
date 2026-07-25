# Phase 1 Data Model: Migrate Parity Assets into the Declarative Conformance System

**Branch**: `023-migrate-parity-to-conformance` | **Date**: 2026-07-24
**Inputs**: [spec.md](./spec.md) Key Entities · [research.md](./research.md) §1 (the enumerated baseline), D1–D9

All records are strict JSON (no comments, no trailing commas), version-controlled, written atomically (temp file + `fs::rename`), and byte-stable across regeneration.

**Ownership rule** (mirrors the existing V14 provenance discipline): `baseline.json` is **machine-owned** — the sole output of `baseline generate`, hand edits caught as drift. `mapping.json` and `residuals.json` are **hand-authored** — generation never writes them. The coverage report is **derived** and git-ignored.

---

## 1. BaselineUnit — `conformance/migration/baseline.json`

The frozen, machine-derived inventory of pre-migration coverage. One record per baseline unit, per FR-049.

| Field | Type | Rules |
|---|---|---|
| `id` | string | Derived, never authored: `<program>::<case-id>` for per-case programs, `<program>::<test-fn>` for guard programs, `realworld::<name>` for manifest entries. Unique across the file. |
| `program` | string | The carrier that reports this unit today (e.g. `parity_corpus_tier1`). |
| `category` | enum | `live-per-case` \| `hermetic-guard` \| `internal-consistency` \| `external-corpus-entry` |
| `dockerRequired` | bool | From `registry.json`'s `live_binaries`. |
| `assertion` | string | What the unit asserts, in one sentence. Human-authored *once* at freeze time, then immutable. |
| `channels` | string[] | Observable channels the unit inspects today (resolvable `chan-*` ids). |
| `errorPath` | bool | True when the unit's expectation is a rejection, a diagnostic, or a non-zero exit. Drives the FR-042 direction check. |
| `fixtures` | string[] | Repo-relative fixture dirs consumed, or `inline:<fn>` for code-authored fixtures. |
| `diffClasses` | string[] | Difference/result classes this unit can currently report. |

**Envelope**: `{ "schemaVersion": 1, "revision": "<git sha at freeze>", "generatedFrom": {...}, "records": [...] }`

**Enumeration contract**: `baseline generate` MUST derive corpus units by calling the *production* discovery functions (`discover_tier1_cases`, `discover_error_cases`) rather than re-walking directories — the origin of the 25-vs-24 error in research D1.

**Frozen totals** (research §1): 111 units — 91 `live-per-case`, 16 `hermetic-guard`, 4 `internal-consistency`; plus 33 `external-corpus-entry` recorded per D8.

---

## 2. MigrationMapping — `conformance/migration/mapping.json`

The explicit unit → destination mapping. Equal counts do not prove conservation (research D7); this table is the proof.

| Field | Type | Rules |
|---|---|---|
| `unit` | string | A `BaselineUnit.id`. Must resolve → else **V21**. |
| `disposition` | enum | `migrated` \| `deduplicated` \| `residual` \| `retired` |
| `caseIds` | string[] | Required and non-empty for `migrated`/`deduplicated`; each must resolve in `cases.json`. Empty for `residual`/`retired`. |
| `residualId` | string | Required iff `disposition: residual`; must resolve in `residuals.json`. |
| `rationale` | string | Required for `deduplicated` (which case absorbs it and why they are the same behavior) and for `retired` (why the loss is intentional and acceptable). |
| `fixtureMapping` | object[] | `{ from, to }` pairs, one-to-one (FR-012). A `from` appearing twice, or a `to` fed by two `from`s, is **V22**. |

Every baseline unit MUST appear exactly once. A missing unit is **V21** (orphan test); a `caseIds` entry naming a case no unit maps to is **V21** in the reverse direction (orphan case).

---

## 3. ResidualRecord — `conformance/registry/residuals.json`

A unit that cannot yet be expressed as data. **Never blocks certification** (FR-054) — it is representation debt, not a coverage gap. Distinct from `gaps.json`, which continues to block.

| Field | Type | Rules |
|---|---|---|
| `id` | string | `res-<slug>`. |
| `units` | string[] | Baseline units it covers; non-empty. |
| `blockedCarrier` | string? | The program that cannot be deleted while this residual stands. **Optional for `external-corpus-entry` residuals only** — the 33 pinned manifest entries have no carrier program to block (research D8). Required for every other category; absent-and-required is **V23**. |
| `missingCapability` | string | A specific named capability (e.g. "cross-CLI container-state snapshot comparison"), never a vague "not supported yet" — vagueness is **V23**. |
| `followUp` | string | A tracked issue reference. Required (FR-055). |
| `behaviors` | string[] | Behaviors still covered by the carrier, so coverage accounting stays truthful. |

**Predicted population** (research D4): concentrated in `parity_state_diff` (8 units) and `parity_observable_state` (7), plus the 33 `external-corpus-entry` units per D8.

---

## 4. CoverageReport — `target/conformance/migration-report.{json,md}` (derived, git-ignored)

The FR-039 before-and-after accounting. Deterministic: no timestamps, no absolute paths (FR-043).

```jsonc
{
  "schemaVersion": 1,
  "baselineRevision": "<sha>",
  "totals": {
    "before": { "units": 111, "behaviors": 25, "channels": 11, "fixtures": 37, "exceptions": 16 },
    "after":  { "cases": 0, "variants": 0, "behaviors": 0, "channels": 0, "fixtures": 0, "exceptions": 0 }
  },
  "accounting": { "migrated": 0, "deduplicated": 0, "residual": 0, "retired": 0, "unaccounted": [] },
  "errorPaths": { "before": 0, "preserved": 0, "weakened": [] },
  "residualQueue": [ { "id": "res-…", "blockedCarrier": "…", "missingCapability": "…", "followUp": "…" } ],
  "strictnessImprovements": [],
  "deletedCarriers": [],
  "deletableCarriers": [],
  "deletionBlockers": [],
  "violations": []
}
```

**Failure conditions** (each names the specific item and its category):

| Condition | Requirement |
|---|---|
| `accounting.unaccounted` non-empty | FR-040/FR-041 |
| A before-behavior, channel, fixture, or exception with no counterpart | FR-041 |
| `errorPaths.weakened` non-empty — a rejection whose direction or diagnostic expectation was lost | FR-042 |
| `totals.after.behaviors > totals.before.behaviors` | SC-005 (denominator inflation) |
| `migrated + deduplicated + residual + retired ≠ totals.before.units` — every unit needs exactly one disposition; raw case count is NOT the measure, since a residual conserves coverage without producing a case | SC-005 |
| Regeneration is not byte-identical | FR-043 / SC-012 |

---

## 5. EquivalenceLedger — `target/parity/equivalence.json` (derived, parity lane only)

Per-unit outcome comparison between the superseded carrier and its replacement. Gates deletion (FR-033–FR-038).

| Field | Type | Meaning |
|---|---|---|
| `unit` | string | Baseline unit id. |
| `legacyOutcome` | enum | Outcome under the superseded path. |
| `replacementOutcome` | enum | Outcome under the authoritative runner. |
| `relation` | enum | `equivalent` \| `stricter` \| `more-permissive` |
| `detail` | string | Required for `stricter` and `more-permissive`. |

**Relation rule** (spec A-002 — outcome, not message text): identical outcomes → `equivalent`; a difference detected only by the replacement → `stricter` (permitted, reported, and the new difference must be characterized per FR-036); a difference detected only by the legacy path → `more-permissive` (**blocks deletion**, FR-035).

**Deletion predicate**: a carrier is deletable iff every unit it carries has `relation ∈ {equivalent, stricter}` **and** no `ResidualRecord` names it as `blockedCarrier`.

---

## 6. NormalizationRule (registered)

Every rule becomes an enumerable record so FR-021 is checkable rather than aspirational.

| Field | Rules |
|---|---|
| `name` | e.g. `path_token`, `label_semantic`. |
| `scope` | `channel:<chan-id>` or `field:<json-pointer>`. A rule with no scope, or scoped to "all", is **V24**. |
| `action` | `rewrite` \| `canonicalize` \| `segment` \| `drop`. A `drop` requires `justification` naming the specific field and why it is not observable behavior. |
| `removes` | Required for `drop`: the **finite, enumerated** list of field names removed. An open-ended removal set — prefix match, pattern, or type predicate ("every empty value") — is **V24** by construction (FR-021). |
| `justification` | Required for `drop`; recommended otherwise. |

**Migration effect** (research D3): `prune` (blanket drop of every null/empty plus `configFilePath`) and `replace_hex12` (any 12-char hex run) cannot be registered as-is — both have open-ended removal sets, so both are **V24** by construction. They are replaced by `null_preserving` semantics plus, where genuinely needed, narrow field-scoped `drop` rules with enumerated `removes` lists.

`drop_noise_env` registers cleanly: its removal set is the finite enumerated `NOISE_ENV_KEYS` list. **`strip_intentional_labels` does not** — it matches label *prefixes*, an open-ended set. It must be narrowed to the enumerated labels both CLIs actually stamp, or replaced by `label_semantic` plus scoped allowed-differences. Both are channel-scoped to the legacy `chan-container-state` and retire with their carrier.

---

## 7. New Validation Classes (`validate.rs`, all block a PR via `registry_valid`)

Continuing the existing V1–V20 sequence:

| Class | Meaning |
|---|---|
| **V21** | Orphan — a baseline unit with no mapping entry, or a mapped case id that no unit reaches. Both directions. |
| **V22** | Fixture correspondence broken — an unreferenced migrated fixture, or a `fixtureMapping` that is not one-to-one (silent merge/split/drop). |
| **V23** | Malformed residual — missing/vague `missingCapability`, missing `followUp`, unresolvable `blockedCarrier`, or a residual counted as migrated. |
| **V24** | Unscoped or unjustified normalization rule — no scope, an "all" scope, or a `drop` action without a field-specific justification. |
| **V25** | Baseline provenance — committed `baseline.json` ≠ regenerated, or its `revision` does not match the recorded freeze commit. Removed with the drift gate at feature completion (FR-053). |

**Certification impact** (FR-054): `certify` reports the residual queue as **non-blocking** information and continues to block only on gaps, uncovered in-profile behaviors, and the inventory/clause classes.

---

## 8. Entity Relationships

```text
BaselineUnit ──1:1── MigrationMapping ──┬─→ Case[]        (migrated | deduplicated)
      │                                 └─→ ResidualRecord (residual)
      │                                          │
      │                                          └─→ blockedCarrier ─→ Program
      ├── fixtures ─→ FixtureMapping (1:1) ─→ conformance/fixtures/<id>/
      ├── channels ─→ chan-*
      └── diffClasses ─→ result vocabularies

Case ──→ Behavior[]  (denominator: must not grow)
     └─→ Variant      (same behavior, differing context/oracleType/channel)

EquivalenceLedger ──per-unit──→ deletion predicate ──→ Program deletion
CoverageReport ────consumes───→ BaselineUnit + MigrationMapping + registry
```

---

## 9. State Transitions

A baseline unit moves monotonically; it never returns to an earlier state:

```text
enumerated ──→ mapped ──→ expressed ──→ equivalence-proven ──→ carrier-deleted
     │            │                            │
     │            └──→ residual ───────────────┘  (blocks carrier deletion until resolved)
     └──→ retired  (requires rationale; reported explicitly, never implied by a total)
```

**Invariant** (research D5): once migration starts, a legacy carrier's mapped-unit count may only **decrease**. A hermetic test enforces this, bounding the transitional dual-path window that Constitution VIII would otherwise forbid.
