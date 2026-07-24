# Contract: Declarative Case Schema

Extends the existing `conformance/registry/cases.json` record. Consumed by `deacon-conformance` (load + validate) and `parity-harness` (execute). Legacy binary-backed records remain valid; a record is legacy **or** declarative, never both.

## Declarative case (canonical example)

```json
{
  "id": "case-up-postcreate-env",
  "behaviors": ["bhv-lifecycle-postcreate-env"],
  "context": ["single-container"],
  "oracleType": "live-differential",
  "resourceGroup": "docker-shared",
  "operations": [
    {
      "id": "op-up",
      "subcommand": "up",
      "argv": ["--workspace-folder", "${WORKSPACE}"],
      "fixtures": ["fx-devcontainer-postcreate"]
    },
    {
      "id": "op-exec-env",
      "subcommand": "exec",
      "argv": ["--workspace-folder", "${WORKSPACE}", "printenv", "MY_VAR"]
    }
  ],
  "expected": [
    { "channel": "chan-exit-code", "operation": "op-exec-env", "assertion": { "equals": 0 } },
    { "channel": "chan-injected-process", "operation": "op-exec-env",
      "assertion": { "env": { "MY_VAR": "hello" } } }
  ],
  "allowedDifferences": [
    { "behavior": "bhv-lifecycle-postcreate-env", "context": ["single-container"],
      "observablePath": "chan-injected-process.env.TZ",
      "rationale": "reference leaks host TZ; deacon does not — characterized",
      "waiverId": "wvr-postcreate-tz" }
  ],
  "fsAllowlist": [],
  "cleanup": { "containers": true, "images": "case-built", "networks": true, "volumes": true, "tempdir": true },
  "notes": "Exercises postCreateCommand env injection parity."
}
```

## Field rules (enforced at load — fail-loud, FR-003)

| Rule | Violation |
|------|-----------|
| Exactly one of `operations` (declarative) or `executable` (legacy) present | new V-series |
| `behaviors` non-empty, each resolves in registry | V3 / V1 |
| every `expected[].channel` declared in `channels.json` | V9 |
| `oracleType` ∈ 4 enum values | new V-series |
| `oracleType=invariant-metamorphic` ⇒ ≥2 ops + a `relationship` referencing a sibling op id | new V-series (arity) |
| `oracleType=spec-expectation` ⇒ every `expected[].assertion` present | new V-series |
| `fsAllowlist` non-empty **iff** an `expected` filesystem channel exists | new V-series |
| each `operations[].subcommand` ∈ consumer surface | new V-series (Principle II) |
| `allowedDifferences` well-formed & scoped (see allowed-difference rules below) | new V-series |

## Allowed-difference rules (FR-031..035)

- MUST carry `behavior`, `context`, `observablePath`, `rationale`, and exactly one of `waiverId`/`divergenceId`.
- `waiverId`/`divergenceId` MUST resolve to an existing registry `wvr-*` / `ext-*`/intentional-divergence record (dangling → V1-style).
- `observablePath` MUST be a dotted path *within* a channel; a bare channel with no path is rejected unless the `behavior` is inherently channel-wide (guard against global ignore lists, FR-032).
- Two `allowedDifferences` with the same `(behavior, observablePath)` and differing bodies → load error (FR-035).
- A difference matches **only** its `(behavior, observablePath, context)`; identical differences elsewhere still fail (FR-033).

## Hashing (FR-020 / research D3)

`caseHash = SHA256(canonical_json({ operations, oracleType, expected, fsAllowlist, fixtureHashes }))`.
`notes`, `allowedDifferences`, and human prose are **excluded** — editing them does not invalidate snapshots.

## Coexistence & migration

Legacy `{ executable, outcomes }` records keep running under their Rust binaries. Migrating one = replace `executable`/`outcomes` with `operations`/`expected`/`oracleType`, delete the bespoke `parity_*` test if no longer referenced, and (if it was a live binary) update `fixtures/parity-corpus/registry.json` + nextest overrides accordingly.
