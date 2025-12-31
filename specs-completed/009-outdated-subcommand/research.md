# Research — Outdated Subcommand

## Clarifications Resolved

- Decision: JSON features map keyed by canonical fully‑qualified feature ID without version.
  - Rationale: Matches clarified requirement in spec session; stable keys across tags/digests.
  - Alternatives considered: Use user-declared IDs as-is; rejected for instability and noisy keys.

- Decision: Unknown fields appear as null with keys present in JSON.
  - Rationale: Stable schema for consumers; easier downstream handling than missing keys.
  - Alternatives considered: Omit keys; rejected due to schema instability.

- Decision: `--fail-on-outdated` flag exits with code 2 when any outdated is detected.
  - Rationale: CI gating behavior per clarification; non-conflicting with other exit codes.
  - Alternatives considered: Exit 1; rejected to preserve 1 for user errors.

- Decision: Determine "latest" as highest stable semver; exclude pre‑releases; ignore non‑semver tags.
  - Rationale: Aligns with deacon-core semver utils and upstream expectations.
  - Alternatives considered: Include pre-releases; rejected due to noisy signals.

- Decision: A feature is outdated if current < wanted OR wanted < latest.
  - Rationale: Captures both “behind desired” and “upgrade available” cases.
  - Alternatives considered: Only compare current vs latest; rejected as it misses intent deltas.

## Best Practices — Tech Choices

- CLI parsing with clap v4; reuse global flags in `cli.rs`; add new subcommand `Outdated` with local options `--output-format`, `--fail-on-outdated` and reuse global terminal hints.
- Logging via `tracing`; respect constitution stdout/stderr split; pretty JSON when stdout is TTY.
- Network access via `deacon-core::oci::ReqwestClient`; isolate HTTP in core; tests use fakes/mocks.
- Semver with `deacon_core::semver_utils`; filter and sort tags; compute majors.
- Deterministic ordering: preserve config declaration order when rendering.

## Integration Patterns

- Config discovery: reuse `read_configuration` helpers to get effective config and features list.
- Lockfile read: adjacent to config; read-only; supply `current = lock.version.or(wanted)`.
- Registry failures: produce nulls for affected fields; overall exit 0 unless `--fail-on-outdated` triggers.

## Alternatives Considered

- Implement bespoke semver filtering: rejected; core already ships `semver_utils`.
- Use user-declared feature IDs as JSON keys: rejected per clarification favoring canonical ID without version.
- Hard-fail on any registry error: rejected; spec mandates resilience.
