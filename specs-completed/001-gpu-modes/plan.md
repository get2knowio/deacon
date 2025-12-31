# Implementation Plan: GPU Mode Handling for Up

**Branch**: `001-gpu-modes` | **Date**: 2025-11-26 | **Spec**: [specs/001-gpu-modes/spec.md](./spec.md)  
**Input**: Feature specification from `/specs/001-gpu-modes/spec.md`

## Summary

Implement GPU mode handling for the `up` workflow to align with `docs/repomix-output-devcontainers-cli.xml`: support GPU modes `all`, `detect`, and `none`, defaulting to `none`, and apply the selected mode consistently across docker run/build and compose paths with appropriate warnings and user-facing output.

## Technical Context
**Language/Version**: Rust 2021 (rust-toolchain pinned)  
**Primary Dependencies**: clap, serde/serde_json, anyhow/thiserror, tracing, Docker CLI/compose integration helpers  
**Storage**: N/A (runtime configuration only)  
**Testing**: cargo-nextest via `make test-nextest-fast` as default loop; clippy/fmt per constitution  
**Target Platform**: Linux hosts running Docker/Compose (devcontainer-compatible)  
**Project Type**: CLI (multi-crate: `crates/core`, `crates/deacon`)  
**Performance Goals**: Preserve existing `up` latency; GPU detection adds <1s overhead and does not delay container startup noticeably  
**Constraints**: Must mirror spec behavior exactly (docs/repomix-output-devcontainers-cli.xml); no silent fallbacks; consistent JSON/text output contracts; warnings on stderr; stdout JSON unchanged; no network dependence for GPU detection  
**Scale/Scope**: Applies to all `up`-initiated run/build/compose operations in typical devcontainer projects (single or multi-service)

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

- Spec-parity: Plan adheres to `docs/repomix-output-devcontainers-cli.xml` and CLI spec alignment.  
- No silent fallbacks: Detect mode warns and proceeds; all mode surfaces runtime failures; none mode no-ops GPU requests.  
- Build/test hygiene: Use `cargo fmt --all`, `cargo clippy --all-targets -- -D warnings`, and targeted nextest suites (start with `make test-nextest-fast`; include docker/compose-focused suites if runtime/compose wiring is touched).  
- Shared abstractions: Reuse existing helpers for Docker/Compose flag wiring; avoid bespoke parsing.  
- Observability/output contracts: Maintain stdout/stderr separation; preserve ordering and schema; ensure warnings in detect mode are user-visible.

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
├── core/          # shared logic, helpers, data models
└── deacon/        # CLI binary, subcommand wiring (including up)

docs/              # specifications and references (incl. repomix output)
examples/          # runnable examples with exec.sh per constitution
fixtures/          # test fixtures
scripts/           # helper scripts
specs/             # feature specifications and plans
tests/             # integration/unit tests (aligned with nextest config)
```

**Structure Decision**: Use existing multi-crate CLI layout (`crates/core`, `crates/deacon`) with specs and docs under `specs/` and `docs/`; no new top-level packages required.

## Complexity Tracking

No constitution violations anticipated; complexity tracking not required for this feature.

## Phase 0 - Research (Complete)
- Resolved GPU detection approach, uniform propagation across run/build/compose, and warning policy for detect mode.
- Output: [research.md](./research.md) with decisions, rationale, and alternatives.

## Phase 1 - Design & Contracts (Complete)
- Data model captured in [data-model.md](./data-model.md).
- API/interaction contract captured in [contracts/openapi.yaml](./contracts/openapi.yaml) covering GPU mode application.
- Quickstart usage captured in [quickstart.md](./quickstart.md).
- Constitution re-check: aligns with spec parity, no silent fallbacks, and observability/output contracts; no new risks identified.
