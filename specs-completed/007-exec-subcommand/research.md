# Research: Exec Subcommand Parity

Date: 2025-11-16
Branch: 007-exec-subcommand
Spec: /workspaces/deacon/specs/007-exec-subcommand/spec.md

## Unknowns Extracted from Technical Context

1. PTY library selection for interactive execution (Rust): NEEDS CLARIFICATION
2. Performance goal for startup overhead (<250ms) exact target: NEEDS CLARIFICATION
3. Runtime backends beyond Docker (Podman support): NEEDS CLARIFICATION

## Research Tasks and Findings

### Decision: PTY Library
- Decision: Use `portable-pty` with `tokio` integration for cross-platform PTY; fall back to `nix` APIs on Linux if needed.
- Rationale: `portable-pty` provides a higher-level abstraction with good Windows/macOS/Linux coverage; aligns with interactive shell use cases. For Linux CI, `nix` can provide fine-grained control if necessary.
- Alternatives considered:
  - `tokio-pty-process`: Less mature, fewer examples, narrower platform support.
  - Raw `nix` + `forkpty`: Powerful but low-level; more code and edge-case handling.

### Decision: Performance Goal
- Decision: Target added CLI overhead ≤ 200ms p95 over raw `docker exec` for non-interactive runs; interactive runs may incur PTY setup (<300ms p95).
- Rationale: Keeps UX snappy in local development and CI; aligns with constitution principle II (fast loop productivity).
- Alternatives considered:
  - No explicit target: Risks slow regressions.
  - Aggressive <100ms: Potentially unrealistic given process spawning and logging setup.

### Decision: Runtime Backend Scope
- Decision: Scope to Docker CLI for this milestone; emit explicit error if non-Docker runtime requested (no silent fallback). Document Podman support as follow-up.
- Rationale: Repository already uses Docker helpers; keeps changes minimal and focused per Prime Directives. Avoids hidden divergence.
- Alternatives considered:
  - Add Podman parity now: Increases scope and risk; needs abstraction work.
  - Experimental Podman detection: Violates “no silent fallbacks,” risks inconsistent behavior.

## Best Practices & Patterns Considered
- Env merge order: compute via pure functions; unit test with table-driven cases for `remoteEnv`, CLI `--remote-env`, and `userEnvProbe` results.
- Container selection: validate `--id-label` as `name=value` pairs; deterministic precedence; explicit error if none provided.
- Exit code mapping: propagate child code; map signals to `128 + signal` consistently; default to `1` when unknown.
- Logging: honor text vs json mode; send logs to stderr; redact secrets in env dumps.

## Outcomes
- All NEEDS CLARIFICATION items are resolved with the decisions above.
- Proceed to Phase 1: design (data model, contracts, quickstart) and update agent context.
