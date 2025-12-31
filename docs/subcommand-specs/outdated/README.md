# Outdated Subcommand

- Executive Summary: Reports current, wanted, and latest versions of Features declared in a devcontainer configuration. Uses the lockfile (if present) and OCI registry tags to determine whether upgrades are available. Outputs either a human-readable table or a structured JSON document.

- Common Use Cases:
  - Audit features for upgrades: `devcontainer outdated --workspace-folder .`
  - Produce machine-readable report: `devcontainer outdated --workspace-folder . --output-format json`
  - Check a specific config file: `devcontainer outdated --workspace-folder . --config .devcontainer/devcontainer.json`

- Documents:
  - SPEC: docs/subcommand-specs/outdated/SPEC.md
  - DIAGRAMS: docs/subcommand-specs/outdated/DIAGRAMS.md
  - DATA STRUCTURES: docs/subcommand-specs/outdated/DATA-STRUCTURES.md

- Implementation Checklist:
  - Parse CLI; require `--workspace-folder`; accept `--config`, `--output-format {text|json}`, log flags, and optional `--terminal-{rows,columns}`.
  - Resolve effective `devcontainer.json`; load lockfile adjacent to the config when present.
  - Derive version info per feature: compute `wanted` (tag/digest semantics), `current` (lockfile or wanted), `latest` (highest semver tag), majors, and omit unversionable features.
  - Render text table (Feature, Current, Wanted, Latest) in a deterministic order; or emit JSON object mapping feature IDs to version info.
  - Handle registry/network errors gracefully by returning items with undefined fields rather than failing the command.

