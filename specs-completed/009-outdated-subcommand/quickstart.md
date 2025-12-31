# Quickstart — Outdated Subcommand

- Human report:
  - `deacon outdated --workspace-folder .`
  - Output columns: `Feature | Current | Wanted | Latest`

- JSON report:
  - `deacon outdated --workspace-folder . --output json`
  - Keys: canonical fully‑qualified feature ID without version
  - Unknowns: null values with keys present

- CI gating:
  - `deacon outdated --workspace-folder . --output json --fail-on-outdated`
  - Exit code 2 when any outdated is detected; JSON still emitted
