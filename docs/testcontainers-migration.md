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
2. `crates/deacon/tests/parity_utils.rs` - Parity test utilities
3. `crates/deacon/tests/smoke_*.rs` - Smoke tests (6 files)
4. `crates/deacon/tests/up_*.rs` - Up command tests (7 files)
5. `crates/deacon/tests/integration_exec_*.rs` - Exec tests
6. `crates/deacon/tests/integration_build_args.rs` - Build tests
7. `crates/deacon/tests/integration_custom_container_name.rs` - Container naming
8. `crates/deacon/tests/parity_*.rs` - Parity tests (2 files)

### Phase 1: Foundation
- [ ] Add `testcontainers` and `testcontainers-modules` dev-dependencies
- [ ] Create shared testcontainers helpers module
- [ ] Update nextest config for improved parallelization

### Phase 2: Core Test Migration
- [ ] Migrate `test_utils.rs` (DeaconGuard pattern)
- [ ] Migrate smoke tests
- [ ] Migrate up command tests
- [ ] Migrate exec tests

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

## Reference

- [testcontainers-rs docs](https://rust.testcontainers.org/)
- [Issue #436](https://github.com/anthropics/deacon/issues/436)
