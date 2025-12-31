# Implementation Plan: Up Build Parity and Metadata

**Branch**: `007-up-build-parity` | **Date**: 2025-11-27 | **Spec**: specs/007-up-build-parity/spec.md
**Input**: Feature specification from `/specs/007-up-build-parity/spec.md`

## Summary

Align the `up` subcommand with the devcontainers spec so BuildKit cache-from/cache-to/buildx options apply uniformly to Dockerfile and feature builds, skip-feature-auto-mapping and lockfile/frozen enforcement keep feature resolution deterministic, and feature metadata is merged into `mergedConfiguration`. Technical approach: thread build options through both build paths, enforce fail-fast validation for unsupported BuildKit/buildx or lockfile drift, reuse shared config/feature resolution helpers, and extend merged configuration construction to include metadata for every built feature.

## Technical Context

**Language/Version**: Rust stable (2021 edition per workspace toolchain)  
**Primary Dependencies**: clap, serde/serde_json, anyhow/thiserror, tracing, tokio, cargo-nextest for tests; reuse existing config/feature/build helpers in `crates/core` and CLI wiring in `crates/deacon`  
**Storage**: Filesystem-based config/lockfile inputs and merged outputs; no persistent DB  
**Testing**: `cargo fmt --all && cargo fmt --all -- --check`, `cargo clippy --all-targets -- -D warnings`, targeted `make test-nextest-unit` for parsing/logic, `make test-nextest-docker` for build/runtime flows, `make test-nextest-fast` for combined quick runs  
**Target Platform**: Linux CLI environments running Docker/BuildKit/buildx as per devcontainer workflows  
**Project Type**: Rust CLI workspace (`crates/core`, `crates/deacon`)  
**Performance Goals**: Preserve current `up` latency; when cache-from/cache-to provided, builds must leverage caches without regressions; no added retries that mask failures  
**Constraints**: Fail-fast on unsupported BuildKit/buildx or lockfile drift; no silent fallbacks; maintain stdout/stderr contracts including JSON purity; reuse shared helpers per constitution  
**Scale/Scope**: Typical devcontainer configs with multiple features (small tens) and single Dockerfile build per `up` invocation

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

- Spec parity (Principle I): Must align with docs/repomix-output-devcontainers-cli.xml and containers.dev; data shapes/order must follow spec; build/feature behaviors cannot diverge. **Status: Pass** (spec reviewed, shapes captured in spec/data-model sections).
- Keep build green (Principle II): Use fmt+clippy and `make test-nextest-*` targets (unit → docker → fast) per change scope; fix failing tests, no skips. **Status: Pass** (plan adopts required cadence).
- No silent fallbacks (Principle III): Unsupported BuildKit/buildx or lockfile drift must fail with clear errors; cache fetch issues warn but still surface degraded caching. **Status: Pass** (captured in requirements/research).
- Idiomatic, safe Rust (Principle IV): No `unsafe`; structured errors via thiserror/anyhow at boundary; use tracing spans; avoid blocking async. **Status: Pass** (existing patterns reused).
- Observability & output contracts (Principle V): JSON stdout purity; ordering preserved for features/metadata; exit codes consistent across modes. **Status: Pass** (to be enforced in contracts/tests).
- Testing completeness & nextest config (Principle VI): Add/align tests and nextest groups for new integration coverage. **Status: Pass** (tests planned; nextest overrides to be updated if new bins).
- Shared helpers (Principle VII): Reuse up/config/feature resolution helpers; extend builder-style merged configuration enrichment. **Status: Pass** (design favors shared components).
- Examples hygiene (Principle VIII): Update examples/fixtures only if user-facing behavior changes; keep exec.sh in sync. **Status: Pass** (no deviations planned).
- Deferral tracking: No deferrals planned; if introduced, document in research.md and tasks.md under Deferred Work. **Status: Pass (no deferrals)**

Post-Phase-1 check: All gates remain satisfied with the design artifacts produced.

## Project Structure

### Documentation (this feature)

```text
specs/007-up-build-parity/
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
├── core/    # shared devcontainer/core logic, feature/config resolution, helpers
└── deacon/  # CLI binary, subcommand wiring (including up)

examples/    # executable examples with exec.sh per scenario
fixtures/    # deterministic fixtures for tests
tests/       # workspace-level tests if present; crate-specific tests live under each crate
```

**Structure Decision**: Use existing Rust workspace with `crates/core` for shared logic and `crates/deacon` for CLI; extend `up` paths and merged configuration builder within these crates, keeping tests alongside respective crates and fixtures/examples in their existing roots.

## Complexity Tracking

> **Fill ONLY if Constitution Check has violations that must be justified**

| Violation | Why Needed | Simpler Alternative Rejected Because |
|-----------|------------|-------------------------------------|
| _None_ | — | — |

No constitution violations identified; table remains empty.
