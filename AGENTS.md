# AGENTS: Quick Guide for AI Assistants
- Source of truth: follow the `docs/subcommand-specs/*/SPEC.md` files; no silent fallbacks—fail fast if unimplemented.
- **CRITICAL PRE-IMPLEMENTATION**: Read COMPLETE spec (SPEC.md + data-model.md + contracts/) BEFORE coding; verify data structures match spec shapes exactly (map vs vec, null handling, ordering); identify all spec-defined algorithms to implement precisely.
- Build: `cargo build --quiet`; Run CLI: `cargo run -- --help`.
- Fast loop (DEFAULT): `make test-nextest-fast` (unit/bin/examples + doctests; excludes smoke/parity/docker)
- Targeted test commands (USE THESE EXCLUSIVELY):
	- Fast parallel subset → `make test-nextest-fast`
	- Unit tests only → `make test-nextest-unit`
	- Docker integration → `make test-nextest-docker`
	- Smoke tests → `make test-nextest-smoke`
	- Long-running tests → `make test-nextest-long-running`
	- Full parallel suite → `make test-nextest` (before PR)
	- CI conservative profile → `make test-nextest-ci`
- Lint: `cargo clippy --all-targets -- -D warnings` (zero warnings).
- Format: `cargo fmt --all` && `cargo fmt --all -- --check` (no trailing whitespace).
- **Critical Principle (Scope‑Aligned Validation)**: After each change batch run fmt + clippy; choose the *smallest* relevant test target (see decision tree) instead of full suite spam.
- Features: core: `json-logs`; deacon: `docker` (default), `config`.
- Imports: std, external crates, then local (`crate`/`super`); let rustfmt organize.
- Naming: modules/files `snake_case`; types/traits `CamelCase`; fns/vars `snake_case`.
- Types: prefer explicit public types; avoid unnecessary clones; use `&str` over `String` when borrowing.
- Errors: use `thiserror` enums in core; `anyhow` only at binary boundaries; add context with `anyhow::Context`. Never swallow errors—propagate with `Result`; avoid unwraps without `.expect("context")`.
- Logging: use `tracing` spans/fields; honor `RUST_LOG` and `DEACON_LOG`.
- Tests: deterministic and hermetic (no network); use `fixtures/` and `assert_cmd` for CLI. **CRITICAL**: Implement ALL spec-mandated tests (output formats, exit codes, edge cases, resilience).
- Commits/PRs: Conventional Commits; keep build green locally after every change.
- Safety: no `unsafe` code; review new deps carefully. Migrate deprecated deps promptly (e.g., `atty` → `is-terminal`).
- Shared helpers: whenever multiple subcommands expose the same flag or behavior (terminal sizing, config/override/secrets resolution, container targeting, remote env merging, compose env-file wiring, env probing, etc.), reuse the canonical helper. If no helper exists, create one and record the debt in the shared alignment log before extending individual subcommands.
- Examples: every `examples/` directory MUST include an `exec.sh` that runs **all** README-documented paths in one non-interactive pass, echoes scenario banners, and cleans up containers/images/volumes it creates. Keep README and `exec.sh` in lockstep; update the subcommand-level aggregator scripts (e.g., `examples/up/exec.sh`) whenever examples are added/changed.
- Copilot rules: follow `.github/copilot-instructions.md` (run build/test/fmt/clippy after every change).
- Use `make test-nextest-fast` by default during spec-phase; run `make test-nextest` before PRs.

## Smart Testing Strategy & Decision Tree

**MANDATORY**: Use `make test-nextest-*` targets EXCLUSIVELY for all test execution. These provide optimal parallelization, timing artifacts, and consistent configuration.

Decision Tree:
```
What did you change?
├─ Formatting / minor refactor → make test-nextest-fast
├─ Core logic / parsing → make test-nextest-unit
├─ Docker / runtime integration → make test-nextest-docker
├─ Smoke tests needed → make test-nextest-smoke
├─ Feature with broad impact → make test-nextest-fast (quick check) then make test-nextest (full)
└─ Before PR submission → make test-nextest
```

Guidelines:
- ALWAYS use nextest targets for test execution (faster feedback, timing artifacts under `artifacts/nextest/`).
- Never use raw `cargo test` commands during development; use `make test-nextest-*` targets instead.
- Avoid repeated full-suite runs (`make test-nextest`) during active editing.
- When both parsing and lifecycle touched: run `make test-nextest-unit` then `make test-nextest-docker`.
- Update/add tests when outputs, algorithms, or public behavior change.
- Anti-pattern: running all tests after every keystroke; running raw cargo test instead of nextest.

## Nextest Configuration Requirements (MANDATORY for New Tests)

**ALL new integration tests MUST be configured in `.config/nextest.toml`** with appropriate test groups. When adding a new test binary or test suite:

1. **Identify Resource Requirements**:
   - Docker exclusive (shared container state, lifecycle conflicts) → `docker-exclusive`
   - Docker shared (safe concurrent daemon access) → `docker-shared`
   - Filesystem heavy (significant I/O) → `fs-heavy`
   - Long-running (>30s execution) → `long-running`
   - Smoke tests (high-level E2E) → `smoke`
   - Parity tests (upstream comparison) → `parity`

2. **Add Override Rules to ALL Profiles** (default, dev-fast, full, ci):
   ```toml
   [[profile.*.overrides]]
   filter = 'binary(=your_test_binary)'
   test-group = 'docker-exclusive'  # or appropriate group
   ```

3. **Maximize Safe Parallelism**:
   - Prefer `docker-shared` over `docker-exclusive` when tests can safely share Docker daemon
   - Prefer parallel execution over serial when no state conflicts exist
   - Split test binaries by resource type (e.g., `integration_docker_*` vs `integration_fs_*`) for finer control

4. **Verify Configuration**:
   - Run `make test-nextest` to ensure tests pass with parallel execution
   - Check for race conditions or "No such container" errors indicating insufficient isolation
   - Document parallelization constraints in test module comments when non-obvious

**Goal**: Maximize test throughput while maintaining determinism and avoiding flaky tests.

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
9. **Nextest Configuration**: Have you planned which test group each new integration test will use and added overrides to all profiles?

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
- **Missing Nextest Config**: Adding new integration tests without configuring them in `.config/nextest.toml` with appropriate test groups.
- **Suboptimal Test Grouping**: Using `docker-exclusive` when `docker-shared` would work; causing unnecessary serialization and slower CI.

## Active Technologies
- Rust stable (2021 edition) + clap, serde/serde_json, anyhow/thiserror, tracing, tokio (as already in repo) (001-up-gap-spec)
- cargo-nextest for parallel test execution with resource-aware scheduling
- N/A (CLI orchestrator; uses filesystem for configs/cache) (001-up-gap-spec)
- Rust (2021 edition; workspace pinned via rust-toolchain) + clap, serde/serde_json, tracing, anyhow/thiserror, tokio, existing exec/TTY helpers in crates/core and crates/deacon (001-force-pty-up)
- N/A (CLI runtime only) (001-force-pty-up)
- Rust (stable, 2021 edition; rust-toolchain pins stable) + clap, serde/serde_json, anyhow/thiserror, tracing, tokio, compose/exec helpers in crates/core and crates/deacon (001-compose-mount-env)
- N/A (compose config files and runtime Docker resources) (001-compose-mount-env)
- Rust (stable, Edition 2021) + clap, serde/serde_json, anyhow/thiserror, tracing, tokio; local crates `core` and `deacon` (006-align-workspace-mounts)
- N/A (filesystem discovery only) (006-align-workspace-mounts)
- Rust stable (2021 edition per workspace toolchain) + clap, serde/serde_json, anyhow/thiserror, tracing, tokio, cargo-nextest for tests; reuse existing config/feature/build helpers in `crates/core` and CLI wiring in `crates/deacon` (007-up-build-parity)
- Filesystem-based config/lockfile inputs and merged outputs; no persistent DB (007-up-build-parity)
- Rust stable (Edition 2021) + clap, serde/serde_json, tracing, anyhow/thiserror, tokio, internal lifecycle helpers in `crates/core` and `crates/deacon` (008-up-lifecycle-hooks)
- None (filesystem state/markers only) (008-up-lifecycle-hooks)

## Recent Changes
- 001-up-gap-spec: Added Rust stable (2021 edition) + clap, serde/serde_json, anyhow/thiserror, tracing, tokio (as already in repo)
- 2025-11-19: Added pre-implementation checklist and common anti-patterns based on outdated subcommand implementation review
- 2025-11-20: Standardized on `make test-nextest-*` targets exclusively; added mandatory nextest configuration requirements; documented test parallelization strategy for optimal resource utilization
