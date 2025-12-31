# Implementation Plan: Outdated Subcommand Parity

**Branch**: `009-outdated-subcommand` | **Date**: 2025-11-17 | **Spec**: docs/subcommand-specs/outdated/SPEC.md
**Input**: Feature specification from `/specs/009-outdated-subcommand/spec.md`

## Summary

Implement the `outdated` subcommand to report Dev Container Feature versions: Current (lockfile or wanted), Wanted (derived from tag/digest rules), and Latest (highest stable semver tag). Provide human-readable table (default) and JSON output keyed by canonical fully‑qualified feature ID without version, respect stdout/stderr contracts, and add an opt‑in `--fail-on-outdated` exit behavior.

## Technical Context

**Language/Version**: Rust 1.70 (Edition 2021, tokio async)
**Primary Dependencies**: clap (CLI), serde/serde_json (I/O), tracing (+subscriber/error), thiserror (errors), anyhow (binary boundary), reqwest (HTTP, rustls TLS), semver (versioning), tokio (async), once_cell, toml
**Storage**: N/A (read-only: config + optional lockfile)
**Testing**: cargo test; assert_cmd for CLI; doctests; nextest optional locally
**Target Platform**: Linux, macOS, Windows, WSL2 (CLI)
**Project Type**: Rust workspace (CLI crate `deacon` + core crate `deacon-core`)
**Performance Goals**: <=10s for up to ~20 features on broadband; parallel tag queries
**Constraints**: No network in tests; zero clippy warnings; no unsafe; JSON stdout-only in JSON mode; compact JSON non-interactive, pretty JSON on TTY; keep build green via Fast Loop cadence
**Scale/Scope**: Single subcommand leveraging existing OCI + semver utilities in core; read-only

## Constitution Check

- I. Spec‑Parity as Source of Truth: Aligns with `docs/subcommand-specs/outdated/SPEC.md` and clarifications in `specs/009-outdated-subcommand/spec.md`.
- II. Keep the Build Green: Use Fast Loop (`make dev-fast`) during iterations; run full gate before PR.
- III. No Silent Fallbacks — Fail Fast: Config discovery failure is a user error (exit 1). Network/registry failures yield nulls in fields per spec; command still exits 0 unless `--fail-on-outdated` applies.
- IV. Idiomatic, Safe Rust: No `unsafe`; errors via `thiserror` in core; `anyhow` at binary boundary; structured `tracing`.
- V. Observability and Output Contracts: JSON mode prints only the JSON to stdout; logs to stderr; secrets redacted.
- Additional Constraints & Security: No arbitrary shell; tests deterministic and hermetic; minimal dependencies.

Gates Evaluation: PASS. No violations requiring justification.

## Project Structure

### Documentation (this feature)

```text
specs/009-outdated-subcommand/
├── plan.md              # This file
├── research.md          # Phase 0 output
├── data-model.md        # Phase 1 output
├── quickstart.md        # Phase 1 output
└── contracts/           # Phase 1 output (OpenAPI + JSON schema)
```

### Source Code (repository root)

```text
crates/
├── deacon/
│  └── src/
│     ├── cli.rs
│     ├── commands/
│     │  ├── build/
│     │  ├── exec/
│     │  ├── read_configuration/
│     │  └── outdated.rs        # NEW (implementation)
│     └── ui/
└── core/
   └── src/
      ├── oci.rs
      ├── semver_utils.rs
      └── ...
```

**Structure Decision**: Extend Rust workspace; add `crates/deacon/src/commands/outdated.rs` and `crates/core/src/outdated.rs` for shared helpers; reuse `deacon-core` OCI + semver utilities.

## Complexity Tracking

| Violation | Why Needed | Simpler Alternative Rejected Because |
|-----------|------------|-------------------------------------|
| (none) |  |  |
