# GEMINI: Guide for AI Assistants in the `deacon` repository

This document provides the essential guidelines and principles for AI assistants working on this project. Adherence to these rules is mandatory. This is your authoritative source of truth, synthesized from the project's constitution and agent-specific instructions.

## 1. Core Principles: Spec-Driven Development

### I. Spec‑Parity is the Non-Negotiable Source of Truth
- **Authoritative Specs**: Deacon MUST implement behavior consistent with the specifications in `docs/CLI-SPEC.md` and `docs/subcommand-specs/*/SPEC.md`. Terminology MUST be preserved.
- **Data Model and Algorithm Alignment**: Implementation data structures MUST exactly match spec-defined shapes (e.g., `map<string, T>` vs. `Vec<T>`, null handling, field presence, ordering). Algorithms defined in the spec MUST be followed step-by-step.
- **Complete Configuration Resolution**: Commands MUST use the full resolution path (`extends` chains, overrides, variable substitution), not just the top-level file.

### II. No Silent Fallbacks — Fail Fast
- Production code MUST NOT silently downgrade, noop, or substitute mock/stub implementations. It MUST emit a clear, user-facing error and abort.
- Mocks/fakes are permitted ONLY in tests.
- Invalid inputs (e.g., non-OCI feature refs) MUST be filtered or skipped at ingress points as specified. Do not pass them to downstream logic.

## 2. Pre-Implementation Protocol: The Spec Alignment Checklist

**CRITICAL**: BEFORE writing any implementation code for a new subcommand or feature, you MUST complete and document the following checklist (e.g., in a `plan.md` or PR description):

1.  **Full Spec Read**: Have you read the COMPLETE spec section (`SPEC.md`, `data-model.md`, `contracts/`) to understand all requirements?
2.  **Data Model Match**: Do your data structures match the spec shapes exactly? (Check: `map` vs `vec`, field names, null semantics, ordering).
3.  **Algorithm Fidelity**: Have you identified all spec-defined algorithms (e.g., version derivation, `extends` resolution, tag selection) and planned to implement them precisely?
4.  **Input Validation**: Where will you filter invalid inputs (e.g., non-OCI refs, non-semver tags) as defined by the spec?
5.  **Config Resolution**: If your command reads configuration, does it use the full resolution path to honor `extends` chains and overrides?
6.  **Output Contracts**: Have you verified the JSON schema (key names, presence, null handling), output ordering, and exit code contracts for ALL output modes?
7.  **Test Coverage**: Have you listed ALL spec-mandated tests (formats, exit codes, edge cases, resilience, doctests) and planned their implementation?
8.  **Infrastructure Reuse**: Have you identified which existing shared helpers, loaders, or traits you MUST use instead of reimplementing?
9.  **Nextest Configuration**: For new integration tests, have you planned which test group to use and how to configure it in all `nextest.toml` profiles?

## 3. Development Workflow & Quality Gates

### A. Build, Format, and Lint
- **Build**: `cargo build --quiet`
- **Format**: `cargo fmt --all && cargo fmt --all -- --check` (no trailing whitespace)
- **Lint**: `cargo clippy --all-targets -- -D warnings` (must have zero warnings)

### B. Smart Testing Strategy

**MANDATORY**: Use `make test-nextest-*` targets EXCLUSIVELY for all test execution. Do not use raw `cargo test`.

**Decision Tree (Choose the SMALLEST relevant test target):**
```
What did you change?
├─ Formatting / minor refactor → make test-nextest-fast
├─ Core logic / parsing → make test-nextest-unit
├─ Docker / runtime integration → make test-nextest-docker
├─ Smoke tests needed → make test-nextest-smoke
├─ Feature with broad impact → make test-nextest-fast (quick check) then make test-nextest (full)
└─ Before PR submission → make test-nextest
```

**Available `make` targets:**
- `make test-nextest-fast`: Fast parallel subset (unit/bins/examples + doctests; excludes smoke/parity/docker). **USE THIS BY DEFAULT.**
- `make test-nextest-unit`: Only unit tests.
- `make test-nextest-docker`: Only Docker integration tests.
- `make test-nextest-smoke`: Only smoke tests.
- `make test-nextest-long-running`: Long-running integration tests.
- `make test-nextest`: Full parallel suite (run before PRs).
- `make test-nextest-ci`: CI profile with conservative settings.

### C. Nextest Configuration for New Tests

**ALL new integration tests MUST be configured in `.config/nextest.toml`**.
1.  **Identify Resource Requirements** and choose a `test-group`: `docker-exclusive`, `docker-shared`, `fs-heavy`, `long-running`, `smoke`, `parity`.
2.  **Add Override Rules** to ALL profiles (`default`, `dev-fast`, `full`, `ci`).
3.  **Maximize Safe Parallelism**: Prefer `docker-shared` over `docker-exclusive`. Split test binaries by resource type.
4.  **Verify** by running `make test-nextest`.

### D. The "Keep the Build Green" Mandate
- **Fix, Don't Skip**: When tests fail, they MUST be fixed—not disabled or ignored. Work MUST STOP until the issue is resolved.
- After each change batch, run `fmt`, `clippy`, and the appropriate `make test-nextest-*` command.

## 4. Coding Standards & Architectural Principles

- **Idiomatic, Safe Rust**: Use modern, idiomatic Rust (2021 Edition). No `unsafe` without full justification.
- **Subcommand Consistency & Shared Abstractions**: All CLI subcommands MUST share canonical helpers for common behaviors (terminal sizing, config resolution, container targeting, etc.). DO NOT hand-implement these per subcommand. If a helper is missing, create it and reuse it.
- **Error Handling**: Use `thiserror` for domain errors in core libraries. Use `anyhow` only at the binary boundary with meaningful context (`.context()`). Never swallow errors with `.unwrap()` or `.expect()`; always propagate with `Result`.
- **Dependencies**: Keep dependencies current. Migrate from deprecated crates promptly (e.g., `atty` → `is-terminal`).
- **Logging**: Use `tracing` with structured fields and spans aligned to workflows (`config.resolve`, `feature.install`).
- **Imports**: Order: `std`, then external crates, then local modules (`crate`/`super`). Let `rustfmt` organize them.
- **Naming**: modules/files `snake_case`; types/traits `CamelCase`; functions/vars `snake_case`.

## 5. Output Contracts and Hygiene

- **Stdout/Stderr Separation**: For JSON output (`--output json`), stdout contains ONLY the JSON document; all logs go to stderr. For text output, stdout has human-readable results; logs go to stderr.
- **Schema and Ordering Compliance**: JSON output MUST conform exactly to the spec-defined schema. If the spec requires declaration order, use `Vec` or `IndexMap`, not `BTreeMap`.
- **Exit Code Contracts**: Honor spec-defined exit codes (e.g., exit 2 for "outdated detected") in ALL output modes.
- **Fixture and Example Hygiene**: Pin all external dependencies in fixtures/examples to specific versions (e.g., `alpine:3.18`), not `latest`.

## 6. Common Anti-Patterns to Avoid

- **Data Structure Mismatch**: Using `Vec` when spec says `map`, or using `BTreeMap` for JSON when spec requires declaration order.
- **Incomplete Config Resolution**: Loading only the top-level config file and ignoring `extends` or `overrides`.
- **Silent Fallbacks**: Passing invalid inputs downstream instead of filtering them at the boundary.
- **Exit Code Gating**: Honoring special exit codes in only one output mode.
- **Error Swallowing**: Using `.unwrap()` or returning sentinel values instead of propagating `Result` types.
- **Test Gaps**: Not implementing all tests mandated by the spec.
- **Missing Nextest Config**: Adding integration tests without configuring them in `nextest.toml`, leading to slow or flaky CI.
- **Non-Reproducible Fixtures**: Using `latest` tags for images or dependencies.
