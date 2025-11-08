# Implementation Plan: 002 — Features Package GAP Closure

**Branch**: `002-features-package-collection` | **Date**: 2025-11-01 | **Spec**: `docs/subcommand-specs/features-package/SPEC.md`
**Input**: Feature specification from `/specs/002-features-package-collection/spec.md`

## Summary

Close the implementation gaps for the `features package` subcommand to support both single-feature and collection packaging. The subcommand will detect mode based on target path, produce deterministic `.tgz` artifacts, and always emit a `devcontainer-collection.json` metadata file. Text-only output is mandated; no JSON mode for this subcommand. Technical approach follows local filesystem packaging with tar+gzip archives and schema-driven validation of `devcontainer-feature.json`.

## Technical Context

**Language/Version**: Rust (stable toolchain, Edition 2021)  
**Primary Dependencies**: `tar` (archiving), `flate2` (gzip), `serde`/`serde_json` (metadata), `clap` (CLI), `tracing` (logs), `thiserror` (errors)  
**Storage**: Local filesystem (read sources, write artifacts)  
**Testing**: `cargo test`, `assert_cmd` for CLI flows; unit tests for detection/validation; integration tests for end-to-end packaging  
**Target Platform**: Cross-platform (Linux/macOS/Windows), validated in CI; dev container default Linux  
**Project Type**: CLI subcommand in Rust workspace (`crates/deacon` for CLI, shared logic in future `crates/core`)  
**Performance Goals**: Deterministic artifacts; handle typical collections (10–50 features) within seconds; avoid unnecessary IO. Deterministic tar/gzip is implemented via tar header normalization (mtime/uid/gid/mode/uname/gname) and fixed gzip parameters (mtime=0, fixed level/strategy).  
**Constraints**: No network operations; fail fast on invalid inputs; keep build green (fmt, clippy, tests)  
    - CLI guard rejects global JSON mode for this subcommand with an actionable error message.
    - Artifact naming uses `<featureId>-<version>.tgz` with a sanitizer: map invalid characters to `-`, collapse repeats, trim leading/trailing hyphens; error if result is empty or version missing.
**Scale/Scope**: Local repos with single feature or collections under `src/` (dozens of features); large directories supported but not optimized for parallelism (non-goal)

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

- I. Spec‑Parity: Aligns with `features-package` SPEC and devcontainer features distribution. Terminology preserved. PASS
- II. Keep the Build Green: Plan includes fmt/clippy/tests per gates; tests updated for new behavior. PASS
- III. No Silent Fallbacks: Invalid single/collection fails with explicit errors; no hidden skips. PASS
- IV. Idiomatic, Safe Rust: No `unsafe`; `thiserror`, `tracing`, traits for future backends. PASS
- V. Observability & Output Contracts: Text-only output, logs via stderr, clear messages; collection JSON written deterministically. PASS
 - Coverage is governed by FR‑8 (Functional Requirements). The prior SC‑5 duplication was removed to avoid drift.

## Project Structure

### Documentation (this feature)

```text
specs/002-features-package-collection/
├── plan.md              # This file (filled by plan workflow)
├── research.md          # Phase 0 output
├── data-model.md        # Phase 1 output
├── quickstart.md        # Phase 1 output
└── contracts/           # Phase 1 output (OpenAPI schemas for metadata)
```

### Source Code (repository root)

```text
crates/
├── deacon/              # CLI entrypoint & subcommand wiring
│   └── src/commands/features/package.rs   # Packaging command (to be implemented/refined)
└── core/                # (optional) Shared types/helpers for packaging workflows
    └── src/features/package/...

tests/
├── crates/deacon/tests/integration_features_package.rs  # E2E CLI tests
└── crates/deacon/tests/unit_features_package.rs         # Unit tests (detection, validation)
```

**Structure Decision**: Implement as a CLI subcommand in `crates/deacon`, extracting reusable logic into `crates/core` only when duplication emerges. Keep changes incremental and test-covered.

## Complexity Tracking

No constitution violations anticipated; no extra complexity justification required at this time.

## Constitution Check (Post-Design)

Re-evaluated after Phase 1 artifacts (research, data model, contracts, quickstart): still compliant with Principles I–V. PASS
