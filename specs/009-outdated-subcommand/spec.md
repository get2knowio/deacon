# Feature Specification: Outdated Subcommand Parity

**Feature Branch**: `009-outdated-subcommand`  
**Created**: 2025-11-17  
**Status**: Draft  
**Input**: User description: "Implement the @docs/subcommand-specs/outdated/tasks/ to close the @docs/subcommand-specs/outdated/GAP.md in the @docs/subcommand-specs/outdated/SPEC.md "

## Clarifications

### Session 2025-11-17
- Q: In the JSON report, what should the features map be keyed by? → A: Canonical fully‑qualified feature ID without version
- Q: How should unknown fields be represented in JSON? → A: Null values; keys present
- Q: In CI behavior, should the command support failing when outdated features are detected? → A: Opt‑in flag `--fail-on-outdated`; exit code 2 if any outdated
- Q: How should “latest” be determined from registry tags? → A: Highest stable semver; exclude pre‑releases; ignore non‑semver
- Q: How should a feature be classified as “outdated” for reporting and CI? → A: Outdated if current < wanted OR wanted < latest

## User Scenarios & Testing *(mandatory)*

### User Story 1 - See outdated features quickly (Priority: P1)

As a developer, I want a simple command that tells me which declared Dev Container Features are outdated so I can decide if I should upgrade before building.

**Why this priority**: This delivers immediate, high‑value visibility and is the primary reason for the command.

**Independent Test**: Run the command in a repo with a devcontainer config and features; verify a human‑readable report shows each versionable feature with Current, Wanted, and Latest values in the same order as declared.

**Acceptance Scenarios**:

1. Given a project with versionable Features, When I run the command with default options, Then stdout shows a table with columns "Feature | Current | Wanted | Latest" and each row reflects the corresponding values, where Latest reflects the highest stable semantic version available (pre‑releases excluded).
2. Given a project that has no features in its config, When I run the command, Then it completes successfully and shows an empty report (header only or empty map) with exit code 0.

---

### User Story 2 - Machine‑readable output for CI (Priority: P2)

As a CI/automation engineer, I need a stable JSON report of feature versions so pipelines can gate merges or notify teams when upgrades are available.

**Why this priority**: Enables automation and policy enforcement; critical for adoption in teams and organizations.

**Independent Test**: Run the command with `--output json` and parse it programmatically; confirm the schema is consistent and includes the required fields for each feature.

**Acceptance Scenarios**:

1. Given a project with multiple features, When I run with `--output json`, Then stdout emits a single JSON object with a features map keyed by the canonical fully‑qualified feature ID without version and values including current, wanted, and latest (with unknown values represented as null and keys present).
2. Given a CI environment with non‑interactive output, When I run the command, Then it emits compact JSON without interactive formatting and returns exit code 0.

> JSON Formatting Policy: Non‑interactive (non‑TTY) → compact JSON. Interactive (TTY) → pretty JSON.
3. Given a project with any outdated features, When I run the command with `--fail-on-outdated`, Then it returns exit code 2 and still emits the report (text or JSON per options).

---

### User Story 3 - Resilient and predictable behavior (Priority: P3)

As a developer, I want the command to be reliable, not crash on network issues, and to clearly communicate when information cannot be determined so I can still proceed.

**Why this priority**: Reliability and predictability are essential for developer trust and CI usage.

**Independent Test**: Simulate a registry/network failure and confirm the command completes, leaving unknown fields empty while still reporting on other features.

**Acceptance Scenarios**:

1. Given network issues or a registry error for one feature, When I run the command, Then it completes with exit code 0 and shows undefined/missing values only for the affected feature(s), while others are fully populated.
2. Given an invalid or non‑versionable feature reference, When I run the command, Then the item is skipped (or shown without values) without causing a failure, and the command still returns exit code 0.

---

### Edge Cases

- Config not found → exit code 1 with a clear error message to stderr.

- No features declared in the configuration → output shows an empty report and exit 0.
- Features that are not versionable (e.g., local paths, legacy forms) → omitted from results or shown with missing values; never cause failure.
- Lockfile is absent or does not contain a feature → current falls back to the intended (wanted) version.
- Registries return no valid stable semantic versions or only non‑semantic/pre‑release tags → latest may be undefined; wanted remains per configuration.
- Digest‑pinned features where version metadata cannot be derived → wanted remains undefined unless recorded in the lockfile.
- Very large feature sets (e.g., >20) → command remains responsive; output order remains deterministic.

## Requirements *(mandatory)*

### Functional Requirements

- FR‑001: The command MUST generate an "outdated" report for features declared in the effective devcontainer configuration.
- FR‑002: The default output MUST be human‑readable text that includes, per versionable feature, the columns: Feature | Current | Wanted | Latest.
- FR‑003: The command MUST support a machine‑readable JSON report containing, for each versionable feature, fields for current, wanted, and latest; unknown values MUST be represented as null with keys present. The JSON features map MUST be keyed by the canonical fully‑qualified feature ID without version (e.g., ghcr.io/devcontainers/features/node).
- FR‑004: The report MUST preserve the declaration order of features from the configuration to keep results predictable across runs.
- FR‑005: The command MUST succeed (exit code 0) even if some registries are unreachable; only the affected features show missing data. Configuration not found is a user error and MUST produce exit code 1 with a clear message.
- FR‑006: Non‑versionable or invalid feature identifiers MUST NOT cause failure; they are omitted from results (or shown with missing values) and clearly do not block the report.
- FR‑007: When no lockfile entry exists for a feature, the "current" value MUST fall back to the intended (wanted) version derived from the configuration.
- FR‑008: The command MUST offer `--output json` to emit JSON instead of text to support CI usage.
- FR‑009: The command MUST avoid exposing credentials or secrets in its output or logs.
- FR‑010: The command MUST perform consistently in non‑interactive environments (e.g., CI) without relying on terminal capabilities.
- FR‑011: The command MUST support an opt‑in flag `--fail-on-outdated` which causes the process to exit with code 2 if any declared versionable feature is outdated.
- FR‑012: “Latest” MUST be determined as the highest stable semantic version available from the registry; pre‑release tags are excluded; non‑semantic tags are ignored.
- FR‑013: A feature is considered “outdated” if (a) current < wanted OR (b) wanted < latest; either condition should be indicated in the report and triggers `--fail-on-outdated`.

### Key Entities *(include if feature involves data)

- Feature Identifier: The canonical fully‑qualified feature ID without version (e.g., ghcr.io/devcontainers/features/node).
- Version Information: A set of fields describing current (last used/locked), wanted (based on the configuration’s intent), and latest (highest stable semantic version available) for a feature.
- Outdated Report: The overall result aggregating per‑feature Version Information in deterministic order, in either text or JSON form.

## Success Criteria *(mandatory)*

#### Wanted Version Derivation

- Tag reference: wanted = tag normalized (strip leading 'v'); validate as semver; if non‑semver, wanted = null.
- Digest‑pinned feature: wanted = lockfile version if present; else null.
- Non‑semver tags: wanted = null; comparisons vs latest consider only semver.
- Examples:
  - `ghcr.io/devcontainers/features/node:1.2.3` → wanted = "1.2.3"
  - `...:v1.2.3` → wanted = "1.2.3"
  - `...@sha256:deadbeef` without lockfile → wanted = null

### Measurable Outcomes

- SC‑001: Developers can determine upgrade opportunities from the default text output in under 10 seconds for projects with up to 20 features on a typical broadband connection. Performance validation in tests uses mocked registries (no network).
- SC‑002: JSON output is valid, parseable, and stable across runs (schema and key names unchanged); automated parsing succeeds in >99% of runs under normal conditions.
- SC‑003: In the presence of partial registry/network failures affecting ≤50% of features, the command still exits 0 and produces a report with unaffected features fully populated 100% of the time.
- SC‑004: Output ordering matches the configuration declaration order 100% of the time, enabling stable diffs and predictable CI checks.
- SC‑005: No user credentials or access tokens appear in stdout/stderr during normal operation; manual audits during testing confirm 0 incidents.

## Assumptions & Dependencies

- Assumes a valid devcontainer configuration file can be discovered or specified by users.
- Assumes feature identifiers are resolvable to discoverable version spaces; non‑versionable identifiers are skipped without error.
- Assumes optional lockfile may or may not be present; the command is read‑only and does not write or modify lockfiles.
- Depends on access to upstream registries for version discovery; transient failures must not cause overall command failure.
- Depends on existing logging and configuration discovery behavior elsewhere in the CLI for consistent user experience.

