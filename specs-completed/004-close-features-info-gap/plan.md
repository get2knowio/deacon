# Implementation Plan: Features Info GAP Closure

**Branch**: `004-close-features-info-gap` | **Date**: 2025-11-01 | **Spec**: `specs/004-close-features-info-gap/spec.md`
**Input**: Feature specification from `specs/004-close-features-info-gap/spec.md`

**Note**: Generated via speckit plan workflow.

## Summary

Close the behavior and output‑contract gaps for the Features Info subcommand as defined in `docs/subcommand-specs/features-info/SPEC.md` and this feature spec. Deliver four modes — `manifest`, `tags`, `dependencies` (text‑only), and `verbose` — with:
- Unified flags: `--output-format <text|json>` and `--log-level <info|debug|trace>` (replace legacy `--json`).
- Deterministic JSON output contracts: always include `canonicalId` (null for local refs), partial‑failure policy for `verbose` JSON, `{}` + exit 1 on errors elsewhere.
- Text mode uses Unicode boxed sections for Manifest/Canonical Identifier, Published Tags, and a Mermaid dependency graph.
- Robust registry interactions: 10s per‑request timeout, pagination cap (10 pages/1000 tags), auth support, and stable tag ordering.

## Technical Context

**Language/Version**: Rust (stable, Edition 2021)
**Primary Dependencies**: clap (CLI), tracing (logs), serde/serde_json (JSON), thiserror (errors), reqwest (HTTP with rustls), tokio (async)
**Storage**: N/A (read‑only network/file operations)
**Testing**: cargo test; unit + doctests default; smoke/integration under `crates/deacon/tests/` per repo guidance
**Target Platform**: Cross‑platform CLI (Linux/macOS/Windows); network I/O only
**Project Type**: Multi‑crate Rust workspace (`crates/core`, `crates/deacon`)
**Performance Goals**: Tag listing returns within ~3s for public refs on typical networks; single manifest fetch under 2s
**Constraints**: No network in unit tests; JSON mode prints only JSON on stdout; redact secrets; exit code 1 on errors; no silent fallbacks
**Scale/Scope**: Single‑command enhancement; touches argument parsing, output formatting utilities, and registry client

NEEDS CLARIFICATION: None — requirements are fully specified by the feature spec and `docs/subcommand-specs/features-info/SPEC.md`. Research tasks still captured in Phase 0 for implementation details and best practices.

## Constitution Check

Gates derived from `.specify/memory/constitution.md`:

- I. Spec‑Parity as Source of Truth: PASS — aligns with `docs/CLI-SPEC.md` and subcommand spec; any divergence will be flagged in PR.
- II. Keep the Build Green: PASS — plan adheres to mandatory fmt, clippy, tests; updates will include tests/examples.
- III. No Silent Fallbacks — Fail Fast: PASS — JSON `{}` + exit 1 or partial failure policy for `verbose`; no hidden stubs.
- IV. Idiomatic, Safe Rust: PASS — no `unsafe`; traits for registry client; structured errors via `thiserror` in core.
- V. Observability and Output Contracts: PASS — stdout JSON only; logs to stderr; structured spans for `feature.info.*` operations.

Re‑check is required post‑design to ensure contracts and tests remain compliant. Current status: PASS.

## Project Structure

### Documentation (this feature)

```text
specs/004-close-features-info-gap/
├── plan.md              # This file
├── research.md          # Phase 0 output
├── data-model.md        # Phase 1 output
├── quickstart.md        # Phase 1 output
└── contracts/           # Phase 1 output (OpenAPI + JSON Schemas)
```

### Source Code (repository root)

```text
crates/
├── core/                # Registry client, models, JSON contracts, text boxing utils (shared)
│   └── src/
├── deacon/              # CLI entrypoint; arg parsing, subcommand wiring, output formatting
│   └── src/
└── ...

tests/
├── crates/deacon/tests/ # Smoke/integration for CLI outputs and exit codes
└── crates/core/tests/   # Unit tests for parsing/registry client helpers
```

**Structure Decision**: Extend existing multi‑crate layout. Implement `features info` orchestration in `crates/deacon`, delegate registry/network logic and output data types to `crates/core` behind traits for testability.

## Complexity Tracking

No constitution violations anticipated; table not required.
