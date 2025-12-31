# Implementation Plan: Workspace Mount Consistency and Git-Root Handling

**Branch**: `006-align-workspace-mounts` | **Date**: 2025-11-27 | **Spec**: /workspaces/deacon/specs/006-align-workspace-mounts/spec.md
**Input**: Feature specification from `/specs/006-align-workspace-mounts/spec.md`

## Summary

Align workspace mount generation with the spec so that (1) the user-selected workspace mount consistency is preserved and visible across Docker and Compose outputs, and (2) the git-root flag mounts the repository root consistently for both Docker and Compose flows with a clear fallback when no git root exists. Technical approach: reuse existing workspace discovery, apply consistency at mount construction, extend git-root detection to both runtimes, and keep failure handling explicit and aligned with the spec (no silent fallbacks).

## Technical Context

**Language/Version**: Rust (stable, Edition 2021)  
**Primary Dependencies**: clap, serde/serde_json, anyhow/thiserror, tracing, tokio; local crates `core` and `deacon`  
**Storage**: N/A (filesystem discovery only)  
**Testing**: `make test-nextest-fast` during dev; targeted `make test-nextest-unit` for parsing/logic; `make test-nextest` before PR  
**Target Platform**: Linux containers/devcontainers; CLI runtime on host  
**Project Type**: Single CLI workspace (Rust workspace with binary + core crate)  
**Performance Goals**: Mount discovery and rendering incur no perceptible delay (<200ms per invocation path)  
**Constraints**: No silent fallbacks; mount ordering and values must match spec; logging respects stdout/stderr split  
**Scale/Scope**: Single-feature change touching workspace mount discovery/rendering for Docker and Compose flows

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

- Spec-parity: Behavior must match `docs/repomix-output-devcontainers-cli.xml` and CLI spec for workspace mounts; no divergence allowed. **Status: OK.**
- Keep build green: Follow fmt → clippy → `make test-nextest-fast` cadence; full `make test-nextest` before PR. **Status: OK.**
- No silent fallbacks: Git-root detection failures must surface and fall back explicitly; outputs must stay aligned across Docker/Compose. **Status: OK.**
- Observability/output contracts: Respect stdout/stderr separation; preserve mount ordering/values. **Status: OK.**
- Post-design check: Research/design artifacts introduce no new violations. **Status: OK.**

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
```text
crates/
├── core/        # shared workspace/runtime logic
├── deacon/      # CLI binary and runtime orchestration

docs/            # specs and reference docs (includes CLI spec)
examples/        # runnable examples with exec.sh aggregators
fixtures/        # test fixtures
tests/ (via crates/deacon/tests) # integration/smoke coverage
```

**Structure Decision**: Single Rust workspace with `core` + `deacon` crates; updates will focus on workspace discovery/mount rendering modules and associated tests under `crates/deacon/tests`.

## Complexity Tracking

> **Fill ONLY if Constitution Check has violations that must be justified**

| Violation | Why Needed | Simpler Alternative Rejected Because |
|-----------|------------|-------------------------------------|
| (none) | - | - |

## Phase 0 - Research

- Confirm workspace discovery inputs (cwd vs configured workspace) and how consistency flag is currently surfaced in Docker and Compose mount builders.
- Validate git-root detection path (expected: git top-level relative to working dir) and how it threads into Docker and Compose mount generation.
- Decide fallback messaging/behavior when git root is absent while ensuring outputs stay aligned across Docker/Compose.
- Capture test scope: prioritize unit-level coverage for path selection/rendering; target integration coverage if output formatting flows differ.

Deliverable: `research.md` capturing decisions, rationale, and alternatives.

## Phase 1 - Design & Contracts

- Produce `data-model.md` describing workspace discovery inputs/outputs, mount definitions, and git-root resolution artifacts.
- Produce API/CLI contract in `contracts/` describing Docker and Compose mount outputs with consistency and git-root toggles, plus expected fallback responses.
- Write `quickstart.md` with runbook for implementing and validating the feature (fmt → clippy → targeted nextest).
- Run `.specify/scripts/bash/update-agent-context.sh codex` to update agent context with any new tech or constraints discovered.
- Re-run Constitution Check to ensure design choices stay within gates.

## Phase 2 - Implementation & Testing Outline

- Implement workspace mount consistency propagation across Docker/Compose builders; align git-root detection across both.
- Add tests for mount path selection (workspace vs git root), consistency value propagation, and fallback behavior; configure any new integration tests in `.config/nextest.toml` with appropriate groups.
- Validation cadence: `cargo fmt --all && cargo fmt --all -- --check`, `cargo clippy --all-targets -- -D warnings`, `make test-nextest-unit` (logic), `make test-nextest-fast` (broader), `make test-nextest` before PR.
