# Data Model: Repository-Owned Conformance Registry

**Feature**: `019-conformance-registry` | **Date**: 2026-07-19

## File layout (authoritative data)

```text
conformance/
├── RULES.md                        # Human-documented contradiction rules (FR-014) + scope notes
└── registry/
    ├── revisions.json              # rev-*   pinned source revisions
    ├── dimensions.json             # dim-*   context dimensions + enumerated values
    ├── channels.json               # chan-*  observable channels (closed set)
    ├── profiles.json               # prof-*  certification profiles
    ├── sources/
    │   ├── schema.json             # src-schema-*   schema-constraint inventory
    │   ├── spec.json               # src-spec-*     normative-clause inventory
    │   ├── cli.json                # src-cli-*      shared CLI surface inventory
    │   └── observed.json           # src-obs-*      empirically observed reference behavior
    ├── behaviors/
    │   └── <area>.json             # bhv-*   normalized behavior units, grouped by area
    ├── cases.json                  # case-*  executable-test references
    ├── gaps.json                   # gap-*   known gaps
    ├── extensions.json             # ext-*   intentional Deacon extensions
    └── waivers/
        └── <wvr-id>.json           # wvr-*   one file per waiver (migrated parity waivers)
```

Test fixtures (valid + one-per-violation-class invalid registries) live under
`fixtures/conformance/` and are NOT part of the authoritative registry.

## Identity rules (all record types)

- ID format: `^(rev|src|dim|chan|prof|bhv|case|gap|wvr|ext)-[a-z0-9]+(-[a-z0-9]+)*$`
- The prefix MUST agree with the record type (FR-004). IDs are human-assigned, unique
  across the whole registry (not just within a type), and immutable once merged.
- Every cross-reference field holds a stable ID; dangling references are validation
  failures (V1). All collections serialize in ID-sorted order for determinism.

## Entities

### SourceRevision (`rev-`) — `revisions.json`

| Field | Type | Rules |
|-------|------|-------|
| `id` | string | e.g. `rev-spec-113500f4`, `rev-oracle-0-87-0` |
| `kind` | enum | `spec` \| `schema` \| `oracle` \| `cli-surface` |
| `pin` | string | commit SHA, semver, or equivalent immutable identifier |
| `url` | string | upstream location (informational) |
| `verifiedAgainst` | string? | repo-local machine-readable pin this must match (e.g. `fixtures/parity-corpus/oracle.json`); staleness check V7 |

### SourceUnit (`src-`) — `sources/*.json`

| Field | Type | Rules |
|-------|------|-------|
| `id` | string | prefix by inventory: `src-schema-`, `src-spec-`, `src-cli-`, `src-obs-` |
| `inventory` | enum | `schema` \| `spec` \| `cli` \| `observed` — MUST match the file it lives in |
| `revision` | ref → SourceRevision | provenance anchor |
| `locator` | string | where within the source (JSON pointer, section heading, flag name, corpus case) |
| `summary` | string | one-sentence statement of the requirement/observation |
| `behaviors` | ref[] → BehaviorUnit | many-to-many; may be empty ONLY if `outOfScope` set |
| `outOfScope` | object? | `{ "reason": string }` — explicit classification; absence + empty `behaviors` = violation V4 |

### ContextDimension / value (`dim-`) — `dimensions.json`

| Field | Type | Rules |
|-------|------|-------|
| `id` | string | `dim-os`, `dim-arch`, `dim-runtime`, `dim-oracle` |
| `values` | string[] | closed enumerated set (`linux`, `amd64`, `docker`, `podman`, `0.87.0`, …) |

Applicability conditions and profiles may only reference declared dimension/value pairs
(violation V1 otherwise).

### ObservableChannel (`chan-`) — `channels.json`

| Field | Type | Rules |
|-------|------|-------|
| `id` | string | seed set: `chan-stdout`, `chan-stderr`, `chan-exit-code`, `chan-container-state`, `chan-filesystem`, `chan-file-content` |
| `description` | string | what the channel observes |

Closed set: outcomes referencing an undeclared channel are violation V9.

### CertificationProfile (`prof-`) — `profiles.json`

| Field | Type | Rules |
|-------|------|-------|
| `id` | string | initial: `prof-linux-amd64-docker-0870` |
| `context` | map dim → value | MUST assign every declared dimension exactly one declared value |
| `active` | bool | exactly one profile is active for validation/coverage in this feature |

Profiles are independent: no record may claim more than its declared context (FR-016).

### BehaviorUnit (`bhv-`) — `behaviors/*.json`

| Field | Type | Rules |
|-------|------|-------|
| `id` | string | e.g. `bhv-readconfig-malformed-jsonc-rejected` |
| `area` | string | grouping key, matches file name |
| `statement` | string | normalized, externally observable behavior statement |
| `applicability` | condition[] | each `{ "dimension": dim-ref, "values": [string] }`; empty array = applicable everywhere |
| `spec` | enum | `conformant` \| `nonconformant` \| `unspecified` \| `not-applicable` (FR-009) |
| `reference` | enum | `aligned` \| `divergent` \| `unknown` \| `not-applicable` (FR-010) — a claim about the active profile's oracle only (FR-013). Multi-profile evolution (FR-016) is additive: a future `schemaVersion` bump adds an optional per-profile override map while this field keeps the initial profile's value — existing records are never restructured |
| `decision` | enum | `follow-spec` \| `align-with-reference` \| `deacon-extension` \| `intentional-divergence` \| `unresolved-gap` (FR-011) |
| `sources` | (derived) | inverse of SourceUnit.behaviors; every behavior MUST be referenced by ≥1 source unit (V1) |
| `notes` | string? | rationale, issue links |

All three disposition fields are mandatory (FR-012); missing any → schema violation.

### TestCase (`case-`) — `cases.json`

| Field | Type | Rules |
|-------|------|-------|
| `id` | string | e.g. `case-parity-corpus-errors-malformed-json` |
| `behaviors` | ref[] → BehaviorUnit | ≥1 required; empty = orphan, violation V3 |
| `context` | condition[] | declared context; must intersect every linked behavior's applicability (V10) |
| `executable` | object | `{ "binary": string, "test": string?, "corpus": string?, "case": string? }` — binary must exist as a test file under `crates/*/tests/` (V1, per research Decision 9) |
| `outcomes` | outcome[] | ≥1 required |

**ExpectedOutcome** (inline in TestCase): `{ "channel": chan-ref, "expectation": string }`
— channel must be declared (V9).

### Gap (`gap-`) — `gaps.json`

| Field | Type | Rules |
|-------|------|-------|
| `id` | string | |
| `kind` | enum | `coverage` (no case yet) \| `knowledge` (reference behavior unknown) \| `implementation` (deacon lacks the behavior) |
| `behaviors` | ref[] → BehaviorUnit | ≥1 |
| `description` | string | required |
| `tracking` | string? | issue link |

Gaps satisfy structural coverage (V5) but ALWAYS fail strict certification (FR-020,
FR-025). No expiry — gaps persist until resolved by editing the registry.

### Waiver (`wvr-`) — `waivers/*.json`

| Field | Type | Rules |
|-------|------|-------|
| `id` | string | file name = `<id>.json` |
| `behaviors` | ref[] → BehaviorUnit | ≥1 |
| `scope` | object | harness-consumable scope, preserved from the parity schema: `{ "kind": "corpus_case" \| "state_field", … }` |
| `expect` | object | preserved parity expectation (`both-accept` \| `both-reject` \| `deacon-stricter` \| `reference-stricter`, optional `signal`) |
| `rationale` | string | required, non-empty |
| `added` | date | ISO `YYYY-MM-DD` |
| `expires` | date | ISO `YYYY-MM-DD`; `expires < today` → violation V6 (valid through the stated date; boundary `expires == today` passes) |

Waived coverage is reported as `waived`, never `conformant` (FR-023). The parity
harness's stale-waiver mechanic (waiver whose difference stops reproducing fails the run)
is preserved via `scope`/`expect`.

### DeaconExtension (`ext-`) — `extensions.json`

| Field | Type | Rules |
|-------|------|-------|
| `id` | string | e.g. `ext-workspace-trust-gate` |
| `behaviors` | ref[] → BehaviorUnit | ≥1; each linked behavior MUST have `decision: deacon-extension` (consistency check, part of V8) |
| `description` | string | required |
| `docs` | string? | pointer (e.g. `SECURITY.md`, `docs/DIFFERENTIATORS.md`) |

## Validation rules (violation classes)

Each class has a stable code, used by tests (SC-002) and error output. Validation reports
ALL violations in a run (FR-019).

| Code | FR | Failure condition |
|------|----|-------------------|
| V1 | 018a | any reference to a non-existent record ID (includes dimension values, executable test binaries) |
| V2 | 018b | duplicate stable ID anywhere in the registry; or ID prefix/type mismatch or format violation (FR-004) |
| V3 | 018c | test case linked to no behavior |
| V4 | 018d | source unit with empty `behaviors` and no `outOfScope` classification |
| V5 | 018e | behavior applicable in the active profile with no case, no waiver, and no gap |
| V6 | 018f | waiver with `expires` earlier than today |
| V7 | 018g | SourceRevision whose `pin` disagrees with its `verifiedAgainst` repo file |
| V8 | 018h | disposition contradiction — rules R1–R8 (research Decision 5); includes extension/decision consistency |
| V9 | 018i | expected outcome referencing an undeclared observable channel |
| V10 | 018j | test case whose context has an empty intersection with a linked behavior's applicability |

Schema-level failures (missing mandatory field, bad enum value, malformed JSON) are
reported as a distinct `SCHEMA` class with file + location, consistent with constitution
IV (fail fast, precise messages).

## Derived evaluations (not stored)

- **Coverage state per in-profile behavior** (FR-023): `conformant` (spec `conformant` ∧
  reference `aligned` ∧ ≥1 case), `divergent` (reference `divergent` with decision
  `intentional-divergence`/`follow-spec` and case-backed), `waived` (coverage via
  waiver), `uncovered` (nothing — structural violation V5, so never appears in a valid
  registry's report; gap-covered behaviors report as `gap`). Extensions report in their
  own bucket. Out-of-profile behaviors are listed separately and excluded from
  denominators (FR-017).
- **Strict certification** (FR-025): fails iff any gap exists OR any in-profile behavior
  is uncovered. Waivers do not fail certification; they are enumerated in its output.
- **Denominator** (FR-003): count of distinct in-profile BehaviorUnits. Never source
  units.

## State transitions

- Gap resolution: `unresolved-gap` behavior gains a case → statuses become
  evidence-backed → decision re-recorded (R8/R4 no longer force `unresolved-gap`) → gap
  record deleted in the same change (else R7/R1 flag the contradiction).
- Waiver expiry: passing `expires` makes validation fail (V6) → maintainer either
  re-reviews and extends the date (with rationale update) or removes the waiver and fixes
  the divergence. There is no auto-renewal.
- Pin bump: editing a `rev-` pin without reclassifying dependent source units leaves the
  units' provenance pointing at the new revision — the bump PR must re-verify affected
  units (spec Edge Case "Source pin advances"); `verifiedAgainst` files must be updated in
  the same change or V7 fails.
