# Repository Guidelines

## Project Structure & Module Organization
- `crates/deacon` — CLI binary crate (`main.rs`, CLI parsing in `cli.rs`).
- `crates/core` — shared library (config, docker, logging, features, templates, lifecycle).
- `crates/*/tests` — integration tests per crate; unit tests live near code.
- `docs/CLI-SPEC.md` — internal spec and architecture reference.
- `fixtures/` — sample devcontainer configs for tests.

## Build, Test, and Development Commands
- Build workspace: `cargo build` (add `--release` for optimized binaries).
- Run CLI: `cargo run -- --help` or `target/debug/deacon --help`.
- Test all crates: `cargo test`.
- Lint (deny warnings): `cargo clippy --all-targets -- -D warnings`.
- Format: `cargo fmt --all`.
- Coverage (optional): `cargo llvm-cov --workspace --open`.
- Features: core: `json-logs`; deacon: `docker` (default), `config`.
  Examples: `cargo test --no-default-features`, `cargo run --features json-logs` (core) or `--features config` (deacon).

## Coding Style & Naming Conventions
- Rust 2021 edition; `rustfmt` defaults; zero `unsafe` (forbidden at workspace level).
- Naming: modules/files `snake_case`; types/traits `CamelCase`; functions/vars `snake_case`.
- Errors: use domain errors in `crates/core::errors`; `anyhow::Result` at binary boundaries.
- Logging: use `tracing`; respect `RUST_LOG`/`DEACON_LOG` (see `crates/core/logging.rs`).

## Testing Guidelines
- Frameworks: built‑in Rust tests; integration tests use `assert_cmd`, `predicates`, `tempfile`.
- Locations: unit tests inline (`mod tests`); integration in `crates/*/tests/` (e.g., `integration_*.rs`).
- Run subset: `cargo test -p deacon` or `-p deacon-core`.
- Keep tests hermetic (no network); use fixtures from `fixtures/` when needed.

## Commit & Pull Request Guidelines
- Commits: prefer Conventional Commits (e.g., `feat: ...`, `fix: ...`, `test: ...`).
- PRs: include a clear description, linked issues, and rationale; ensure:
  - `cargo test` passes on all crates
  - `cargo fmt --all` has been run
  - `cargo clippy --all-targets -- -D warnings` is clean
  - Add/adjust tests for behavior changes

## Security & Configuration Tips
- No `unsafe` code allowed; review new deps carefully.
- Docker-related code is behind features; design for graceful absence of Docker.
- For debugging, enable logs: `RUST_LOG=debug cargo run -- ...`.

