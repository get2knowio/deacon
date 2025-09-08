# Contributing to deacon

Welcome! This guide covers the basics for working on the Rust codebase.

## Prerequisites
- Rust toolchain (install via https://rustup.rs)
- Git
- (Optional) `cargo-edit` for dependency upgrades: `cargo install cargo-edit`

## Quick Start
```bash
# Clone and enter
git clone https://github.com/get2knowio/deacon.git
cd deacon

# Build all crates
cargo build

# Run the binary
cargo run -- hello

# Run all tests
cargo test

# Run only integration tests
cargo test --test integration_hello

# Run benches (Criterion)
cargo bench
```

## Common Tasks
| Task | Command |
|------|---------|
| Format code | `cargo fmt --all` |
| Lint (clippy) | `cargo clippy --all-targets -- -D warnings` |
| Update lockfile | `cargo update` |
| Upgrade dependencies (edit Cargo.toml) | `cargo upgrade --workspace` (requires cargo-edit) |
| Clean build artifacts | `cargo clean` |

## Workspace Layout
```
crates/
  deacon/          # CLI binary crate (current implementation)
```
(Additional crates like `core` will be added as functionality grows.)

## Adding Dependencies
Add to root workspace if shared:
```bash
cargo add crate_name --workspace
```
Or only to the CLI crate:
```bash
cargo add --manifest-path crates/deacon/Cargo.toml crate_name
```

## Testing Notes
- Use `assert_cmd` for integration tests that spawn the binary.
- Keep individual tests fast (< 1s) to maintain quick feedback.
- Benchmarks live under `crates/deacon/benches/` and use Criterion.

## Logging & Debugging
Enable more verbose logs:
```bash
RUST_LOG=debug cargo run -- hello
```

## Coding Style
- Follow `rustfmt` defaults.
- For new modules keep functions small and focused; prefer `?` for error propagation.
- Avoid `unsafe` (workspace forbids it currently).

## Submitting Changes
1. Fork the repo.
2. Create a feature branch: `git checkout -b feature/short-description`.
3. Make changes; run format & clippy:  
   `cargo fmt --all && cargo clippy --all-targets -- -D warnings`.
4. Ensure tests pass: `cargo test`.
5. Commit with a clear message (consider Conventional Commits style).  
6. Open a Pull Request describing the change and referencing any issue numbers.

## Release Process (Planned)
A GitHub Actions release workflow will build multi-platform binaries when a tag like `v0.x.y` is pushed.

## Getting Help
Open an issue or start a discussion if uncertain about direction or architecture.

Thanks for contributing!
