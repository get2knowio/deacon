# Implementation Plan: Exec Subcommand Parity

**Branch**: `007-exec-subcommand` | **Date**: 2025-11-16 | **Spec**: `/workspaces/deacon/specs/007-exec-subcommand/spec.md`
**Input**: Feature specification from `/specs/007-exec-subcommand/spec.md`

## Summary

Close the GAP for the `exec` subcommand to match the documented behavior: precise container targeting with precedence (`--container-id` > `--id-label` > `--workspace-folder`), deterministic environment merge order (shell userEnvProbe → config remoteEnv → CLI --remote-env), PTY allocation rules (TTY-detected or forced with `--log-format json` and adjustable size), and clear error/exit-code semantics including signal mapping. The technical approach integrates with the existing Rust CLI (`crates/deacon`) and runtime helpers (`crates/core`), leveraging Docker CLI invocation, configuration resolution, and environment probing; it adds tests and preserves the stdout/stderr contract per the constitution.

## Technical Context

**Language/Version**: Rust (stable, Edition 2021)
**Primary Dependencies**: `clap` (CLI), `serde`/`serde_json` (I/O), `tracing` (logs), `thiserror` (errors), `tokio` (async), Docker CLI via process invocation; PTY library selection: NEEDS CLARIFICATION
**Storage**: N/A
**Testing**: `cargo test`, doctests, smoke tests under `crates/deacon/tests/`; fast loop via `make dev-fast`
**Target Platform**: Linux (primary), macOS (best-effort; CI focuses Linux)
**Project Type**: CLI (single workspace with binary crate `crates/deacon` + core crate `crates/core`)
**Performance Goals**: Startup overhead minimal (<250ms added to Docker exec path): NEEDS CLARIFICATION
**Constraints**: No silent fallbacks; strict stdout/stderr contract; zero clippy warnings; formatted code; deterministic tests
**Scale/Scope**: Single subcommand implementation + tests and docs

Known integrations and dependencies:
- Container runtime: Docker CLI (existing helpers). Scope decision: Docker-only for this feature; emit a clear, fail-fast error if Docker is unavailable or only Podman is present (future adapter is out-of-scope).
- Config resolution: As per `docs/CLI-SPEC.md` and existing parsing in repo
- Env probe: `userEnvProbe` modes with default `loginInteractiveShell`

## Constitution Check

Gate assessment prior to Phase 0:
- Spec‑Parity: Align with `docs/CLI-SPEC.md` and `docs/subcommand-specs/exec/SPEC.md` — OK (plan references both)
- Keep Build Green: Plan includes fast loop and full gate commands — OK
- No Silent Fallbacks: Explicit errors for missing selection inputs, missing configs, stopped containers — OK
- Idiomatic, Safe Rust: No `unsafe` planned; errors via `thiserror`; logs via `tracing` — OK
- Observability & Output Contracts: JSON log mode vs text respected; logs to stderr — OK

Result: PASS (proceed to Phase 0). Re-check after Phase 1 design.

Post-Design Recheck (Phase 1): PASS — PTY library choice documented; Docker-only scope explicit with fail-fast errors; output contracts preserved; testing strategy aligns with fast loop/full gate.

## Project Structure

### Documentation (this feature)

```text
specs/007-exec-subcommand/
├── plan.md              # This file (/speckit.plan output)
├── research.md          # Phase 0 output
├── data-model.md        # Phase 1 output
├── quickstart.md        # Phase 1 output
└── contracts/           # Phase 1 output (OpenAPI contract for exec semantics)
```

### Source Code (repository root)

```text
crates/
├── deacon/              # Binary CLI crate (add exec subcommand wiring)
│   ├── src/
│   └── tests/           # Smoke & integration tests for exec
└── core/                # Core helpers (env probe, docker helpers)
    ├── src/
    └── tests/
```

**Structure Decision**: Reuse existing Rust workspace layout (`crates/deacon`, `crates/core`). Add or extend modules under `commands/exec` in `crates/deacon` and corresponding helpers in `crates/core` as needed; tests live alongside existing smoke/integration tests.

## Decisions & Remediation Notes

- PTY backend: Decide during Phase 0 between a Rust PTY crate (e.g., `tokio-pty-process`) and a portable wrapper; tests T021–T024 cover behavior regardless of backend. Implementation detail will not change CLI contract.
- Performance measurement: Measure added overhead to docker exec in CI Linux runners; target <250ms median. Non-blocking; document results in `research.md`.
- Docker-only scope: Adopt Docker as the runtime for this feature; fail fast otherwise.
- JSON log contract: Add explicit tests (T042) to ensure stdout/stderr separation per Constitution §V.

## Complexity Tracking

No constitution violations anticipated. If PTY library selection introduces additional dependencies, justify in PR body with alternatives and rationale (see `research.md`).

## Task Additions (tie-in)

- Defaults & Probe:
    - T033: Default `--default-user-env-probe` to `loginInteractiveShell` (CLI help).
    - T034: Unit tests for `ContainerProbeMode` mapper default/invalids.
- Config Discovery & Errors:
    - T035/T036: Exact missing-config error + negative test with absolute path.
- Effective Config:
    - T037/T038: Core resolver for merging config + image labels and substitution; wire into exec.
- Docker Tooling Flags:
    - T039/T040: Thread and test `--docker-path`, `--docker-compose-path`, `--container-data-folder`, `--container-system-data-folder`.
- Logging & Contracts:
    - T041/T042: Respect global log level; JSON stdout/stderr separation test.
- User Precedence:
    - T043/T044: Implement and test CLI `--user` overriding config `remoteUser`.
