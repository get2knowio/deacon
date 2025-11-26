# Implementation Plan: Enriched mergedConfiguration metadata for up

**Branch**: `001-mergedconfig-metadata` | **Date**: 2025-11-26 | **Spec**: specs/001-mergedconfig-metadata/spec.md  
**Input**: Feature specification from `/specs/001-mergedconfig-metadata/spec.md`

## Summary

Up must emit a spec-compliant `mergedConfiguration` that enriches base config with feature metadata and image/container labels, preserving provenance, ordering, and null semantics for both single and compose flows. We will reuse the `read_configuration` merge logic to generate feature metadata and merge labels so mergedConfiguration differs from the base while remaining JSON-schema compliant.

## Technical Context

**Language/Version**: Rust (stable via rust-toolchain.toml, Edition 2021)  
**Primary Dependencies**: clap, serde/serde_json, indexmap, anyhow/thiserror, tracing, tokio, existing config/feature helpers in crates/core and crates/deacon  
**Storage**: N/A (in-memory config/metadata merge)  
**Testing**: `cargo fmt --all && cargo fmt --all -- --check`; `cargo clippy --all-targets -- -D warnings`; `make test-nextest-fast` default; `make test-nextest-unit` for parsing/merge logic; `make test-nextest-docker` if compose/container label coverage requires it  
**Target Platform**: CLI on Linux/macOS hosts with Docker/Compose runtimes  
**Project Type**: Rust CLI workspace (crates/core for logic, crates/deacon for binary)  
**Performance Goals**: Deterministic merge with negligible overhead (<100ms vs base merge) and stable ordering for feature/label metadata  
**Constraints**: Strict JSON schema/order/null handling; stdout JSON purity; reuse shared merge helpers; no silent fallbacks on missing metadata  
**Scale/Scope**: Configs with multiple features and multi-service compose files; tens of labels/features without noticeable slowdown

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

- Spec-parity: Align with docs/repomix-output-devcontainers-cli.xml and up/read-configuration specs; data structures and ordering must match spec definitions (no Vec vs map drift).  
- Output contracts: JSON stdout only; schema compliance for mergedConfiguration; deterministic ordering and null presence per spec.  
- No silent fallbacks: Missing metadata/labels must surface as null fields, not omission; compose/single paths share logic.  
- Testing discipline: Use fmt+clippy and `make test-nextest-*` targets; add/adjust tests for mergedConfiguration metadata and label handling.  
- Shared helpers: Reuse existing configuration merge and feature metadata helpers; avoid bespoke compose/single divergence.  
- Observability: tracing logs to stderr; redaction for secrets; span alignment with config resolution.  
- Examples/fixtures: Update fixtures/tests as needed; keep outputs deterministic.  
Status: No violations identified; proceed.

## Project Structure

### Documentation (this feature)

```text
specs/001-mergedconfig-metadata/
- plan.md              # This file (/speckit.plan command output)
- research.md          # Phase 0 output (/speckit.plan command)
- data-model.md        # Phase 1 output (/speckit.plan command)
- quickstart.md        # Phase 1 output (/speckit.plan command)
- contracts/           # Phase 1 output (/speckit.plan command)
- tasks.md             # Phase 2 output (/speckit.tasks command - NOT created by /speckit.plan)
```

### Source Code (repository root)

```text
crates/
- core/          # Shared logic, config resolution, feature metadata parsing/merge
- deacon/        # CLI entrypoint, up command, output shaping
- tests/         # Crate-level tests plus workspace fixtures

examples/          # Executable examples with exec.sh
fixtures/          # Deterministic fixtures used in tests
specs/             # Feature specifications
```

**Structure Decision**: Single Rust workspace with core logic in `crates/core` and CLI orchestration/output shaping in `crates/deacon`; tests live under each crate plus workspace fixtures.

## Complexity Tracking

No additional complexity beyond standard workspace; no constitution violations to justify.

## Phase 0: Outline & Research

- Unknowns extracted: None; spec already defines metadata fields, provenance, ordering, and null semantics.  
- Research tasks: Validate reuse of `read_configuration` merge logic for single/compose; confirm label provenance/ordering rules; note alternatives in research.md.

## Phase 1: Design & Contracts

- Data model: Define mergedConfiguration enrichment (feature metadata, image/container labels, provenance, ordering, null handling) aligned to spec.  
- Contracts: OpenAPI-style contract for up success payload with mergedConfiguration enrichment fields and schema notes.  
- Quickstart: Implementation checklist for merging logic reuse, ordering preservation, null handling, and required tests/commands.  
- Agent context: Update via `.specify/scripts/bash/update-agent-context.sh codex`.

## Post-Design Constitution Check

Reconfirm spec-parity, output contract compliance, shared helper reuse, and testing plan after design artifacts are produced.***
