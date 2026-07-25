# Contract: Baseline Inventory

**File**: `conformance/migration/baseline.json` · **Owner**: machine (`baseline generate`) · **Hand edits**: rejected as V25

The baseline is the subject of the sentence "no coverage was lost". If it is wrong or editable, the conservation claim is unfalsifiable (FR-045).

## Envelope

```jsonc
{
  "schemaVersion": 1,
  "revision": "98c26a5",                 // freeze commit; participates in V25
  "generatedFrom": {
    "parityRegistry": "fixtures/parity-corpus/registry.json",
    "discovery": ["discover_tier1_cases", "discover_error_cases"]
  },
  "records": [ /* BaselineUnit, sorted by id */ ]
}
```

## Record

```jsonc
{
  "id": "parity_corpus_tier1::node-ts",
  "program": "parity_corpus_tier1",
  "category": "live-per-case",
  "dockerRequired": false,
  "assertion": "deacon and the pinned reference resolve the same configuration for the node-ts workspace",
  "channels": ["chan-exit-code", "chan-structured-output"],
  "errorPath": false,
  "fixtures": ["fixtures/parity-corpus/node-ts"],
  "diffClasses": ["ref-only", "value", "deacon-only", "oracle-failure", "normalization"]
}
```

## Enumeration rules (FR-049)

1. **Per-case programs** — one record per emitted `CaseResult`. Case ids come from the program's own case list or its discovery function, never from a re-implemented walk (research D1: this is how 24 was mistaken for 25).
2. **Guard programs** — programs that emit no `CaseResult` contribute one record per `#[test]`/`#[tokio::test]` function.
3. **External manifest** — one record per entry, `category: external-corpus-entry`, `program: realworld` (research D8).
4. **No grouping, no splitting** — an enumeration that merges two independently reported outcomes, or splits one, is a defect.

## Determinism (FR-003, SC-012)

Sorted by `id`; no timestamps; no absolute paths; no machine-specific values (hostname, user, tempdir). Regeneration on an unchanged tree is byte-identical.

## Freeze semantics

`revision` is set once at freeze. Afterwards `baseline check` compares the recomputed inventory against the committed file and fails naming each added, removed, or changed unit. `assertion` text is authored once at freeze and is immutable thereafter — it is the record of what the unit asserted *before* migration, and rewriting it post hoc would let the coverage proof be satisfied by lowering the bar.

## Expected content at freeze (research §1)

| Category | Records |
|---|---:|
| `live-per-case` | 91 |
| `hermetic-guard` | 16 |
| `internal-consistency` | 4 |
| `external-corpus-entry` | 33 |
| **Total** | **144** (111 executable units + 33 recorded-only entries) |

The 111 executable units are the denominator for SC-005; the 33 recorded-only entries are inventoried but never counted as migrated (D8).
