# Implementation Plan: Test Parallelization with cargo-nextest

**Branch**: `001-nextest-parallel-tests` | **Date**: 2025-11-09 | **Spec**: specs/001-nextest-parallel-tests/spec.md
**Input**: Feature specification from `/specs/001-nextest-parallel-tests/spec.md`

## Summary

Introduce cargo-nextest-driven parallel execution for the deacon workspace, grouping tests by resource intensity so developers and CI can run fast and full suites safely while preserving serial fallbacks and capturing timing improvements.

## Technical Context

**Language/Version**: Rust 1.70 (workspace toolchain), cargo-nextest 0.9.x  
**Primary Dependencies**: cargo-nextest, GNU Make, GitHub Actions, existing deacon/deacon-core crates  
**Storage**: N/A  
**Testing**: cargo nextest profiles, cargo test (serial fallback), make targets, GitHub Actions workflows  
**Target Platform**: Developer Linux/macOS workstations; GitHub Actions Ubuntu runners  
**Project Type**: Rust workspace (CLI binary + supporting crates)  
**Performance Goals**: ≥40% faster local loop; 50–70% CI runtime reduction; zero new flaky failures  
**Constraints**: Smoke/parity tests stay serial; fail fast when nextest missing; adhere to constitution principles  
**Scale/Scope**: Entire workspace test estate (unit, integration, smoke, parity) orchestrated via make and CI

## Constitution Check

- **I. Spec-Parity**: No CLI surface change; test tooling updates respect CLI spec. ✓
- **II. Keep the Build Green**: Serial targets remain; nextest added with availability checks to keep CI/local green. ✓
- **III. No Silent Fallbacks**: nextest commands abort loudly when binary absent; no hidden downgrades. ✓
- **IV. Idiomatic, Safe Rust**: Mostly configuration scripts; any Rust tweaks stay within idiomatic patterns. ✓
- **V. Observability & Output Contracts**: nextest reporting retained; timing capture avoids stdout contract violations. ✓

*Gate result: Clear to proceed with Phase 0 research.*

*Post-Phase 1 check: Design artifacts maintain compliance with Principles I–V; no new risks identified.*

## Project Structure

### Documentation (this feature)

```text
specs/001-nextest-parallel-tests/
├── plan.md
├── research.md
├── data-model.md
├── quickstart.md
└── contracts/
```

### Source Code (repository root)

```text
Makefile                 # add nextest-oriented targets and documentation hooks
.config/nextest.toml     # define groups, profiles, and filters
.github/workflows/       # ensure CI installs and invokes cargo-nextest
scripts/                 # timing/report helpers if needed (e.g., compare runtimes)
crates/deacon/tests/     # annotate/inventory tests for grouping
crates/core/tests/       # ensure unit/integration tests classified correctly
docs/                    # update contributor instructions and testing guidance
README.md                # point to new quickstart / commands
```

**Structure Decision**: Centralize orchestration in top-level tooling (Makefile, `.config/nextest.toml`, GitHub Actions) while updating documentation and existing test directories to reflect new grouping metadata.

## Complexity Tracking

_No constitution violations identified._

