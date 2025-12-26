# Quickstart - Up Lifecycle Semantics Compliance

## Prerequisites
- Rust stable toolchain (Edition 2021) with rustfmt and clippy installed.
- Workspace branch: `008-up-lifecycle-hooks`.
- Familiarity with lifecycle code paths in `crates/core` and CLI wiring in `crates/deacon`.

## Read This First
- Spec: `/workspaces/deacon/specs/008-up-lifecycle-hooks/spec.md`
- Research decisions: `/workspaces/deacon/specs/008-up-lifecycle-hooks/research.md`
- Constitution: `/workspaces/deacon/.specify/memory/constitution.md`

## Key Code Touchpoints
- Lifecycle ordering and markers: `crates/core/src/lifecycle.rs`, `crates/core/src/container_lifecycle.rs`, `crates/core/src/state.rs`
- Dotfiles handling: `crates/core/src/dotfiles.rs`
- CLI `up` orchestration and flags: `crates/deacon/src/commands/up/` (module directory, esp. `mod.rs`, `args.rs`, `lifecycle.rs`, `dotfiles.rs`) and `crates/deacon/src/commands/shared`
- Summary/output handling: `crates/deacon/src/ui` and related renderers
- Fixtures/tests: `crates/deacon/tests/` (e.g., `up_prebuild.rs`, `up_dotfiles.rs`, `smoke_lifecycle.rs`)

## Development Steps
1. Align implementation with the spec ordering: onCreate → updateContent → postCreate → dotfiles → postStart → postAttach.
2. Ensure resume uses per-phase markers; rerun earliest incomplete phase if markers are missing/corrupted.
3. Implement `--skip-post-create` behavior to bypass postCreate/postStart/postAttach and dotfiles while still updating content.
4. Implement prebuild mode to stop after updateContent, skip dotfiles/post* hooks, and isolate markers so normal `up` reruns onCreate/updateContent.
5. Surface a summary of executed/skipped phases (including dotfiles) in lifecycle order with reasons.
6. Add/adjust tests and fixtures to cover ordering, resume, skip flag, prebuild reruns, and dotfiles sequencing.

## Test & Check Cadence
- Formatting/lints: `cargo fmt --all && cargo fmt --all -- --check`, `cargo clippy --all-targets -- -D warnings`
- Fast validation: `make test-nextest-fast`
- Targeted lifecycle coverage: `make test-nextest-unit` for logic and `make test-nextest-docker` for `up`/prebuild/dotfiles integration paths
- Run `make test-nextest` before PR if changes touch broad lifecycle execution paths.

## Notes
- If any work is deferred, document decisions in `research.md` and add tasks under "Deferred Work" in `tasks.md`.
- Keep stdout/stderr contracts intact, especially for JSON modes.
