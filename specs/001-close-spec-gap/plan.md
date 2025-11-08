# Implementation Plan: Close Spec Gap (Features Plan)

**Branch**: `001-close-spec-gap` | **Date**: 2025-11-01 | **Spec**: /workspaces/001-features-plan-cmd/specs/001-close-spec-gap/spec.md
**Input**: Feature specification from `/workspaces/001-features-plan-cmd/specs/001-close-spec-gap/spec.md`

**Note**: This plan is produced by the /speckit.plan workflow and will be kept small and incremental per repository conventions.

## Summary

Implement the “features plan” behavior to close gaps with the spec: validate `--additional-features` as a JSON object, reject local feature paths, build a deterministic installation order with lexicographic tie‑breakers, and emit a graph of direct dependencies (union of `installsAfter` and `dependsOn`). Merge option maps shallowly with CLI precedence and fail fast on any registry metadata fetch failure. Output contracts follow JSON mode rules (stdout JSON only; logs on stderr).

## Technical Context

**Language/Version**: Rust (stable toolchain per `rust-toolchain.toml`, Edition 2021)
**Primary Dependencies**: clap (CLI), serde/serde_json (parsing/JSON), thiserror (errors), tracing (logs)
**Storage**: N/A (in‑memory planning only)
**Testing**: cargo test; unit tests for pure logic; CLI tests with assert_cmd; doctests compile
**Target Platform**: Linux/macOS CLI (no network in unit tests; registry fetch simulated in tests)
**Project Type**: Rust workspace CLI — binary crate at `crates/deacon` orchestrates; shared logic at `crates/core`
**Performance Goals**: Favor correctness and determinism over throughput; handle O(100) features comfortably
**Constraints**: Strict stdout/stderr contract in JSON mode; fail‑fast on errors (no partial plan); zero clippy warnings; rustfmt clean
**Scale/Scope**: Typical devcontainer feature sets (dozens of features); no deep graph (>1e3 nodes) in scope

## Constitution Check

*GATE: Must pass before Phase 0 research. Re‑check after Phase 1 design.*

- I. Spec‑Parity as Source of Truth: PASS — behavior aligns with `docs/CLI-SPEC.md` and feature spec clarifications.
- II. Keep the Build Green: PASS — plan commits to fmt/clippy/tests cadence; tests will be added with changes.
- III. No Silent Fallbacks: PASS — planner fails fast on invalid input and registry fetch failures; no partial outputs.
- IV. Idiomatic, Safe Rust: PASS — no `unsafe`; structured errors via `thiserror`; logging with `tracing`.
- V. Observability & Output Contracts: PASS — JSON plan to stdout only; logs to stderr; redact sensitive values.

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
  core/           # shared domain logic (parsing, planning, error types)
    src/
    tests/
  deacon/         # CLI binary (args, orchestration, IO contracts)
    src/
    tests/

specs/001-close-spec-gap/
  plan.md         # this plan
  research.md     # Phase 0
  data-model.md   # Phase 1
  quickstart.md   # Phase 1
  contracts/      # Phase 1
```

**Structure Decision**: Use existing Rust workspace with binary crate at `crates/deacon` and shared logic in `crates/core`. Feature documentation and contracts live under `/workspaces/001-features-plan-cmd/specs/001-close-spec-gap/`.

## Complexity Tracking

> **Fill ONLY if Constitution Check has violations that must be justified**

| Violation | Why Needed | Simpler Alternative Rejected Because |
|-----------|------------|-------------------------------------|
| [e.g., 4th project] | [current need] | [why 3 projects insufficient] |
| [e.g., Repository pattern] | [specific problem] | [why direct DB access insufficient] |

