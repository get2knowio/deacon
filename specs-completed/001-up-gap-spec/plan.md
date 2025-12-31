# Implementation Plan: Devcontainer Up Gap Closure

**Branch**: `001-up-gap-spec` | **Date**: 2025-11-18 | **Spec**: specs/001-up-gap-spec/spec.md
**Input**: Feature specification from `/specs/001-up-gap-spec/spec.md`

**Note**: This template is filled in by the `/speckit.plan` command. See `.specify/templates/commands/plan.md` for the execution workflow.

## Summary

Bring `deacon up` to full parity with the documented devcontainer spec gaps: add all missing flags and validations, complete lifecycle (updateContent/prebuild), implement feature-driven builds, compose parity, secrets/dotfiles handling, and emit the required JSON success/error output while keeping logs on stderr. Technical approach centers on tightening clap parsing/validation, normalizing inputs into internal structs, expanding execution paths (buildx, features, compose profiles, UID/security), and enforcing the stdout JSON contract with standardized error mapping.

## Technical Context

<!--
  ACTION REQUIRED: Replace the content in this section with the technical details
  for the project. The structure here is presented in advisory capacity to guide
  the iteration process.
-->

**Language/Version**: Rust stable (2021 edition)  
**Primary Dependencies**: clap, serde/serde_json, anyhow/thiserror, tracing, tokio (as already in repo)  
**Storage**: N/A (CLI orchestrator; uses filesystem for configs/cache)  
**Testing**: cargo test (unit/integration/smoke), cargo fmt, cargo clippy; make dev-fast during iterations  
**Target Platform**: Cross-platform CLI (Linux/macOS/Windows; Docker/Compose runtime)  
**Project Type**: Rust CLI + core library crates (`crates/deacon`, `crates/core`)  
**Performance Goals**: End-to-end `deacon up` success path under 3 minutes for representative scenarios; validation failures fail fast pre-runtime  
**Constraints**: No silent fallbacks; stdout JSON contract strict; logs stderr-only; no `unsafe`; deterministic, hermetic tests (no network)  
**Scale/Scope**: Single CLI binary with supporting core crates; scope limited to `up` parity tasks in `docs/subcommand-specs/up/tasks/`

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

- Principle II Keep the Build Green: Plan adopts fast loop (`make dev-fast`, fmt/clippy/unit+doc tests) and full gate before PR. **PASS**
- Principle III No Silent Fallbacks: Plan requires explicit errors for unimplemented capabilities (e.g., features, GPU, lockfile) per spec. **PASS**
- Principle V Observability/Output: Plan enforces stdout JSON only, logs to stderr, and redaction for secrets. **PASS**
- Idiomatic, Safe Rust: No `unsafe`; use thiserror in core, anyhow at binary boundary; structured tracing spans. **PASS**

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
├── deacon/           # CLI binary: args parsing, command orchestration
├── core/             # Shared logic: container runtime, secrets, dotfiles, redaction, features
└── deacon-tests/     # (if present) integration test helpers

docs/                 # Specifications (including docs/subcommand-specs/up/*)
examples/             # CLI usage examples
fixtures/             # Test fixtures for devcontainer/compose scenarios
specs/001-up-gap-spec/ # This feature's planning artifacts
```

**Structure Decision**: Use existing Rust workspace with `crates/deacon` and `crates/core` as primary implementation targets; tests under `crates/deacon/tests` and shared fixtures in `fixtures/`.

## Complexity Tracking

> **Fill ONLY if Constitution Check has violations that must be justified**

| Violation | Why Needed | Simpler Alternative Rejected Because |
|-----------|------------|-------------------------------------|
| [e.g., 4th project] | [current need] | [why 3 projects insufficient] |
| [e.g., Repository pattern] | [specific problem] | [why direct DB access insufficient] |

## Phase 0: Research

- Outcomes recorded in `specs/001-up-gap-spec/research.md`; no open clarifications remained.

## Phase 1: Design & Contracts

- Data model: `specs/001-up-gap-spec/data-model.md`
- Contracts: `specs/001-up-gap-spec/contracts/up.md`
- Quickstart: `specs/001-up-gap-spec/quickstart.md`
- Agent context updated via `.specify/scripts/bash/update-agent-context.sh codex`

## Constitution Check (Post-Design)

- No violations introduced; plan adheres to build green, fail-fast, output contract, and safe Rust tenets. No complexity tracking entries required.
