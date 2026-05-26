# Contributing to deacon

Welcome! This guide covers the development workflow for the Rust DevContainer CLI implementation.

## Prerequisites
- Rust toolchain (stable) - install via https://rustup.rs
- Git
- Docker (for eventual container integration testing)

## Quick Start
```bash
# Fork and clone the repository
git clone https://github.com/YOUR_USERNAME/deacon.git
cd deacon

# Build all crates
cargo build

# Run the CLI
cargo run -- --help
cargo run -- --version
cargo run -- up                # start a devcontainer in the current dir
cargo run -- read-configuration

# Fast development loop: fmt + clippy + fast tests (recommended during iteration)
make dev-fast

# Run the fast test suite via cargo-nextest (excludes docker/smoke tests)
make test-nextest-fast

# Format and lint
cargo fmt --all
cargo clippy --all-targets -- -D warnings
```

`cargo-nextest` is the standard test runner — install it once with
`cargo install cargo-nextest --locked`. The `make` targets shell out to
`cargo nextest run` with the right profile, parallelism, and grouping
(see `.config/nextest.toml`).

## Development Workflow
1. **Fork the repository** on GitHub
2. **Clone your fork locally**
3. **Create a feature branch**: `git checkout -b feature/your-feature`
4. **Make changes** following the coding guidelines below
5. **Test your changes**: `cargo test` and manual testing
6. **Format and lint**: `cargo fmt --all && cargo clippy --all-targets -- -D warnings`
7. **Commit with clear messages** (preferably following [Conventional Commits](https://conventionalcommits.org/))
8. **Push to your fork**: `git push origin feature/your-feature`
9. **Open a Pull Request** with a clear description

## Project Structure
```
crates/
  deacon/          # CLI binary crate (main entrypoint)
  core/            # Shared library crate (config, docker, features, etc.)
docs/
  subcommand-specs/*/SPEC.md      # Authoritative specification
.github/
  workflows/       # CI and release automation
```

## Common Tasks
| Task | Command |
|------|---------|
| Build | `cargo build` |
| Build (release) | `cargo build --release` |
| Fast dev loop (fmt + clippy + fast tests) | `make dev-fast` |
| Fast tests (unit + bins + examples + doctests; excludes docker/smoke) | `make test-nextest-fast` |
| Unit tests only (super fast) | `make test-nextest-unit` |
| Docker integration tests | `make test-nextest-docker` |
| High-level smoke tests | `make test-nextest-smoke` |
| Full parallel suite (before PR) | `make test-nextest` |
| Full quality gate (fmt + clippy + test + build) | `make release-check` |
| View test group assignments | `make test-nextest-audit` |
| Format code | `cargo fmt --all` |
| Lint (clippy) | `cargo clippy --all-targets -- -D warnings` |
| Update dependencies | `cargo update` |
| Clean build | `cargo clean` |
| Coverage report | `make coverage` |

Test groups live in `.config/nextest.toml` (`docker-exclusive`,
`docker-shared`, `fs-heavy`, `long-running`, `smoke`, `parity`). When you
add an integration test that touches Docker or filesystem heavily, add a
group override to all profiles in that file so the suite stays
parallel-safe.

## Adding Dependencies
Add to workspace root for shared dependencies:
```bash
cargo add <crate_name> --workspace
```

Add to specific crate:
```bash
cargo add --manifest-path crates/<crate>/Cargo.toml <crate_name>
```

## Testing Guidelines
- **Unit tests**: Test individual functions and modules
- **Integration tests**: Use `assert_cmd` to test the CLI binary
- **End-to-end tests**: Comprehensive scenarios validating complete workflows
- **Test coverage**: Aim for good coverage of new functionality
- **Performance**: Keep tests fast (< 2s) for quick feedback, e2e tests < 30s total
- **Deterministic**: Tests should not depend on external networks or random data

### Running specific tests

```bash
# Run a single test by name (cargo-nextest, parallel)
cargo nextest run test_name

# Run a test pattern across the workspace
cargo nextest run 'test(integration_)*'

# Traditional cargo test still works if you need it (serial)
cargo test test_name -- --test-threads=1
```

### E2E + integration tests

End-to-end tests live alongside the unit tests in each crate's `tests/`
directory and run as part of `make test-nextest`. They are deterministic
and hermetic by default (no network); tests that need Docker are gated
into the `docker-shared` or `docker-exclusive` nextest groups so they're
opted out of the fast loop.

## Coding Standards
- **Follow `rustfmt` defaults** - run `cargo fmt --all` before committing
- **Use `clippy` lints** - fix all warnings flagged by `cargo clippy`
- **No `unsafe` code** - the workspace forbids unsafe blocks
- **Error handling**: Use `anyhow::Result` at binary boundaries, domain errors for libraries
- **Logging**: Use `tracing` for structured logging with appropriate levels
- **Documentation**: Add rustdoc comments for public APIs

## Architecture Guidelines
- **Follow the CLI specification** in `docs/subcommand-specs/*/SPEC.md` as the source of truth
- **Small, incremental changes** - avoid large refactors in single PRs
- **Domain separation**: Keep CLI concerns in `crates/deacon`, shared logic in `crates/core`
- **Trait abstractions**: Use traits for testability (Docker client, file system, etc.)

## Debugging
Enable verbose logging:
```bash
RUST_LOG=debug cargo run -- --help
```

## CI/CD
- **GitHub Actions** runs on every PR and push to main:
  - **Lint** (`.github/workflows/ci.yml`): rustfmt check + clippy (zero warnings)
  - **Test (MVP fast)** on Ubuntu + macOS: `make test-nextest-fast`
  - **Test (MVP integration)** on Ubuntu: full integration suite
  - **Security (cargo-deny)**: advisories + bans + licenses + sources;
    also runs on a daily schedule
  - **CodeQL** (`.github/workflows/codeql.yml`): security scanning, PR +
    weekly schedule
  - **Validate PR title**: enforces Conventional Commits
- **Release builds** trigger on version tags (`v*.*.*`) and produce
  SLSA-attested artifacts (see `.github/workflows/release.yml`).
- **Format and clippy must pass** for merge — no exceptions.

### Running macOS/Windows CI manually
To validate on other operating systems without gating PRs:
1. Navigate to the GitHub repository → Actions tab
2. Select the workflow named "CI (Other OS)"
3. Click "Run workflow" and optionally choose a branch
4. Observe jobs for `macos-13` and `windows-latest`

Notes:
- macOS uses Colima to provide a Docker runtime for smoke tests
- Windows smoke tests are allowed to continue on error to avoid flakiness when Docker is unavailable; unit/integration tests still run and must pass

## Release Process
1. Update version in `Cargo.toml` files
2. Update `CHANGELOG.md` (when created)
3. Create and push a git tag: `git tag v0.1.2 && git push origin v0.1.2`
4. GitHub Actions will automatically build and release binaries

### Production (Distribution) Build Guidance

All deacon binaries are built with full functionality enabled by default. Use this simple build command for any distribution:

```bash
cargo build --release
```

All capabilities are always available:
- Docker integration and container lifecycle operations
- Configuration format support (including TOML)
- Plugin system scaffolding (experimental)
- JSON logging (via `DEACON_LOG_FORMAT=json` environment variable)

## Getting Help
- **Issues**: Open a GitHub issue for bugs or feature requests
- **Discussions**: Use GitHub Discussions for questions or ideas
- **Specification**: Refer to `docs/subcommand-specs/*/SPEC.md` for architecture decisions

## Code of Conduct
Be respectful, constructive, and collaborative in all interactions.

Thanks for contributing to deacon!
