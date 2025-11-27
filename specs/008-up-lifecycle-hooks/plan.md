# Implementation Plan: Up Lifecycle Semantics Compliance

**Branch**: `008-up-lifecycle-hooks` | **Date**: 2025-11-27 | **Spec**: /workspaces/deacon/specs/008-up-lifecycle-hooks/spec.md
**Input**: Feature specification from `/specs/008-up-lifecycle-hooks/spec.md`

## Summary

Implement devcontainer `up` lifecycle semantics to enforce the ordered phases (onCreate → updateContent → postCreate → dotfiles → postStart → postAttach), resume behavior that reruns only runtime hooks, skip flag behavior that omits post/create hooks and dotfiles, and prebuild behavior that stops after updateContent with isolated markers. Ensure dotfiles ordering, marker-driven resumes, and reporting match the spec and acceptance coverage.

## Technical Context

**Language/Version**: Rust stable (Edition 2021)  
**Primary Dependencies**: clap, serde/serde_json, tracing, anyhow/thiserror, tokio, internal lifecycle helpers in `crates/core` and `crates/deacon`  
**Storage**: None (filesystem state/markers only)  
**Testing**: cargo fmt/clippy; `make test-nextest-*` targets (fast/unit/docker as needed)  
**Target Platform**: Linux devcontainer/host CLI environments  
**Project Type**: CLI workspace with `crates/core` (library) and `crates/deacon` (binary)  
**Performance Goals**: Maintain current `up`/resume runtimes; avoid redundant phase reruns beyond spec; no additional lifecycle passes introduced  
**Constraints**: Must follow CLI spec ordering, output/logging contracts, fail-fast on missing capabilities, and adhere to deferral tracking if any emerge  
**Scale/Scope**: Applies to `up` command lifecycle handling, marker persistence, dotfiles ordering, and skip/prebuild pathways only

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

- Spec-Parity: Must align with `docs/repomix-output-devcontainers-cli.xml` lifecycle semantics and preserve ordering/marker behavior; no silent fallbacks.  
- Deferral Tracking: No deferrals planned; if introduced, record in research.md and tasks.md under Deferred Work.  
- Testing Completeness: Use `make test-nextest-*` targets; add/adjust tests for lifecycle ordering, resume, skip flag, prebuild, and dotfiles sequencing.  
- Keep Build Green: fmt/clippy/nextest required before PR; fail-fast on test failures.  
- Observability/Output Contracts: Respect stdout/stderr separation; summary reporting must match spec ordering.  
Status: Pass (no violations identified for planning). Post-Phase1 check: Pass.

## Project Structure

### Documentation (this feature)

```text
specs/008-up-lifecycle-hooks/
├── plan.md              # This file (/speckit.plan command output)
├── research.md          # Phase 0 output (/speckit.plan command)
├── data-model.md        # Phase 1 output (/speckit.plan command)
├── quickstart.md        # Phase 1 output (/speckit.plan command)
├── contracts/           # Phase 1 output (/speckit.plan command)
└── tasks.md             # Phase 2 output (/speckit.tasks command - NOT created by /speckit.plan)
```

### Source Code (repository root)

```text
/workspaces/deacon/
├── crates/
│   ├── core/src/
│   │   ├── lifecycle.rs
│   │   ├── container_lifecycle.rs
│   │   ├── dotfiles.rs
│   │   ├── state.rs
│   │   └── workspace.rs
│   └── deacon/src/
│       ├── commands/
│       │   ├── up.rs
│       │   ├── shared/
│       │   └── templates.rs
│       ├── ui/
│       └── runtime_utils.rs
├── crates/deacon/tests/    # integration and smoke coverage (includes up, prebuild, dotfiles)
├── fixtures/               # devcontainer and lifecycle fixtures
└── docs/                   # reference specs including repomix-output-devcontainers-cli.xml
```

**Structure Decision**: Use the existing Rust workspace layout with `crates/core` providing lifecycle primitives and `crates/deacon` implementing CLI commands/tests; feature work stays within these paths and the spec folder above.

## Complexity Tracking

> **Fill ONLY if Constitution Check has violations that must be justified**

No constitution violations requiring justification at plan time.
