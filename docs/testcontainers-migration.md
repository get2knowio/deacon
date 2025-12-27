# Testcontainers Migration Plan

Tracking document for issue #436: Adopt testcontainers for integration tests.

## Overview

Replace manual container lifecycle management with [testcontainers-rs](https://rust.testcontainers.org/) for:
- Automatic container cleanup via RAII (`Drop` trait)
- Random port allocation enabling parallel test execution
- Elimination of leaked containers on test failure

## Files Requiring Migration

### High Priority (Direct Docker Commands)

Tests using `Command::new("docker")` or `docker run`:

1. `crates/deacon/tests/test_utils.rs` - Core cleanup utilities (DeaconGuard)
2. `crates/deacon/tests/up_*.rs` - Up command tests (7 files)
3. `crates/deacon/tests/integration_exec_selection.rs` - Uses `docker run` for test containers
4. `crates/deacon/tests/integration_exec_id_label.rs` - Uses `docker run` for test containers
5. `crates/deacon/tests/integration_build_args.rs` - Build tests
6. `crates/deacon/tests/integration_custom_container_name.rs` - Container naming

### Not Applicable (Parity Tests)

Parity tests compare `deacon` behavior to the upstream `devcontainer` CLI. These do NOT
need testcontainers migration because:

1. **`parity_utils.rs`**: Contains only utility functions. The only Docker command is
   `docker version` to check Docker availability.

2. **`parity_build.rs`**: Tests the CLI's `build` command. Uses Docker commands only for
   image inspection (`docker images`, `docker inspect`) and cleanup (`docker rmi`).
   Testcontainers manages running containers, not image builds.

3. **`parity_up_exec.rs`**: Tests the CLI's `up` and `exec` commands. Containers are
   created by `deacon up` and `devcontainer up` (the CLIs being tested), not by direct
   `docker run` calls. Docker commands are used only for inspection (`docker ps`,
   `docker inspect`) to verify labels.

### Not Applicable (CLI-Managed Containers)

The following smoke tests do NOT need testcontainers migration because they test the
`deacon` CLI's container lifecycle management. Using testcontainers would bypass the
very functionality being tested:

- `crates/deacon/tests/smoke_basic.rs` - Tests `deacon build`, `deacon up`, `deacon exec`
- `crates/deacon/tests/smoke_lifecycle.rs` - Tests lifecycle hooks via `deacon up`
- `crates/deacon/tests/smoke_compose_edges.rs` - Tests compose-based `deacon up`
- `crates/deacon/tests/smoke_down.rs` - Tests `deacon down` command
- `crates/deacon/tests/smoke_exec.rs` - Tests `deacon exec` behavior
- `crates/deacon/tests/smoke_exec_stdin.rs` - Tests `deacon exec` stdin streaming
- `crates/deacon/tests/smoke_up_idempotent.rs` - Tests `deacon up` idempotency
- `crates/deacon/tests/smoke_spinner.rs` - Tests spinner output in `deacon up`/`deacon down`

These tests only use `docker info` for availability detection and rely on `DeaconGuard`
(which calls `deacon down`) for cleanup. The container lifecycle is managed by the
`deacon` CLI being tested, not by the tests themselves.

### Phase 1: Foundation
- [x] Add `testcontainers` and `testcontainers-modules` dev-dependencies
- [x] Create shared testcontainers helpers module
- [ ] Update nextest config for improved parallelization

### Phase 2: Core Test Migration
- [ ] Migrate `test_utils.rs` (DeaconGuard pattern)
- [x] ~~Migrate smoke tests~~ - NOT APPLICABLE (see above)
- [ ] Migrate up command tests
- [x] Migrate exec tests (`integration_exec_selection.rs`, `integration_exec_id_label.rs`)

### Phase 3: Cleanup & Validation
- [ ] Remove manual cleanup code
- [ ] Verify all tests pass
- [ ] Validate CI performance

## Dependencies to Add

```toml
[dev-dependencies]
testcontainers = "0.23"
testcontainers-modules = { version = "0.11", features = ["blocking"] }
```

## Development Tips

### Keeping Containers for Debugging

When debugging failing tests, you can prevent testcontainers from removing containers by setting:

```bash
TESTCONTAINERS_COMMAND=keep cargo nextest run test_name
```

This keeps the container running after the test completes, allowing you to inspect logs,
exec into the container, or debug issues interactively. Remember to manually clean up
containers afterward with `docker rm -f <container_id>`.

## Reference

- [testcontainers-rs docs](https://rust.testcontainers.org/)
- [Issue #436](https://github.com/anthropics/deacon/issues/436)
