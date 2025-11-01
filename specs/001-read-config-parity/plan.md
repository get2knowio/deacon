# Implementation Plan: Read-Configuration Spec Parity

**Branch**: `001-read-config-parity` | **Date**: 2025-10-31 | **Spec**: /workspaces/deacon/specs/001-read-config-parity/spec.md
**Input**: Feature specification from `/specs/001-read-config-parity/spec.md`

**Note**: This template is filled in by the `/speckit.plan` command. See `.specify/templates/commands/plan.md` for the execution workflow.

## Summary

Close the gaps identified in GAP.md to achieve spec-compliant behavior for `read-configuration`:
- Add missing flags and validations (selectors, id-label regex, terminal pairing, docker paths, features flags)
- Implement container-aware resolution (before-container `${devcontainerId}`) and merged metadata flow
- Implement features resolution (`--include-features-configuration`, `--additional-features`, `--skip-feature-auto-mapping`)
- Enforce strict stdout JSON contract; logs to stderr only

High-level approach:
- Extend argument parsing and validation in the subcommand
- Introduce container discovery/inspect helper and id-label computation
- Wire features planner to compute `featuresConfiguration` and merged metadata per spec
- Update output struct and emitter to always include `configuration` and conditionally include requested fields

## Technical Context

<!--
  ACTION REQUIRED: Replace the content in this section with the technical details
  for the project. The structure here is presented in advisory capacity to guide
  the iteration process.
-->

**Language/Version**: Rust (stable channel per rust-toolchain.toml)
**Primary Dependencies**: clap (CLI args), serde/serde_json (JSON), tracing (logs), thiserror (errors)
**Storage**: N/A
**Testing**: cargo test; assert_cmd for CLI integration; doctests
**Target Platform**: Linux, macOS, Windows, WSL2
**Project Type**: CLI (single workspace with crates/core and crates/deacon)
**Performance Goals**: Fast local execution; output bounded by config size
**Constraints**: Strict stdout JSON contract; no silent fallbacks; no network except Docker inspect when required
**Scale/Scope**: Single subcommand enhancement; touches CLI parsing, core resolution, and output shaping

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

- Spec-Parity as Source of Truth: All changes align with docs/subcommand-specs/read-configuration/SPEC.md → PASS
- Keep the Build Green: Plan includes tests, fmt, clippy enforcement → PASS
- No Silent Fallbacks: Container inspect failure results in error (no fallback) → PASS
- Idiomatic, Safe Rust: No unsafe; thiserror for domain errors; tracing spans → PASS
- Observability and Output Contracts: Strict stdout JSON; logs to stderr → PASS

## Project Structure

### Documentation (this feature)

```text
specs/[###-feature]/
├── plan.md              # This file (/speckit.plan command output)
├── research.md          # Phase 0 output (/speckit.plan command)
├── data-model.md        # Phase 1 output (/speckit.plan command)
├── quickstart.md        # Phase 1 output (/speckit.plan command)
├── contracts/           # Phase 1 output (/speckit.plan command)
└── tasks.md             # Phase 2 output (/speckit.tasks command - NOT created by /speckit.plan)
```

### Source Code (repository root)
<!--
  ACTION REQUIRED: Replace the placeholder tree below with the concrete layout
  for this feature. Delete unused options and expand the chosen structure with
  real paths (e.g., apps/admin, packages/something). The delivered plan must
  not include Option labels.
-->

```text
crates/
├── core/
│   └── src/
│       ├── config.rs
│       ├── docker.rs
│       ├── container.rs
│       └── (helpers for features/merge)
└── deacon/
  └── src/
    └── commands/read_configuration.rs

tests/
├── crates/deacon/tests/
│   ├── integration_read_configuration.rs
│   └── smoke_basic.rs (existing)
└── crates/core/tests/
  └── unit_feature_resolution.rs
```

**Structure Decision**: Single CLI project with workspace crates. Changes in `crates/deacon` (command args/IO) and `crates/core` (resolution helpers). Tests under `crates/deacon/tests/` and `crates/core/tests/`.

## Complexity Tracking

> **Fill ONLY if Constitution Check has violations that must be justified**

| Violation | Why Needed | Simpler Alternative Rejected Because |
|-----------|------------|-------------------------------------|
| (none) | N/A | N/A |
