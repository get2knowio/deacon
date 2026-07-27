# Contract: the four coverage reports

Written by `coverage report` into `target/conformance/` (git-ignored). Each family emits a
`.json` (the machine contract) and a `.md` (rendered from the same ordered data, never
independently assembled).

**Universal properties** — every report, no exceptions:

| Property | Rule |
|---|---|
| Byte-stable | Identical registry content → identical bytes, on any machine (FR-062, SC-010) |
| No ambient inputs | No timestamps, no absolute paths, no hostname, no run-dependent ordering |
| Read-only | Generation never records, refreshes, or repairs evidence (FR-063) |
| Non-gating | Exit code reflects whether the report could be *written*, never what it says |
| Ordered | Keys sorted or `IndexMap` declaration order; never `HashMap` iteration order |

`schemaVersion` is present on every document and bumped on any breaking shape change.

---

## 1. `coverage-pairwise.json`

```jsonc
{
  "schemaVersion": 1,
  "operations": [
    {
      "operation": "up",
      "applicableDimensions": ["sdim-config-source", "sdim-container-state", "sdim-features",
                               "sdim-layering", "sdim-output-mode"],
      "pairs": [
        { "obligation": "obl-cmb-3f9a2c17",
          "assignment": { "sdim-config-source": "compose", "sdim-features": "lockfile" },
          "bucket": "covered", "by": ["case-up-compose-lockfile"] },
        { "obligation": "obl-cmb-91b0d4ee",
          "assignment": { "sdim-config-source": "compose", "sdim-container-state": "running-stale-config" },
          "bucket": "gap", "by": ["gap-up-compose-stale-config"] }
      ],
      "excluded": [
        { "assignment": { "sdim-container-state": "running" }, "rule": "rule-no-container-state-without-container" }
      ]
    }
  ],
  "summary": { "valid": 0, "covered": 0, "waived": 0, "nonTestable": 0, "gap": 0,
               "inactiveEnvironment": 0, "undispositioned": 0 },
  "deadValues": []
}
```

| Field | Contract |
|---|---|
| `bucket` | one of the five (FR-026) — **never folded together** |
| `by` | the covering case ids, or the backing waiver/gap/rationale id |
| `excluded` | invalid combinations **with the rule that excluded them** (FR-012) |
| `summary.undispositioned` | the number SC-001 requires to be zero |
| `deadValues` | values in no valid combination (V26, FR-010) |

`excluded` exists so that "absent because impossible" is visibly different from "absent
because nobody wrote it". Collapsing the two would make the denominator unfalsifiable.

---

## 2. `coverage-triples.json`

```jsonc
{
  "schemaVersion": 1,
  "triples": [
    { "id": "hrt-compose-features-restart", "obligation": "obl-cmb-77c1a208",
      "assignment": { "sdim-operation": "up", "sdim-config-source": "compose",
                      "sdim-features": "multiple-dependency-order", "sdim-container-state": "running" },
      "reason": "Feature install order interacts with Compose service startup and with re-entry …",
      "bucket": "covered", "by": ["case-up-compose-features-restart"] }
  ],
  "summary": { "selected": 0, "covered": 0, "gap": 0 }
}
```

Only `covered` and `gap` can appear: FR-015 forbids satisfying a triple by rationale or
waiver, and V29 rejects it at load. `reason` is carried into the report so a reviewer can
judge the *selection*, not just the coverage — an unreviewable triple set would make SC-003 a
formality.

---

## 3. `coverage-operations.json`

```jsonc
{
  "schemaVersion": 1,
  "operations": [
    { "operation": "up",
      "cases": 13,
      "inputClasses": { "valid": 6, "boundary": 2, "malformed": 1, "unsupported": 1, "reference-lenient": 3 },
      "configSources": { "image": 8, "dockerfile": 3, "compose": 2 },
      "channels": ["chan-exit-code", "chan-container-state", "chan-structured-output"],
      "missingInputClasses": [],
      "missingConfigSources": [] }
  ]
}
```

`missingConfigSources` is the SC-004 measure: every operation carries ≥1 executable case per
configuration source the applicability rules permit for it. `missingInputClasses` is the FR-040
measure. Both list what is **absent** — a report that only counted what exists would say
nothing about the hole.

---

## 4. `coverage-observables.json`

```jsonc
{
  "schemaVersion": 1,
  "channels": [
    { "channel": "chan-image", "cases": 4,
      "fields": ["config.entrypoint", "config.env.PATH", "config.labels.devcontainer.metadata"],
      "denormalizedFieldsCovered": ["entrypoints", "path", "metadata-label-namespaces"] },
    { "channel": "chan-file-content", "cases": 3, "fields": ["lockfile.features"],
      "denormalizedFieldsCovered": [] }
  ],
  "denormalizedFields": [
    { "field": "lifecycle-array-vs-object", "covered": true, "by": ["case-up-lifecycle-array", "case-up-lifecycle-object"] },
    { "field": "null-empty-omitted", "covered": true, "by": ["case-readconfig-null-empty-omitted"] }
  ],
  "unscopedNormalizationRules": [],
  "summary": { "channelsBelowFloor": 0 }
}
```

| Field | Contract |
|---|---|
| `channelsBelowFloor` | channels compared by fewer than **three** cases — the SC-005 measure |
| `denormalizedFields` | the twelve US5 fields, each with its covering cases — the SC-008 measure |
| `unscopedNormalizationRules` | MUST be empty; a non-empty list is V24 and blocks (FR-056) |

`fields` lists what is actually *compared*, not what is captured. The distinction is the point
of the report: 023 found two real defects the moment a captured-but-uncompared field started
being compared, and a report that counted captures would have shown those channels as healthy
the whole time.

---

## Markdown rendering

Each `.md` is rendered from the same in-memory model as its `.json`, in the same order — never
assembled separately. A discrepancy between the two would mean the human-readable artifact and
the machine-readable one disagree about coverage, and the human one is what gets reviewed.
