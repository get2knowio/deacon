# Implementation Plan: Complete Feature Support During Up Command

**Branch**: `009-complete-feature-support` | **Date**: 2025-12-28 | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification from `/specs/009-complete-feature-support/spec.md`

**Note**: This template is filled in by the `/speckit.plan` command. See `.specify/templates/commands/plan.md` for the execution workflow.

## Summary

Implement complete feature support during the `deacon up` command by extracting and applying feature-declared security options (privileged, init, capAdd, securityOpt), lifecycle commands, mounts, and entrypoints. Additionally, add support for local path (`./`, `../`) and HTTPS tarball feature references alongside existing OCI registry support.

## Technical Context

**Language/Version**: Rust 1.70+ (Edition 2021)
**Primary Dependencies**: clap, serde, tokio, reqwest (rustls TLS), tracing
**Storage**: N/A (devcontainer.json configuration files)
**Testing**: cargo-nextest with test groups (docker-exclusive, docker-shared, fs-heavy)
**Target Platform**: Linux (primary), macOS, Windows
**Project Type**: Single Rust workspace with binary (deacon) and library (deacon_core) crates
**Performance Goals**: Feature resolution <1s for local/OCI features, <30s for HTTPS downloads
**Constraints**: HTTPS downloads with 30-second timeout, single retry on transient errors
**Scale/Scope**: Extension of existing `up` command; 7 functional areas to implement

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

### I. Spec-Parity as Source of Truth
- [x] **Data Model Alignment**: FeatureMetadata struct already contains all required fields (privileged, init, capAdd, securityOpt, mounts, entrypoint, lifecycle commands)
- [x] **Algorithm Alignment**: Security option merging follows OR logic for booleans, union for arrays (matches existing config merge pattern)
- [x] **Configuration Resolution**: Uses existing ConfigLoader::load_with_extends() for full resolution
- [x] **Phased Implementation**: N/A - implementing complete spec requirements

### II. Keep the Build Green
- [x] **Testing Standard**: Will use cargo-nextest test groups for new integration tests
- [x] **Pre-Implementation Validation**: Spec reviewed, data structures mapped, existing infrastructure identified

### III. No Silent Fallbacks - Fail Fast
- [x] **Input Validation**: Feature references validated at parse time (OCI, local path, or HTTPS)
- [x] **Error Handling**: Clear errors for missing local paths, HTTPS failures, invalid mounts
- [x] **Fail-Fast**: Lifecycle command failures stop `up` immediately with exit code 1

### IV. Idiomatic, Safe Rust
- [x] **Error Propagation**: Using Result with anyhow::Context throughout
- [x] **Async Discipline**: HTTPS downloads via async reqwest, no blocking in async context
- [x] **Modular Boundaries**: New logic in existing modules (features_build.rs, lifecycle.rs, container.rs)

### V. Observability and Output Contracts
- [x] **Logging**: Using tracing with structured fields for feature operations
- [x] **Exit Codes**: Lifecycle failures return exit code 1

### VI. Testing Completeness
- [x] **Test Plan**: All 23 functional requirements have corresponding test scenarios
- [x] **Nextest Configuration**: Tests will be assigned to appropriate groups (docker-shared for security option tests)

### VII. Subcommand Consistency & Shared Abstractions
- [x] **Helper Reuse**: Using existing MountParser, ConfigLoader, ContainerLifecycle
- [x] **Avoid Redundant Operations**: Feature resolution performed once, results threaded through execution

### VIII. Executable Examples
- [ ] **TODO**: Add example for features with lifecycle commands after implementation

---

### Post-Design Constitution Re-Check (Phase 1 Complete)

All gates verified after design artifacts generated:

- **I. Spec-Parity**: Data models in data-model.md match spec exactly. Contracts define precise algorithms.
- **II. Build Green**: Test strategy in research.md Decision 10 covers all test groups and categories.
- **III. Fail Fast**: Error handling documented in contracts with source attribution.
- **IV. Idiomatic Rust**: Async patterns for HTTPS, Result propagation throughout.
- **V. Observability**: Tracing spans defined, exit codes documented.
- **VI. Testing**: Test requirements listed in each contract file.
- **VII. Shared Abstractions**: Reuses MountParser, ConfigLoader, ContainerLifecycle.
- **VIII. Examples**: Deferred to implementation phase (documented above).

**Status**: PASS - Ready for task generation (/speckit.tasks)

## Project Structure

### Documentation (this feature)

```text
specs/009-complete-feature-support/
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
├── deacon/                          # CLI binary crate
│   └── src/
│       └── commands/
│           └── up/
│               ├── mod.rs           # Up command entry point
│               ├── container.rs     # MODIFY: Add security options merging
│               ├── features_build.rs # MODIFY: Add local/HTTPS feature support
│               └── lifecycle.rs     # MODIFY: Add feature lifecycle command aggregation
└── core/                            # Core library crate
    └── src/
        ├── features.rs              # EXISTING: FeatureMetadata, ResolvedFeature
        ├── feature_ref.rs           # NEW: Feature reference type detection
        ├── docker.rs                # MODIFY: Pass merged security options
        ├── container_lifecycle.rs   # EXISTING: Lifecycle command execution
        └── mount.rs                 # EXISTING: Mount parsing utilities

crates/deacon/tests/
├── integration_feature_security.rs  # NEW: Security option merging tests
├── integration_feature_lifecycle.rs # NEW: Lifecycle command ordering tests
├── integration_feature_mounts.rs    # NEW: Mount merging tests
├── integration_feature_entrypoints.rs # NEW: Entrypoint chaining tests
└── integration_feature_refs.rs      # NEW: Local/HTTPS feature reference tests
```

**Structure Decision**: Extending existing Rust workspace structure. All new functionality goes into existing crates and modules. Core library handles domain logic (feature reference parsing, data structures), CLI binary handles command orchestration (merging, execution). Tests follow existing pattern in `crates/*/tests/`.

## Complexity Tracking

> **Fill ONLY if Constitution Check has violations that must be justified**

No violations identified. All gates pass.
