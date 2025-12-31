# Research — Features Package (Single + Collection)

This document consolidates decisions and rationale to close gaps for the `features package` subcommand.

## Decisions

1) Mode Detection (Single vs Collection)
- Decision: Detect based on target path.
  - Single: `target/devcontainer-feature.json` exists at root
  - Collection: `target/src/` exists; each `src/<featureId>` is a candidate
- Rationale: Mirrors containers.dev conventions and keeps author UX consistent.
- Alternatives considered: Explicit `--mode` flag (rejected as redundant and error-prone).

2) Archive Format and Naming
- Decision: Use tar+gzip with `.tgz` extension for all feature archives.
- Rationale: Aligns with ecosystem tooling; fast, widely supported; deterministic when inputs are stable.
- Alternatives considered: `.tar.gz` (equivalent, but `.tgz` preferred for brevity); `.zip` (less common in features distribution).

3) Output Folder Semantics
- Decision: Default `--output-folder` to `./output`; create if missing. Overwrite existing files by default.
- Rationale: Simple default; predictable CI behavior.
- Alternatives considered: Fail on existing files (rejected; increases friction); temporary staging folder (added complexity without clear benefit).

4) Force Clean Output
- Decision: `-f, --force-clean-output-folder` empties the output folder before packaging.
- Rationale: Deterministic outputs in CI; prevents residue from past runs.
- Alternatives considered: Per-file unique names (rejected; hurts reproducibility and downstream expectations).

5) Metadata: `devcontainer-collection.json`
- Decision: Always emit collection metadata enumerating packaged features. Include `sourceInformation.source = "devcontainer-cli"` and fields: id, version, name, description, options, installsAfter, dependsOn.
- Rationale: Enables downstream discovery and validation; consistent with Dev Container CLI.
- Alternatives considered: Only emit for collections (rejected; single feature benefits from consistent metadata too).

6) Output Mode (Text vs JSON)
- Decision: Text-only for this subcommand. No structured JSON output.
- Rationale: Simpler contract; aligns with upstream guidance and spec clarifications.
- Alternatives considered: Support global JSON logs to stdout (rejected for this subcommand; logs remain on stderr).

7) Error Behavior
- Decision: Fail entire run on mixed valid/invalid subfolders in collection mode; list invalid ones; produce no artifacts; exit non-zero.
- Rationale: Prevents partial, ambiguous results; explicit failure avoids silent divergences.
- Alternatives considered: Package valid subset and warn (rejected by governance principle "No Silent Fallbacks").

8) Testing Strategy
- Decision: Add unit tests for detection/validation; integration tests for end-to-end packaging; cover invalid paths and `-f` behavior.
- Rationale: Keeps build green; maintains confidence during refactors.
- Alternatives considered: Integration-only tests (rejected; slower and less targeted).

## Open Questions
None — current spec and clarifications are sufficient.

## References
- Spec: `docs/subcommand-specs/features-package/SPEC.md`
- Clarifications (2025-11-01) embedded in feature spec (`specs/002-features-package-collection/spec.md`)
