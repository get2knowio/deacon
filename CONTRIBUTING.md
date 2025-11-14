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

# Run the CLI (currently shows placeholder)
cargo run -- --help
cargo run -- --version
cargo run

# Run all tests
cargo test

# Format and lint
cargo fmt --all
cargo clippy --all-targets -- -D warnings
```

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
| Build (full release feature set) | `cargo build --release` |
| Test | `cargo test` |
| Format code | `cargo fmt --all` |
| Lint (clippy) | `cargo clippy --all-targets -- -D warnings` |
| Update dependencies | `cargo update` |
| Clean build | `cargo clean` |

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

### Running End-to-End Tests
The e2e test suite validates the complete integration of:
- Configuration discovery and loading
- Variable substitution
- Feature parsing and handling
- Lifecycle command processing
- Plugin customization support
- Logging and error handling

```bash
# Run all e2e tests
cargo test --test integration_e2e --manifest-path crates/deacon/Cargo.toml

# Run specific e2e test scenarios
cargo test test_e2e_basic_config_read --manifest-path crates/deacon/Cargo.toml
cargo test test_e2e_variable_substitution --manifest-path crates/deacon/Cargo.toml
cargo test test_e2e_features_configuration --manifest-path crates/deacon/Cargo.toml
cargo test test_e2e_plugin_customizations --manifest-path crates/deacon/Cargo.toml
cargo test test_e2e_lifecycle_simulation --manifest-path crates/deacon/Cargo.toml
cargo test test_e2e_performance_under_30s --manifest-path crates/deacon/Cargo.toml
cargo test test_e2e_error_handling --manifest-path crates/deacon/Cargo.toml
```

The e2e tests are designed to run quickly (total runtime < 30 seconds) and validate:
1. **Basic config reading**: Configuration discovery, loading, and JSON output
2. **Variable substitution**: Replacement of workspace and environment variables
3. **Feature configuration**: Parsing of local and remote feature references
4. **Plugin customizations**: VSCode extensions and settings handling
5. **Lifecycle simulation**: Command processing with variable substitution
6. **Performance validation**: Ensuring operations complete within time limits
7. **Error handling**: Proper handling of missing files and invalid JSON

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
- **GitHub Actions** runs tests on every PR and push to main
- **Ubuntu checks on PR/merge**:
  - Lint: rustfmt check, cargo check, clippy, doctests
  - Test: `make test-nextest-fast` (parallel via cargo-nextest)
  - Smoke: `make test-smoke` (serial)
  - Coverage: cargo-llvm-cov with LCOV upload (threshold enforced via `MIN_COVERAGE`)
- **Nextest CI timing**: `make test-nextest-ci` produces `artifacts/nextest/ci-timing.json` for timing comparison
- **macOS/Windows checks (manual)**: Trigger the "CI (Other OS)" workflow via "Run workflow" in GitHub Actions to run macOS and Windows jobs on demand (macOS uses Colima for Docker)
- **Release builds** are automatically created on version tags (`v*.*.*`)
- **Format and clippy checks** must pass for PR approval

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
