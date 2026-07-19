# Observable-state waivers

State-field waiver records for the observable-state parity binaries
(`parity_observable_state`, `parity_state_diff`). One JSON record per file,
validated by the single loader `parity_harness::waiver` (018-harden-parity-
harness, research D6). This directory replaces the retired
`KNOWN_INTENTIONAL_DIVERGENCES` / `KNOWN_GAPS` Rust consts (both empty at
migration — hence this directory currently holds no records).

Each record characterizes ONE observable-state field difference that is
intentional and mirrors the reference behavior. Schema (see
`specs/018-harden-parity-harness/contracts/registry-waiver-schema.md` and
`data-model.md` §4):

```json
{
  "id": "state/<slug>",
  "scope": {
    "kind": "state_field",
    "binary": "parity_state_diff",
    "fixture": "<case id used by the test>",
    "field": "label:com.docker.compose.project"
  },
  "expect": { "kind": "field-divergence", "ours": "<json>", "reference": "<json>" },
  "rationale": "why this divergence is intentional (constitution IV link, PR#, …)",
  "added": "YYYY-MM-DD"
}
```

Rules (enforced at load time):

- `id` is globally unique across ALL waiver records (these plus
  `errors/*/expect.json`); unknown fields are rejected; `rationale` is non-empty.
- `field` supports an exact match or a trailing-`*` prefix
  (`parity_harness::waiver::field_matches`).
- **Staleness** (FR-011): every loaded record must match an existing fixture
  field AND its characterized divergence must actually be observed each run;
  a record that matches nothing fails the run naming its `id`. Do not add a
  record until the divergence exists, and remove it when the divergence is
  fixed.
