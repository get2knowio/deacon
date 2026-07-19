# Contract: coverage report formats

## `report.json` (machine-readable)

Single JSON document. All arrays sorted by stable ID; maps are emitted in insertion
order from ID-sorted iteration. No timestamps, hostnames, or absolute paths (SC-004).

```jsonc
{
  "schemaVersion": 1,
  "profile": {
    "id": "prof-linux-amd64-docker-0870",
    "context": { "dim-os": "linux", "dim-arch": "amd64", "dim-runtime": "docker", "dim-oracle": "0.87.0" }
  },
  "revisions": [ { "id": "rev-…", "kind": "spec", "pin": "113500f4" } ],
  "summary": {
    "behaviorsInProfile": 0,      // the ONLY denominator (deduplicated behaviors, FR-003)
    "conformant": 0,
    "divergent": 0,
    "waived": 0,
    "gap": 0,
    "extensions": 0,
    "outOfProfile": 0             // excluded from denominator (FR-017)
  },
  "behaviors": [
    {
      "id": "bhv-…",
      "statement": "…",
      "coverage": "conformant" | "divergent" | "waived" | "gap",
      "spec": "conformant", "reference": "divergent", "decision": "intentional-divergence",
      "sources": [ "src-…" ],                       // trace: source → behavior
      "applicability": [ { "dimension": "dim-…", "values": ["…"] } ],  // → context
      "cases": [                                     // → case → outcome
        { "id": "case-…", "outcomes": [ { "channel": "chan-…", "expectation": "…" } ] }
      ],
      "waivers": [ "wvr-…" ], "gaps": [ "gap-…" ]
    }
  ],
  "outOfProfile": [ { "id": "bhv-…", "applicability": [ … ] } ],
  "extensions": [ { "id": "ext-…", "behaviors": [ "bhv-…" ] } ],
  "gaps": [ { "id": "gap-…", "kind": "coverage", "behaviors": [ "bhv-…" ] } ],
  "waivers": [ { "id": "wvr-…", "rationale": "…", "expires": "2027-01-19" } ],
  "unclassifiedSourceUnits": []   // always empty in a valid registry; present for shape stability
}
```

The full source → behavior → context → case → outcome chain (FR-022) is navigable via
the `sources`, `applicability`, `cases`, and `outcomes` fields on each behavior entry.

## `report.md` (human-readable)

Required sections, in order:

1. **Header** — profile identity and pinned source revisions (no generation timestamp).
2. **Summary table** — the `summary` counts; waived shown as its own column, never folded
   into conformant (FR-023).
3. **Gaps** — every gap with kind, description, linked behaviors. Always present as a
   section, "None" when empty; gaps are never hidden (FR-020).
4. **Divergences & waivers** — each divergent/waived behavior with its three-axis
   disposition, rationale, and expiry.
5. **Extensions** — Deacon extensions listed separately from divergences.
6. **Behavior traceability index** — per behavior: sources, contexts, cases, expected
   outcomes per channel (FR-022, SC-003).
7. **Out-of-profile behaviors** — listed with the profiles/conditions they await.
