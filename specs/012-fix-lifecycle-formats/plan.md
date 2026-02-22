# Implementation Plan: Fix Lifecycle Command Format Support

**Branch**: `012-fix-lifecycle-formats` | **Date**: 2026-02-21 | **Spec**: [spec.md](spec.md)
**Input**: Feature specification from `/specs/012-fix-lifecycle-formats/spec.md`

## Summary

Fix lifecycle command execution to support all three formats defined by the DevContainer specification — string (shell), array (exec-style), and object (parallel) — for all six lifecycle commands in both container and host execution paths.

The core issue is that the current implementation flattens all command formats to `Vec<String>` and wraps everything in `sh -c`, breaking exec-style semantics for arrays and losing parallel execution for objects. The fix introduces a typed `LifecycleCommandValue` enum that preserves format intent through the execution pipeline, with format-aware execution branches for container (Docker exec) and host (`std::process::Command`) paths.

## Technical Context

**Language/Version**: Rust 1.70+ (Edition 2021)
**Primary Dependencies**: tokio (async runtime, JoinSet for parallel), serde/serde_json (JSON parsing), indexmap (ordered maps for object format), clap (CLI), tracing (logging)
**Storage**: N/A (devcontainer.json configuration files)
**Testing**: cargo-nextest (parallel test execution), unit + integration + docker tests
**Target Platform**: Linux/macOS (Darwin), Docker container runtime
**Project Type**: CLI tool (DevContainer lifecycle management)
**Performance Goals**: Parallel (object) commands must execute concurrently, not sequentially
**Constraints**: Must not regress string-format behavior; must match upstream DevContainer spec
**Scale/Scope**: ~5 files modified in core + deacon crates, ~200-400 lines of implementation changes

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

| Principle | Status | Notes |
|-----------|--------|-------|
| I. Spec-Parity | PASS | Aligns with containers.dev spec for lifecycle command formats (string/array/object) |
| II. Consumer-Only Scope | PASS | Lifecycle execution is consumer functionality (part of `up`, `run-user-commands`) |
| III. Keep Build Green | GATE | Must run `cargo fmt`, `cargo clippy`, `make test-nextest-fast` after every change |
| IV. No Silent Fallbacks | PASS | Invalid formats produce clear errors; no-ops for empty commands are spec-defined |
| V. Idiomatic Rust | PASS | Uses enum variants for type safety, `tokio::JoinSet` for async concurrency, `Result` propagation |
| VI. Observability | PASS | Progress events with per-command attribution; output prefixed by named key for parallel commands |
| VII. Testing Completeness | GATE | Must add tests for array exec-style and object parallel for container + host paths |
| VIII. Subcommand Consistency | PASS | Centralizes format handling in core; both `up` and `run-user-commands` benefit automatically |
| IX. Examples | N/A | No new examples needed; existing lifecycle examples use string format |

**Post-Design Re-check**: All gates pass. The `LifecycleCommandValue` enum centralizes parsing in core (Principle VIII), typed variants prevent silent fallbacks (Principle IV), and the design adds tests for all format × path combinations (Principle VII).

## Project Structure

### Documentation (this feature)

```text
specs/012-fix-lifecycle-formats/
├── plan.md              # This file
├── research.md          # Phase 0: 8 research decisions
├── data-model.md        # Phase 1: LifecycleCommandValue enum, entity relationships
├── quickstart.md        # Phase 1: Usage examples for all three formats
├── contracts/
│   └── lifecycle-execution.md  # Phase 1: Execution contracts per format × path
└── tasks.md             # Phase 2 output (/speckit.tasks command)
```

### Source Code (repository root)

```text
crates/
├── core/src/
│   ├── container_lifecycle.rs   # MODIFY: LifecycleCommandValue enum, format-aware container execution,
│   │                            #         parallel execution via JoinSet, aggregation parsing
│   ├── lifecycle.rs             # MODIFY: Host-side format support (extend from_json_value for object),
│   │                            #         exec-style host execution
│   └── docker.rs                # REVIEW: ExecConfig may need adjustment for exec-style args
│
├── core/tests/
│   ├── test_aggregate_lifecycle_commands.rs  # MODIFY: Add LifecycleCommandValue parsing tests
│   ├── test_lifecycle_formats.rs            # NEW: Unit tests for all format × path combinations
│   └── integration_lifecycle.rs             # MODIFY: Add array/object format integration tests
│
├── deacon/src/commands/up/
│   └── lifecycle.rs             # MODIFY: Remove commands_from_json_value (moved to core),
│                                #         update execute_initialize_command to use typed commands
│
└── deacon/tests/
    └── integration_feature_lifecycle.rs  # MODIFY: Add parallel execution tests
```

**Structure Decision**: Existing Rust workspace structure with `crates/core/` (domain logic) and `crates/deacon/` (CLI binary). Changes primarily in core crate where lifecycle execution lives, with deacon crate updated to delegate format parsing to core.

## Complexity Tracking

No constitution violations. All changes align with existing principles.
