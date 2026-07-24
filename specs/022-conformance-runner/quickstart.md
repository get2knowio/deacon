# Quickstart: Declarative Conformance Runner

Author a conformance case as **data** (no new Rust function), run it, and — for reference oracles — record a committed, provenance-stamped snapshot.

## 1. Add a declarative case

Append a record to `conformance/registry/cases.json` (`records[]`). Minimal `spec-expectation` case (no Docker, hermetic):

```json
{
  "id": "case-readconfig-echo",
  "behaviors": ["bhv-readconfig-unknown-field-preserved"],
  "context": [],
  "oracleType": "spec-expectation",
  "operations": [
    { "id": "op-read", "subcommand": "read-configuration",
      "argv": ["--workspace-folder", "${WORKSPACE}"],
      "fixtures": ["fx-config-with-unknown-field"] }
  ],
  "expected": [
    { "channel": "chan-exit-code", "assertion": { "equals": 0 } },
    { "channel": "chan-structured-output",
      "assertion": { "jsonSubset": { "configuration": { "customUnknownKey": "preserved" } } } }
  ],
  "cleanup": { "tempdir": true },
  "notes": "Unknown top-level keys round-trip (Constitution IV faithful-on-unmodeled)."
}
```

Add the fixture file it references, then register it (id + hash are derived by the loader).

## 2. Validate (hermetic — fails loud on mistakes)

```bash
cargo run -p deacon-conformance -- validate
```

Catches: unknown behavior, undeclared channel, legacy+declarative mix, bad oracle-type arity, unscoped allowed differences, dangling waiver ids. Fix any violation before proceeding (FR-003).

## 3. Run it

- **Hermetic (spec-expectation / snapshot oracle)** — runs in the fast lane:
  ```bash
  cargo nextest run -p deacon-conformance          # case-schema + staleness + scoping + normalization
  ```
- **Live differential / Docker channels** — the parity lane only (needs Docker + pinned oracle):
  ```bash
  cargo nextest run --profile parity -E 'binary(=parity_conformance_runner)'
  ```

## 4. Record a reference snapshot (only for `oracleType: snapshot`)

Recording is a **reviewed** action — it runs the live oracle and writes committed files:

```bash
cargo run -p parity-harness --bin conformance-snapshot -- refresh --case case-readconfig-echo
```

This writes `conformance/snapshots/<os>-<arch>/case-readconfig-echo/{provenance,raw,normalized}.json` atomically and prints a review diff. Commit them; the git diff is the review surface. Ordinary runs never write these (FR-021).

## 5. Confirm replay stays honest

```bash
cargo run -p deacon-conformance -- snapshot check --case case-readconfig-echo   # PASS
# edit an operation's argv, then:
cargo run -p deacon-conformance -- snapshot check --case case-readconfig-echo   # FAIL: stale (caseHash mismatch)
```

Editing only `notes` does **not** trip staleness (caseHash excludes prose — clarify Q4).

## 6. Add a Docker-backed case

Set `resourceGroup` and declare cleanup + fs allowlist:

```json
{
  "id": "case-up-mounts",
  "behaviors": ["bhv-workspace-mount-source"],
  "context": ["single-container"],
  "oracleType": "live-differential",
  "resourceGroup": "docker-shared",
  "operations": [ { "id": "op-up", "subcommand": "up",
                    "argv": ["--workspace-folder", "${WORKSPACE}"], "fixtures": ["fx-basic-devcontainer"] } ],
  "expected": [ { "channel": "chan-process-graph",
                  "assertion": { "mounts": [ { "target": "/workspaces/proj" } ] } } ],
  "fsAllowlist": [],
  "cleanup": { "containers": true, "images": "case-built", "networks": true, "volumes": true, "tempdir": true }
}
```

The runner creates an **isolated external temp workspace**, uses **collision-resistant** container/network/volume names, pins all images, and reclaims every resource on success **and** failure (RAII guard) — SC-009/SC-010.

## 7. Characterize an intentional divergence (instead of a global ignore)

Never add a global ignore. Scope it to a behavior + observable path + waiver:

```json
"allowedDifferences": [
  { "behavior": "bhv-workspace-mount-source", "context": ["single-container"],
    "observablePath": "chan-process-graph.mounts[0].consistency",
    "rationale": "reference sets 'consistent' on Linux; deacon omits (no-op on Linux) — characterized",
    "waiverId": "wvr-mount-consistency-linux" }
]
```

The tolerance applies **only** to that path; the same difference on another mount still fails (FR-033), and a self-invalidating `wvr-` fails as stale once the difference stops reproducing (FR-034).

## What did NOT require Rust

Steps 1, 4, 6, 7 are pure data edits. Adding cases/fixtures/assertions never adds a Rust test function (SC-001). Rust changes only when a *new observable channel* or *normalization rule* is introduced — a rare, reviewed extension of `observe/` + `normalize.rs`.
