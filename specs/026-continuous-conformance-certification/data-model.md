# Phase 1 Data Model: Continuous Conformance Operation & Release Certification

**Feature**: `026-continuous-conformance-certification`
**Date**: 2026-07-28

All records are strict JSON (unknown fields rejected at load), written atomically (temp file + `fs::rename`),
and rendered in declaration order via `IndexMap` where order is meaningful. Ids are substance-anchored where
a rename or reorder must not change identity, following the `hash8` convention already used for `clu-` and
finding signatures.

Ownership is stated explicitly for every file, because the machine-owned/hand-authored boundary is what makes
"generation never touches classification" checkable:

| File | Owner | Written by |
|---|---|---|
| `conformance/lanes/lanes.json` | hand-authored | humans only |
| `conformance/drift/observations.json` | machine-owned | `drift-scan` only |
| `conformance/discovery/canary.json` | hand-authored | humans only |
| `target/conformance/execution-manifest.json` | machine-owned | the container lane only |
| `target/conformance/certification.{json,md}` | machine-owned | `certify --report-dir` only |
| `target/drift/upgrade-proposal.{json,md}` | machine-owned | `oracle-upgrade-propose` only |

---

## 1. Lane (`lane-*`)

`conformance/lanes/lanes.json` — hand-authored. Exactly five records (FR-001).

```jsonc
{
  "schemaVersion": 1,
  "records": [
    {
      "id": "lane-pr-hermetic",
      "displayName": "pull-request (hermetic)",
      "trigger": "pull-request",              // pull-request | nightly | weekly | invoked | release
      "blocking": true,                        // FR-009 / FR-015 / FR-019
      "preconditions": [],                     // container-engine | reference-oracle | network
      "nextestProfile": "default",             // null when the lane runs no test binaries
      "mayWriteRecord": false,                 // FR-016 / FR-020 — false for every lane
      "includes": {
        "validationClasses": ["V1", "…", "V36"],
        "casePredicate": {                     // derived membership, never an id list (D9)
          "oracleTypes": ["spec-expectation", "snapshot", "invariant-metamorphic"],
          "resourceGroups": ["none", "fs-heavy"]
        },
        "programs": ["conformance_replay", "lane_integrity", "certification_gates",
                     "drift_hermetic", "registry_valid", "discovery_hermetic"],
        "snapshotReplay": true
      },
      "excludes": {                            // FR-005 — stated, not implied by omission
        "rationale": "No container engine, reference oracle, or network is available in this lane.",
        "casePredicate": { "oracleTypes": ["live-differential"], "resourceGroups": ["docker-shared", "docker-exclusive"] }
      }
    }
  ]
}
```

**Field rules**

- `blocking` MUST be `false` for `lane-canary` and `lane-nightly-stable` (FR-015, FR-019).
- `mayWriteRecord` MUST be `false` for all five lanes. The field exists so the constraint is *stated* and
  testable rather than merely absent — a future lane that wants to write must change a value a reviewer sees.
- `preconditions` drives FR-004: a lane declaring `reference-oracle` MUST fail when the oracle is missing or
  mismatched; a lane **not** declaring it MUST fail if its execution path tries to resolve one.
- `casePredicate` is a predicate over existing case fields, never an id list (research D9, FR-002a). Two
  predicates on the same lane are ANDed; `includes` and `excludes` MUST partition the case space with no
  overlap and no remainder. This partition check is what makes derived selection safe: a predicate can
  silently *capture* a new case, which is intended, but the partition proof means it can never silently
  *drop* one — the failure mode FR-002's allow-list rule guards against for programs.

**Relationships**: references validation classes (from `Violation`), programs (from the test tree), and cases
(from `Registry`). It owns none of them.

---

## 2. Execution Unit (derived — no file)

The denominator FR-003 checks. Never authored; enumerated by `lane.rs` from four sources (research D2):

| Kind | Source | Id form |
|---|---|---|
| `validation-class` | **both** class enumerations — the registry `Violation` enum (V1–V36) and the discovery enum (D1–D6) | `unit-vcls-<class>` e.g. `unit-vcls-V26`, `unit-vcls-D6` |
| `case` | `Registry::cases` | `unit-case-<case-id>` |
| `program` | `#[test]` scan over `crates/deacon/tests/*.rs` | `unit-prog-<binary>` |
| `snapshot-replay` | `conformance/snapshots/<os-arch>/<case-id>/` | `unit-snap-<os-arch>-<case-id>` |

**Assignment**: each unit maps to ≥1 lane by evaluating every lane's `includes`. Zero assignments → **V34**.

**Why derived**: a hand-authored denominator would let an omitted unit satisfy full-assignment validation
while being covered by nothing, inverting the check into a rubber stamp (research D2).

---

## 3. Execution Manifest

`target/conformance/execution-manifest.json` — machine-owned, git-ignored, moved between CI jobs as an
artifact. Produced only by the container-backed lane; consumed only by `certify`.

```jsonc
{
  "schemaVersion": 1,
  "revision": "e6a9cc3…",                 // full 40-hex commit under test (FR-033c)
  "profile": "prof-linux-amd64-docker-0870",
  "environment": {
    "platform": "linux",
    "arch": "x86_64",
    "containerEngine": "docker",
    "containerEngineVersion": "27.3.1",
    "composeVersion": "2.29.7"
  },
  "requiredCaseCount": 39,
  "cases": [
    {
      "caseId": "case-up-decl-basic-image",
      "caseHash": "…",                     // must equal the current computed hash (FR-033d)
      "fixtureHash": "…",
      "outcome": "pass",                   // pass | fail | allowed-difference | excluded
      "excludedBy": null                   // a disposition id when outcome == "excluded"
    }
  ]
}
```

**Verification rules** (each is a distinct **V35** sub-case, so a failure names its cause — FR-042):

| Sub-case | Condition |
|---|---|
| `V35-absent` | no manifest at the expected path |
| `V35-incomplete` | a required case id is missing from `cases` |
| `V35-revision` | `revision` ≠ the revision under certification |
| `V35-stale` | a recorded `caseHash`/`fixtureHash` ≠ the currently computed one |
| `V35-unaccounted` | an `outcome` outside the enumeration, or `excluded` with an unresolvable `excludedBy` |

`V35-unaccounted` is FR-041(i)'s *silently skipped case*: a result that is neither pass, fail, nor an
explicitly dispositioned exclusion. Note that `outcome: "fail"` is **not** a manifest-integrity violation —
it is an ordinary certification blocker reported against the case, which keeps "the evidence is malformed"
distinct from "the evidence says deacon diverged".

---

## 4. Drift Observation (`drf-*`)

`conformance/drift/observations.json` — machine-owned, sole output of `drift-scan`.

```jsonc
{
  "schemaVersion": 1,
  "records": [
    {
      "id": "drf-spec-113500f4-a1b2c3d4",   // hash8 over kind ‖ pinnedRevision ‖ observedRevision
      "kind": "spec-commit",                 // spec-commit | schema-change | reference-release
                                             // | cli-surface-change | upstream-test-or-changelog
      "pinnedRevision": "113500f4",
      "observedRevision": "9f21ab77",
      "affectedSurfaces": ["conformance/spec/113500f4/features.md"],
      "observedAt": "2026-07-28",            // date only — no clock time, keeps the file byte-stable
      "reviewArtifact": "target/drift/scan.json"
    }
  ],
  "lastCompletedRun": { "date": "2026-07-28", "kindsProbed": ["spec-commit", "schema-change",
                        "reference-release", "cli-surface-change", "upstream-test-or-changelog"] }
}
```

**`lastCompletedRun` is what makes FR-025 checkable.** "No drift" is `records: []` **with** a
`lastCompletedRun` covering all five kinds; "did not run" is a missing or partial `lastCompletedRun`. Without
this field the two states are the same empty array, which is exactly the ambiguity FR-025 forbids.

**Observations are not pins.** This file records *what upstream looks like*, never *what deacon is pinned
to*. The pin stays in `conformance/registry/revisions.json` and remains human-only (FR-028). That separation
is what lets `drift-scan` write here without violating FR-024.

---

## 5. Upgrade Proposal

`target/drift/upgrade-proposal.{json,md}` — machine-owned, git-ignored, deterministic.

```jsonc
{
  "schemaVersion": 1,
  "fromOracle": "0.87.0",
  "toOracle": "0.88.0",
  "inputState": { "registryDigest": "…", "worktreeClean": true },   // FR-027 + dirty-tree edge case
  "sections": {
    "schemaDrift":            { "present": true, "entries": [ /* … */ ] },
    "specificationDrift":     { "present": true, "entries": [] },
    "cliSurfaceDrift":        { "present": true, "entries": [] },
    "referenceBehaviorDrift": { "present": true, "entries": [] },
    "snapshotDifferences":    { "present": true, "entries": [] },
    "newlyFailingCases":      { "present": true, "entries": [] },
    "affectedDispositions":   { "present": true, "entries": [] }
  }
}
```

**All seven keys MUST be present.** `"entries": []` means *investigated, nothing found*; a missing key means
*not investigated* and is **V36-incomplete** (FR-030). Conflating the two is the failure this shape prevents:
an unrun analysis must never read as a clean one.

**Determinism**: no timestamp, no absolute path, no hostname. `inputState.registryDigest` records what it was
computed from so a proposal built against a dirty tree is recognizable (spec edge case).

---

## 6. Canary Pin (`cnr-*`)

`conformance/discovery/canary.json` — hand-authored, in the **discovery** root (clarification Q5).

```jsonc
{
  "schemaVersion": 1,
  "records": [
    {
      "id": "cnr-cli-main-9f21ab77",
      "target": "reference-cli",              // reference-cli | spec
      "revision": "9f21ab7712c4…",            // 40-hex commit, or an exact published version
      "url": "https://github.com/devcontainers/cli/tree/9f21ab7712c4…",
      "added": "2026-07-28"
    }
  ]
}
```

**D6 sub-cases**: a `revision` that is not 40-hex and not an exact version (a branch, moving tag, or dist-tag
is mutable — FR-018); a duplicate id; a non-derived id; an unresolvable `target`.

**Isolation**: no registry loader may reference this file, asserted behaviorally (SC-016: identical verdict
with the file populated and absent) and by source scan, mirroring 025's
`no_discovery_source_references_a_registry_or_snapshot_writer`.

---

## 7. Certification Report

`target/conformance/certification.{json,md}` — machine-owned, byte-stable, derived from the `Certification`
verdict value (research D4).

```jsonc
{
  "schemaVersion": 1,
  "certified": false,
  "identity": {
    "deaconRevision": "e6a9cc3…", "oracleVersion": "0.87.0",
    "specRevision": "113500f4", "schemaRevisions": ["113500f4"]
  },
  "environment": {
    "platform": "linux", "arch": "x86_64",
    "containerEngine": "docker", "containerEngineVersion": "27.3.1",
    "composeVersion": "2.29.7"
  },
  "scope": {
    "profile": "prof-linux-amd64-docker-0870",
    "doesNotCertify": ["podman", "macos", "windows", "aarch64", "oracle != 0.87.0"],
    "statement": "Certification covers linux/x86_64 with docker against @devcontainers/cli 0.87.0 only."
  },
  "sourceScope": {
    "schemaDocuments": 4, "proseDocuments": 18, "cliSurface": "0.87.0",
    "unclassifiedUnits": []
  },
  "coverage": {
    "behaviorCount": 0, "contextCoverage": { /* obligation buckets */ },
    "observableCoverage": { /* per-channel covering-case counts */ }
  },
  "exceptions": { "gaps": [], "waivers": [], "intentionalDivergences": [] },
  "snapshotProvenance": [ { "caseId": "…", "platform": "linux-x86_64", "staleness": "fresh" } ],
  "notCertified": { "inactiveProfiles": [], "nonTestable": [], "noReferenceForPlatform": [] },
  "evaluationDate": "2026-07-28",
  "blocking": [ { "condition": "stale-snapshot", "record": "case-…", "detail": "caseHash drifted" } ]
}
```

**All sixteen required fields** (FR-034, SC-004) map as: `identity` ×4 (deacon revision, oracle version, spec
revision, schema revisions), `environment` ×4 (platform, arch, container engine *and its version* counted as
one, Compose version), `sourceScope` ×1, `coverage` ×3, `exceptions` ×3, `snapshotProvenance` ×1 — sixteen
exactly.

`scope.profile`, `scope.doesNotCertify`, `evaluationDate`, and `notCertified` are all **required** too, but
they are not among FR-034's sixteen: they satisfy FR-035, FR-040, and FR-037 respectively. A test asserting
field count must use sixteen for the FR-034 set and assert the other four separately — counting them together
yields twenty and fails.

**Byte-stability**: `evaluationDate` is injected via the existing global `--today`, never read from the
clock. `deaconRevision` comes from the build environment, not from a `git` call at report time.

---

## 8. Validation Classes

> **Class numbering note (implementation).** V31 and V32 were already taken by 025's
> metamorphic relation catalogue. This feature's classes are **V34** (lane integrity),
> **V35** (execution-manifest integrity), and **V36** (drift-record integrity); the
> discovery class is **D6**. Reusing a documented number would have made two different
> defects report under one code — the exact failure the per-class split exists to prevent.

Registry-root classes (block a PR via `registry_valid`; V35 additionally feeds `certify`):

| Class | Guards |
|---|---|
| **V34** lane integrity | a derived unit with zero lane assignments; a lane referencing an unknown validation class, program, or profile; `includes`/`excludes` case predicates that overlap or leave a remainder; a `blocking: true` on a lane FR-015/FR-019 requires to be non-blocking; `mayWriteRecord: true`; a lane's `nextestProfile` whose `default-filter` disagrees with its declared programs |
| **V35** execution-manifest integrity | the five sub-cases in §3 |
| **V36** drift-record integrity | an observation whose id is not derived from its substance, or naming an unknown `kind`; a `lastCompletedRun` missing a probed kind; an upgrade proposal missing any of the seven section keys; a proposal whose regeneration is not byte-identical |

Discovery-root class (blocks a PR via `discovery_hermetic`; can never block a release):

| Class | Guards |
|---|---|
| **D6** canary-pin integrity | the four sub-cases in §6 |

**Why V35 is a registry class but consumed by `certify`, while D6 is a discovery class**: V35 polices
evidence that *backs a certification claim*, so it must be able to block a release. D6 polices canary state,
which by FR-017a must never influence one. The class boundary follows the root boundary (research D7).

---

## 9. State transitions

Only two records have a lifecycle worth modelling.

**Drift Observation**: `observed` → (human review) → either a registry change landing a new pin, or nothing.
An observation for the same `kind` with a newer `observedRevision` supersedes its predecessor (spec edge
case), which the derived id makes mechanical: a new substance produces a new id, and the scan rewrites the
file wholesale rather than accumulating history.

**Upgrade Proposal**: `prepared` → `complete` (all seven sections present) → (human acceptance) → a registry
change advancing the pin plus re-recorded snapshots through the reviewed record path. There is no
`auto-applied` state, and no code path can construct one (FR-028, SC-006).
