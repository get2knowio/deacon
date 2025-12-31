# Data Model

## Entity: TestGroup
- **Identifier**: `slug` (string, kebab-case; e.g., `docker-exclusive`)
- **Fields**:
  - `max_threads` (integer ≥ 1) — upper bound for concurrent executions within the group
  - `kind` (enum: `exclusive`, `shared`, `cpu-bound`, `fs-heavy`, `unit`, `smoke`, `parity`)
  - `selectors` (list of test name or package filters applied via `match` rules)
  - `requires_docker` (bool) — indicates integration with Docker runtime
  - `notes` (string) — rationale or onboarding guidance for assignment
- **Relationships**:
  - Referenced by one or more `ExecutionProfile` entries to orchestrate concurrency
  - Populated from existing tests under `crates/*/tests`
- **Validation Rules**:
  - `max_threads` must be 1 for `exclusive`, `smoke`, and `parity` kinds
  - At least one selector is required per group to avoid empty group definitions
  - Groups referencing Docker-only tests must set `requires_docker = true`
- **State Transitions**:
  - Maintainers may move tests between groups; changes require documentation update and PR review

## Entity: ExecutionProfile
- **Identifier**: `name` (string; `dev-fast`, `full`, `ci`)
- **Fields**:
  - `default_parallelism` (integer ≥ 1) — baseline thread count for groups without overrides
  - `group_overrides` (map<TestGroup.slug, max_threads>)
  - `filter` (optional nextest filter expression) to trim slow suites for fast profile
  - `reporters` (enum set: `console`, `junit`, `json`) — determines output artifacts
- **Relationships**:
  - Aggregates one-to-many `TestGroup` definitions
  - Consumed by `MakeTarget` definitions and CI workflows
- **Validation Rules**:
  - Must reference only existing `TestGroup.slug` values
  - `dev-fast` profile must exclude smoke/parity groups explicitly
  - `ci` profile must run every group at least once
- **State Transitions**:
  - Profiles can evolve with new groups; updates require timing comparison per SC-007

## Entity: MakeTarget
- **Identifier**: GNU Make target name (`test-nextest-fast`, `test-nextest`, `test-nextest-ci`)
- **Fields**:
  - `command` (string) — full `cargo nextest run` invocation with profile flag
  - `preflight_check` (bool) — ensures `cargo-nextest` present before running
  - `artifacts_path` (string) — location to store timing/report output
  - `description` (string) — help text shown in `make help`
- **Relationships**:
  - Each target maps to an `ExecutionProfile`
  - Referenced by documentation (`quickstart.md`) and CI workflows
- **Validation Rules**:
  - Preflight check must fail fast with actionable guidance if command not found
  - Targets must return non-zero exit codes on failure and pass through nextest status

## Entity: TimingArtifact
- **Identifier**: `<profile>-timing.json`
- **Fields**:
  - `profile` (string) — matches `ExecutionProfile.name`
  - `duration_seconds` (float) — total runtime for the invocation
  - `timestamp_utc` (ISO 8601 string)
  - `comparison_baseline_seconds` (float, optional) — reference serial duration
  - `notes` (string, optional) — freeform context for anomalies
- **Relationships**:
  - Produced by CI jobs for auditing SC-007
  - Stored under `artifacts/nextest/`
- **Validation Rules**:
  - `profile` must exist in execution profiles
  - `duration_seconds` ≥ 0
  - If `comparison_baseline_seconds` present, must be > 0

## Entity: DocumentationSection
- **Identifier**: `slug` (string; e.g., `testing/nextest`)
- **Fields**:
  - `location` (path relative to repo root)
  - `audience` (enum: `developer`, `maintainer`, `ci`)
  - `content_requirements` (list) — key topics that must be covered
- **Relationships**:
  - References `MakeTarget` and `TestGroup` entities to explain usage
- **Validation Rules**:
  - Must stay synchronized with Make targets and profiles whenever they change
