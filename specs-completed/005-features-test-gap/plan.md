# Implementation Plan: Features Test GAP Closure

**Branch**: `005-features-test-gap` | **Date**: 2025-11-01 | **Spec**: `/workspaces/deacon/docs/subcommand-specs/features-test/SPEC.md`
**Input**: Feature specification from `/workspaces/deacon/specs/005-features-test-gap/spec.md`

**Note**: Generated per speckit plan workflow. See `.specify/templates/commands/plan.md` for process.

## Summary

Close the behavior and output gaps for the `features test` subcommand per the spec: implement strict flag validation (exclusivity and structure checks), scenario selection semantics (including `_global` rules), duplicate/idempotence policy, clear error handling with no silent fallbacks, and a strict JSON output mode that prints exactly `[ { "testName": string, "result": boolean } ]` to stdout when `--json` is set. Execution remains serial by default; containers are labeled and cleaned up unless `--preserve-test-containers` is provided.

## Technical Context

**Language/Version**: Rust (stable; per `rust-toolchain.toml`)  
**Primary Dependencies**: `clap` (CLI args), `serde`/`serde_json` (JSON), `tracing` (logs), `thiserror` (errors); Docker/engine integration via existing runtime helpers in the repo  
**Storage**: N/A (ephemeral workspaces only)  
**Testing**: `cargo test`, doctests; CLI e2e via `assert_cmd` and fixtures under `fixtures/` and `examples/`  
**Target Platform**: Linux, macOS, Windows/WSL (consistent path handling; runtime via Docker/Desktop/WSL)  
**Project Type**: Rust workspace CLI (`crates/deacon` entrypoint, shared logic planned/located in `crates/core`)  
**Performance Goals**: Deterministic, serial execution; predictable runtime for small suites (minutes). Parallelization is deferred.  
**Constraints**: Follow Constitution v1.3.0 — keep build green, no silent fallbacks, strict stdout/stderr contracts, idiomatic safe Rust  
**Scale/Scope**: Collections with dozens of tests; sequential container launches; no persistent state  
**Platform Semantics**: Case-sensitive discovery and filtering across platforms; normalize path separators; support spaces in paths and feature IDs; default base image is `ubuntu:focal` unless overridden by `--base-image`.

NEEDS CLARIFICATION (addressed in research):
- Whether to introduce limited parallelism for scenarios (decision: defer; keep serial for now)
- Exact container runtime trait surface for test labeling/cleanup (decision: reuse existing runtime helpers; expand later if needed)
- Randomization flag semantics for `--permit-randomization` (decision: defer; emit explicit "Not implemented yet: randomization" error when provided to comply with Constitution III)

## Constitution Check (pre‑design)

- II. Keep the Build Green: PASS — plan honors fast/full gates; no code merge without fmt/clippy/tests.
- III. No Silent Fallbacks: PASS — invalid flags and missing preconditions are explicit errors; runtime unavailability fails fast.
- IV. Idiomatic, Safe Rust: PASS — no `unsafe`; use `thiserror` in core and `anyhow` only at binary boundary.
- V. Observability and Output Contracts: PASS — JSON mode prints only the array to stdout; logs go to stderr; `--quiet` respected.
- Spec‑Parity: PASS — aligns with `docs/subcommand-specs/features-test/SPEC.md` and this feature spec.

## Project Structure

### Documentation (this feature)

```text
specs/005-features-test-gap/
├── plan.md              # This file (speckit.plan output)
├── research.md          # Phase 0 output
├── data-model.md        # Phase 1 output
├── quickstart.md        # Phase 1 output
└── contracts/           # Phase 1 output (OpenAPI for result schema)
```

### Source Code (repository root)

```text
crates/
├── deacon/              # CLI entrypoint (subcommands via clap)
│   ├── src/
│   └── tests/           # integration & smoke tests
└── core/                # shared logic (parsing, models, runtime abstractions)
    └── src/
```

**Structure Decision**: Keep logic behind trait abstractions in `crates/core` (e.g., container runtime operations and test execution helpers). Add/extend models and parsers as needed for scenario discovery and results while keeping the CLI orchestration in `crates/deacon`.

## Complexity Tracking

No constitution violations anticipated; no complexity waivers required.

## Constitution Check (post‑design)

Re‑evaluated after producing Phase 0/1 artifacts (`research.md`, `data-model.md`, `contracts/openapi.yaml`, `quickstart.md`).

- II. Keep the Build Green: PASS — documentation‑only changes; future code will follow fast/full gates.
- III. No Silent Fallbacks: PASS — explicit validations and errors called out in model and quickstart.
- IV. Idiomatic, Safe Rust: PASS — no `unsafe` planned; errors via `thiserror` in core, `anyhow` at binary boundary.
- V. Output Contracts: PASS — strict JSON array, logs on stderr, text summary rules documented.
- Spec‑Parity: PASS — artifacts align with `docs/subcommand-specs/features-test/SPEC.md` and feature spec clarifications.
