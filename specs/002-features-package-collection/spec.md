# 002 — Features Package GAP Closure

Purpose: Close the implementation gaps for the `features package` subcommand so authors can package single features and full collections reliably, with proper metadata, consistent CLI, and test coverage as defined in the specification.

## Background and References
- Primary spec: `docs/subcommand-specs/features-package/SPEC.md`
- Gap analysis: `docs/subcommand-specs/features-package/GAP.md`
- Related commands: `features publish`, `features test`
- Dev Container reference: containers.dev implementors spec (Features Distribution)

## Clarifications

### Session 2025-11-01
- Q: Behavior on mixed valid/invalid subfolders in collection → A: Fail entire run; list invalid subfolders; produce no artifacts; exit non‑zero.
- Q: Archive naming convention → A: Use `.tgz` (tar+gzip) for all packaged feature archives.
- Q: JSON mode output (if any) → A: Text‑only output; no structured JSON for this subcommand.

## Problem Statement (What & Why)
Authors and CI systems cannot package feature collections end‑to‑end per spec. Missing capabilities (collection mode, collection metadata, clean output) block publishing flows and reduce interoperability with other tools.

## Goals (What)
- Support both single‑feature and collection packaging, auto‑detected from target path.
- Always emit `devcontainer-collection.json` describing packaged content.
- Provide a clean‑output option to ensure deterministic, reproducible artifacts.
- Align CLI with spec defaults for positional `target` and output folder.
- Provide clear, testable text outputs; this subcommand does not support JSON output.

## Non‑Goals
- Changing feature schema or OCI upload behavior (covered by `features publish`).
- Parallel/concurrent packaging optimization (may be future enhancement).

## Actors
- Feature authors packaging locally.
- CI engineers generating artifacts for publish.
- Tooling consuming collection metadata for discovery.

## Assumptions
- Local filesystem is the only source; no network calls during packaging.
- Spec’s detection rules are authoritative:
  - If `target/devcontainer-feature.json` exists → single feature mode
  - Else if `target/src` exists → collection mode (each `src/<featureId>` packaged)
- Global `--log-level` is acceptable in place of subcommand flag (documented behavior).

## User Scenarios & Testing
1. Single Feature Packaging
   - User runs `features package` in a feature folder.
   - System creates one `*.tgz` and writes `devcontainer-collection.json` reflecting that single feature.

2. Feature Collection Packaging
   - User runs `features package` at repo root where `src/` contains multiple features.
   - System packages each valid `src/<featureId>` into its own `*.tgz` and writes `devcontainer-collection.json` enumerating them.

3. Force Clean Output
   - Output directory already contains old artifacts.
   - User runs with `-f/--force-clean-output-folder`; system empties output before writing new artifacts.

4. Default Target Path
   - User omits the positional `target`.
   - System defaults to current working directory `.` and proceeds per detection rules.

5. Invalid Feature Metadata
   - Corrupt or missing `devcontainer-feature.json`.
   - System fails fast with clear error and non‑zero exit code.

6. Empty or Invalid Collection
   - `src/` exists but contains no valid feature subfolders.
   - System fails with a message indicating no valid features were found.

If any scenario is underspecified at implementation time, prefer explicit user‑facing error messages over silent fallbacks.

## Functional Requirements (Testable)
FR‑1 Mode Detection
- The command MUST detect packaging mode based on the target path.
- Single feature: `devcontainer-feature.json` present at target root.
- Collection: `src/` exists under target and contains ≥1 valid feature folder.

FR‑2 Artifact Creation
- For each packaged feature, the system MUST produce a `*.tgz` in the output folder.
- Existing files with the same name MUST be overwritten (or cleaned via FR‑4) deterministically.
 - Archives MUST be tar+gzip and use the `.tgz` extension (no alternative extensions).

 - Artifact naming MUST be `<featureId>-<version>.tgz` (lowercase; `featureId` sanitized to `[a-z0-9-]`, collapsing repeated hyphens and trimming leading/trailing hyphens). The `version` MUST be sourced from `devcontainer-feature.json`. If `version` is missing or empty, the system MUST fail with a clear error per FR‑7.
 - In collection mode, the system MUST produce one artifact per feature under the output folder using the naming rule above.

### Determinism Requirements

- Reproducible archives: the implementation MUST enforce byte-for-byte determinism for archive contents and compression.
- Tar header normalization per entry:
   - `mtime = 0` (Unix epoch)
   - `uid = 0`, `gid = 0`; `uname`/`gname` empty
   - `mode` normalized: files 0644; executables 0755; directories 0755
   - Entry path ordering MUST be lexicographically sorted by normalized POSIX path (case-sensitive)
   - Exclude common VCS/admin files such as `.git/` and `.DS_Store`
- Gzip parameters:
   - `mtime = 0`
   - Fixed compression level (e.g., 6) and stable strategy
- Do not include PAX/GNU extensions that carry variable timestamps unless such fields are zeroed.

FR‑3 Collection Metadata
- The system MUST always write `devcontainer-collection.json` in the output folder.
- The file MUST include `sourceInformation.source = "devcontainer-cli"` and a `features[]` array describing each packaged feature (id, version, name, description, options, installsAfter, dependsOn).

FR‑4 Clean Output (Optional Flag)
- When `--force-clean-output-folder` is provided, the system MUST remove previous content of the output folder before packaging.

FR‑5 CLI Interface
- Positional `target` is OPTIONAL and defaults to `.` when omitted.
- `--output-folder, -o` defaults to `./output` when not provided and MUST be created if absent.
- `--log-level` is treated as a global flag; documentation MUST indicate usage: `deacon --log-level debug features package ...`.

FR‑6 Messages and Exit Codes
- Text mode MUST log whether single or collection mode is used and list created artifacts.
- Exit code MUST be 0 on success and non‑zero on any failure described in Error Handling.
 - This subcommand is text‑only: it MUST NOT emit a structured JSON summary, even when a global JSON/logging mode is enabled (JSON reserved for other commands).
 - If a global JSON output mode is supplied (e.g., `--json`), this subcommand MUST fail fast with a non‑zero exit and the message: `JSON output is not supported for features package`.

FR‑7 Error Handling
- Missing `devcontainer-feature.json` in single mode MUST error with context.
- Empty or invalid collection MUST error with context.
- Filesystem write failures MUST surface as actionable errors.
- Mixed valid/invalid subfolders in collection mode MUST fail the entire run: list all invalid subfolders, produce no artifacts, and exit non‑zero.
 - Non‑ASCII path handling: if filesystem encoding prevents reading/writing, the program MUST fail with an actionable error naming the problematic path.
 - Read‑only output folder: the program MUST fail before packaging with `Output folder not writable: <path>`.
 - Deeply nested content: the program MUST archive recursively; if OS path-length limits are hit, the program MUST fail with a message indicating the offending path.

FR‑8 Test Coverage
- Unit/integration tests MUST cover scenarios in “User Scenarios & Testing,” including invalid paths and `-f` behavior.

## Success Criteria (Measurable & Tech‑Agnostic)
SC‑1 Packaging Reliability
- 100% of valid single features and collections produce expected artifacts in one run.

SC‑2 Metadata Completeness
- `devcontainer-collection.json` accurately enumerates 100% of packaged features with required fields.

SC‑3 Deterministic Outputs
 - Re‑running packaging on the same inputs MUST produce byte-for-byte identical artifact files and metadata.

SC‑4 Usability
- A first‑time user can package a collection following one example, completing in under 3 minutes.

## Key Entities
- Feature: directory containing `devcontainer-feature.json` and content to archive.
- Collection: a set of features under `src/` within the target path.
- Output Folder: destination for `*.tgz` and `devcontainer-collection.json`.
- Collection Metadata: JSON file describing packaged features and source information.

## Dependencies
- Local filesystem access for reading sources and writing outputs.
- Existing CLI scaffolding and logging.

## Edge Cases
- Non‑ASCII file/folder names.
- Read‑only or non‑writable output folder.
- Deeply nested content within a feature directory.
- Mixed valid/invalid subfolders under `src/` → fail entire run; list invalid subfolders; no artifacts; exit non‑zero.
 - Expected behaviors:
    - Non‑ASCII names MUST be archived correctly when the filesystem supports them; otherwise fail with a clear, actionable error naming the path.
    - Read‑only output MUST be detected prior to packaging; fail with `Output folder not writable: <path>`.
    - Deep nesting MUST be archived recursively; path-length limit violations MUST report the offending path.

## Out of Scope
- Network operations and publishing to OCI (handled by `features publish`).
- Content validation beyond schema presence (feature tests handled elsewhere).

## Acceptance Scenarios (Summary)
A‑1 Single Feature: produces one `.tgz` and valid collection metadata; exit code 0.
A‑2 Collection (≥2 features): produces one `.tgz` per feature and valid collection metadata; exit code 0.
A‑3 Force Clean: prepopulated output is emptied and only new artifacts remain; exit code 0.
A‑4 Invalid Single Feature: missing/corrupt metadata → fails with clear error; non‑zero exit.
A‑5 Empty Collection: `src/` with zero valid features → fails with clear error; non‑zero exit.

## Risks & Mitigations
- Risk: Ambiguous target path contents. Mitigation: deterministic detection rules with explicit error messages.
- Risk: Drift from upstream spec. Mitigation: keep references in this spec and tests aligned with `docs/.../SPEC.md`.

## Clarification status
None at this time.
