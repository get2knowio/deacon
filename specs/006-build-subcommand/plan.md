# Implementation Plan: Build Subcommand Parity Closure

**Branch**: `006-build-subcommand` | **Date**: 2025-11-14 | **Spec**: `specs/006-build-subcommand/spec.md`
**Input**: Feature specification from `/specs/006-build-subcommand/spec.md`

**Note**: This template is filled in by the `/speckit.plan` command. See `.specify/templates/commands/plan.md` for the execution workflow.

## Summary

Close the GAP analysis for `deacon build` by delivering full spec-parity behavior: expose the missing CLI flags, enforce validation rules, support Dockerfile, image-reference, and Compose configurations, inject devcontainer metadata/feature customizations, manage BuildKit push/export flows, and return spec-compliant JSON responses. Implementation will extend the existing Rust CLI (`crates/deacon`) and shared logic (`crates/core`) while preserving determinism and failing fast when prerequisites such as BuildKit are unavailable.

## Technical Context

**Language/Version**: Rust 1.70 (workspace toolchain per `rust-toolchain.toml`)  
**Primary Dependencies**: `clap` for CLI parsing, `serde`/`serde_json` for metadata and output, Docker CLI/buildx via process invocation, internal traits/tools for features and configuration resolution  
**Storage**: N/A (temporary filesystem artifacts only)  
**Testing**: `cargo test`, `cargo test --doc`, targeted integration tests under `crates/deacon/tests`, smoke tests (`make test-smoke`), fast loop via `make dev-fast`  
**Target Platform**: Linux/macOS hosts with Docker/BuildKit; CI on Linux runners
**Project Type**: Multi-crate Rust CLI workspace (`crates/deacon`, `crates/core`)  
**Performance Goals**: Match upstream CLI timing; push/export builds complete within 12 minutes per architecture (SC-003)  
**Constraints**: Maintain spec-compliant stdout JSON, keep build green (Constitution II), no silent fallbacks, honor BuildKit gating, deterministic tagging/metadata  
**Scale/Scope**: Single CLI feature set applied across all configuration modes (Dockerfile, image, Compose); supports multiple tags/labels per build

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

- **Principle I (Spec-Parity)**: Plan references `docs/subcommand-specs/build/SPEC.md`, `GAP.md`, and the feature spec to ensure behavior matches the authoritative spec. ✅
- **Principle II (Keep the Build Green)**: Work will follow fast loop (`make dev-fast`) after each change and full gate before PR per Constitution; plan includes test updates for new behavior. ✅
- **Principle III (No Silent Fallbacks)**: Build will fail fast when BuildKit requirements or unsupported flag combinations arise; no placeholder implementations planned. ✅
- **Principle IV (Idiomatic, Safe Rust)**: Changes stay within existing Rust crates without introducing `unsafe`; leverage traits and structured logging already in place. ✅
- **Principle V (Observability & Output Contracts)**: JSON output formatting changes will maintain strict stdout/stderr separation; logs remain on stderr via `tracing`. ✅

Gate status: **PASS** – proceed to Phase 0.

Post-design review (2025-11-14): Phase 1 artifacts uphold all constitutional principles; gate remains **PASS**.

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
```text
crates/
├── core/
│   ├── src/
│   │   ├── config/
│   │   ├── features/
│   │   ├── oci/
│   │   └── build/
│   └── tests/
└── deacon/
  ├── src/
  │   ├── cli.rs
  │   └── commands/
  │       └── build.rs
  └── tests/
    ├── integration_*.rs
    └── smoke_basic.rs

examples/
└── build/
  ├── basic-dockerfile/
  ├── platform-and-cache/
  └── secrets-and-ssh/

docs/
└── subcommand-specs/
  └── build/

fixtures/
└── config/
  └── build/
```

**Structure Decision**: Extend existing Rust workspace modules (`crates/core`, `crates/deacon`) and accompanying tests/examples under `examples/build` to deliver build parity without introducing new top-level packages.

## Implementation Highlights

- **BuildKit-only feature detection** will live in `crates/core/src/build/buildkit.rs`, providing a reusable helper that surfaces explicit errors whenever features require BuildKit on hosts where it is unavailable.
- **Metadata serialization** for devcontainer labels and tags will be centralized in `crates/core/src/build/metadata.rs`, ensuring the build domain owns artifact construction rather than distributing logic across the features module.
- **CLI surface updates** include adding `--push` and `--output` to `crates/deacon/src/cli.rs`, updating help text, and expanding argument parsing tests to enforce mutual exclusivity and documentation parity.

## Complexity Tracking

> **Fill ONLY if Constitution Check has violations that must be justified**

| Violation | Why Needed | Simpler Alternative Rejected Because |
|-----------|------------|-------------------------------------|
| [e.g., 4th project] | [current need] | [why 3 projects insufficient] |
| [e.g., Repository pattern] | [specific problem] | [why direct DB access insufficient] |
