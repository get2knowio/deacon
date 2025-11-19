# AGENTS: Quick Guide for AI Assistants
- Source of truth: follow the `docs/subcommand-specs/*/SPEC.md` files; no silent fallbacks—fail fast if unimplemented.
- **CRITICAL PRE-IMPLEMENTATION**: Read COMPLETE spec (SPEC.md + data-model.md + contracts/) BEFORE coding; verify data structures match spec shapes exactly (map vs vec, null handling, ordering); identify all spec-defined algorithms to implement precisely.
- Build: `cargo build --quiet`; Run CLI: `cargo run -- --help`.
- Test (all): `cargo test --quiet -- --test-threads=1`; doctests: `cargo test --doc`.
- Fast loop: `make dev-fast` (fmt-check + clippy + unit/bins/examples + doctests; skips slow integration/smoke)
- Test (crate): `cargo test --quiet -p deacon`; `cargo test --quiet -p deacon-core`.
- Test (single unit): `cargo test --quiet -p deacon <name_substring>`.
- Test (single integration): `cargo test --quiet -p deacon --test integration_build_args <test_name>`.
- Lint: `cargo clippy --all-targets -- -D warnings` (zero warnings).
- Format: `cargo fmt --all` && `cargo fmt --all -- --check` (no trailing whitespace).
- **CRITICAL CI**: Run after EVERY change: build, test, fmt, clippy. Keep build green locally.
- Features: core: `json-logs`; deacon: `docker` (default), `config`.
- Imports: std, external crates, then local (`crate`/`super`); let rustfmt organize.
- Naming: modules/files `snake_case`; types/traits `CamelCase`; fns/vars `snake_case`.
- Types: prefer explicit public types; avoid unnecessary clones; use `&str` over `String` when borrowing.
- Errors: use `thiserror` enums in core; `anyhow` only at binary boundaries; add context with `anyhow::Context`. Never swallow errors—propagate with `Result`; avoid unwraps without `.expect("context")`.
- Logging: use `tracing` spans/fields; honor `RUST_LOG` and `DEACON_LOG`.
- Tests: deterministic and hermetic (no network); use `fixtures/` and `assert_cmd` for CLI. **CRITICAL**: Implement ALL spec-mandated tests (output formats, exit codes, edge cases, resilience).
- Commits/PRs: Conventional Commits; keep build green locally after every change.
- Safety: no `unsafe` code; review new deps carefully. Migrate deprecated deps promptly (e.g., `atty` → `is-terminal`).
- Copilot rules: follow `.github/copilot-instructions.md` (run build/test/fmt/clippy after every change).
- Use Fast Loop by default during spec-phase; run `make test`/`make release-check` periodically and before PRs.

## Spec Alignment Checklist (Before Implementation)
Run this checklist BEFORE writing implementation code for any new subcommand/feature:
1. **Full Spec Read**: Have you read SPEC.md, data-model.md, and contracts/ completely?
2. **Data Model Match**: Do your structs match spec shapes exactly? Check: map vs vec, field names, null semantics, ordering (declaration order vs alphabetical).
3. **Algorithm Fidelity**: Have you identified all spec-defined algorithms (e.g., version derivation, extends resolution, tag selection)? Will you implement them step-by-step as specified?
4. **Input Validation**: Which inputs are valid/supported per spec (e.g., only OCI refs, only semver tags)? Where will you filter invalid entries?
5. **Config Resolution**: If your command reads config, will you use `ConfigLoader::load_with_extends` (not just top-level load) to honor extends chains and overrides?
6. **Output Contracts**: Have you verified JSON schema (key names, field presence, null handling) and ordering requirements? Do exit codes match spec for all output modes?
7. **Test Coverage**: Have you listed all spec-mandated tests (formats, exit codes, edge cases, resilience, doctests) and planned to implement them?
8. **Infrastructure Reuse**: Are you using existing helpers (e.g., `ConfigLoader`, error types, OCI parsers) instead of reimplementing?

Document this checklist in your plan.md or PR description to prevent spec drift.

## Common Anti-Patterns to Avoid (from fixes.md analysis)
- **Data Structure Mismatch**: Using `Vec` when spec defines `map<string, T>`; skipping null serialization when spec requires all fields present.
- **Incomplete Resolution**: Loading top-level config only; ignoring extends/override chains required by spec.
- **Silent Fallbacks**: Passing invalid inputs (e.g., local feature paths) to downstream logic instead of filtering at ingress per spec.
- **Ordering Violations**: Using `BTreeMap` for JSON output when spec requires declaration order; use `Vec` or `IndexMap` instead.
- **Exit Code Gating**: Only honoring special exit codes (e.g., --fail-on-outdated) in one output mode; spec applies to all modes unless stated otherwise.
- **Error Swallowing**: Returning sentinel values or unwrapping without context; always propagate with `Result` and `.context()`.
- **Test Gaps**: Implementing features without spec-mandated tests; treating test suites as optional when they're acceptance criteria.
- **Deprecated Dependencies**: Continuing to use deprecated crates (e.g., `atty`) when replacements are available (`is-terminal`).
- **Non-Reproducible Fixtures**: Using `latest` tags in examples/fixtures; always pin versions (e.g., `alpine:3.18`).
- **Incomplete Test Setup**: Integration tests missing flags/fields implied by test name (e.g., `test_ignore_host_requirements` without `ignore_host_requirements: true`).

## Active Technologies
- Rust stable (2021 edition) + clap, serde/serde_json, anyhow/thiserror, tracing, tokio (as already in repo) (001-up-gap-spec)
- N/A (CLI orchestrator; uses filesystem for configs/cache) (001-up-gap-spec)

## Recent Changes
- 001-up-gap-spec: Added Rust stable (2021 edition) + clap, serde/serde_json, anyhow/thiserror, tracing, tokio (as already in repo)
- 2025-11-19: Added pre-implementation checklist and common anti-patterns based on outdated subcommand implementation review
