# Quickstart: Workspace Mount Consistency and Git-Root Handling

1) Read the spec `specs/006-align-workspace-mounts/spec.md`, research `research.md`, and data model `data-model.md` to confirm workspace discovery, consistency propagation, and git-root handling expectations.
2) Update workspace discovery and mount rendering in `crates/core` / `crates/deacon` to:
   - Apply the provided consistency to every default workspace mount (Docker + Compose).
   - Use git root when the git-root flag is set; fall back to workspace root with an explicit note otherwise.
   - Keep Docker and Compose mount generation in parity for host path and consistency.
3) Add/adjust tests:
   - Unit tests for path selection (workspace vs git root) and consistency propagation.
   - Integration/CLI-render tests if Docker vs Compose formatting paths differ.
   - Configure any new integration binaries in `.config/nextest.toml` with correct test groups.
4) Validation cadence:
   - `cargo fmt --all && cargo fmt --all -- --check`
   - `cargo clippy --all-targets -- -D warnings`
   - `make test-nextest-unit` (logic) and `make test-nextest-fast` (broader)
   - `make test-nextest` before PR
5) Ensure fallback messaging is surfaced without silent divergence and that stdout/stderr contracts remain intact.

---

## Test Coverage

The feature implementation includes **38 unit tests** in `crates/deacon/tests/workspace_mounts.rs` covering:

### Test Modules

- **docker_consistency_tests**: Docker workspace mount consistency propagation (6 tests)
- **compose_consistency_tests**: Compose workspace mount consistency propagation (4 tests)
- **cli_consistency_propagation_tests**: CLI argument consistency format tests (2 tests)
- **git_root_docker_mount_tests**: Docker git-root path selection (5 tests)
- **git_root_with_consistency_tests**: Git-root combined with consistency (3 tests)
- **default_workspace_discovery_tests**: Default behavior preservation (4 tests)
- **compose_git_root_tests**: Compose git-root path selection (4 tests)
- **compose_git_root_consistency_tests**: Compose git-root with consistency (6 tests)
- **performance_tests**: Timing validation for workspace discovery and mount rendering (4 tests)

### Running Tests

```bash
# Run all workspace mount tests (fastest feedback)
cargo nextest run -E 'binary(workspace_mounts)'

# Run specific test module
cargo nextest run -E 'binary(workspace_mounts) and test(docker_consistency)'
cargo nextest run -E 'binary(workspace_mounts) and test(compose_git_root)'

# Run with verbose output
cargo nextest run -E 'binary(workspace_mounts)' --no-capture

# Include in broader test runs
make test-nextest-fast    # Includes workspace_mounts tests (unit-level, no Docker)
make test-nextest         # Full suite before PR
```

### Test Characteristics

- **Unit-level tests**: No Docker daemon required
- **Fast execution**: All tests use in-memory fixtures and tempfile directories
- **Deterministic**: No network or external dependencies
- **Performance target**: Workspace discovery + mount rendering < 200ms
