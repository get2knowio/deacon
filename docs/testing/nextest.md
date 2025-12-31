# Test Parallelization with cargo-nextest

This guide explains how deacon uses [cargo-nextest](https://nexte.st/) to run tests in parallel, how tests are classified into groups, and how maintainers can categorize new tests appropriately.

## Table of Contents

- [Overview](#overview)
- [Quick Start](#quick-start)
- [Test Groups](#test-groups)
- [Execution Profiles](#execution-profiles)
- [Classification Workflow](#classification-workflow)
- [Auditing Test Assignments](#auditing-test-assignments)
- [Remediation Steps](#remediation-steps)
- [Troubleshooting](#troubleshooting)

## Overview

The deacon project uses cargo-nextest to achieve faster test feedback loops by running tests in parallel where safe. Tests are organized into **groups** based on their resource requirements and constraints:

- **Exclusive access tests**: Require sole control of the Docker daemon or other shared resources (run serially)
- **Shared resource tests**: Can share Docker but need limited concurrency
- **Filesystem-heavy tests**: Intensive I/O operations requiring throttling
- **Unit tests**: Fast, isolated tests that can run with high parallelism
- **Integration suites**: Smoke and parity tests requiring careful orchestration

This organization enables:
- ≥40% faster local development feedback loops
- 50–70% CI runtime reduction
- Zero new flaky test failures from resource contention

## Quick Start

### Installation

Install cargo-nextest:

```bash
# Via cargo (recommended)
cargo install cargo-nextest --locked

# Via pre-built binaries
# See: https://nexte.st/book/pre-built-binaries.html
```

Verify installation:
```bash
cargo nextest --version
```

### Running Tests

```bash
# Fast parallel tests (excludes slow smoke/parity tests)
make test-nextest-fast

# Full suite with appropriate grouping
make test-nextest

# CI-aligned conservative profile
make test-nextest-ci

# Long-running tests (heavy integration/build tests)
make test-nextest-long-running

# Audit test group assignments
make test-nextest-audit
```

## Test Groups

Tests are classified into the following groups defined in `.config/nextest.toml`:

### 1. `docker-exclusive`

**Characteristics:**
- Requires exclusive access to the Docker daemon
- Cannot run concurrently with other Docker tests
- Examples: container lifecycle tests, Docker daemon state manipulation

**Concurrency:** `max-threads = 1` (serial execution)

**When to use:**
- Test manipulates Docker daemon state (networks, volumes, system-wide settings)
- Test expects a clean Docker environment
- Test creates/destroys many containers rapidly
- Test depends on specific Docker daemon configuration

**Selectors in `.config/nextest.toml`:**
```toml
[[profile.default.overrides]]
filter = 'test(integration_lifecycle) | test(docker_exclusive)'
threads-required = 1
```

### 2. `docker-shared`

**Characteristics:**
- Uses Docker but can share the daemon with a few other tests
- Needs limited concurrency to avoid overwhelming the daemon
- Examples: container build tests, image operations, isolated container exec

**Concurrency:** `max-threads = 2-4` (limited parallelism)

**When to use:**
- Test creates/runs containers but doesn't manipulate daemon state
- Test can tolerate other containers running simultaneously
- Test uses unique container names/labels to avoid conflicts
- Test cleans up its own resources reliably

**Selectors in `.config/nextest.toml`:**
```toml
[[profile.default.overrides]]
filter = 'package(deacon) & test(integration_) & !test(integration_lifecycle)'
threads-required = 2
```

### 3. `fs-heavy`

**Characteristics:**
- Performs intensive filesystem operations
- May cause contention on shared storage
- Examples: large file copying, tarball extraction, directory tree traversal

**Concurrency:** `max-threads = 4` (moderate parallelism)

**When to use:**
- Test creates/manipulates large files or directory trees
- Test performs many sequential I/O operations
- Test may experience slowdowns from disk contention
- Test doesn't involve Docker at all

**Selectors in `.config/nextest.toml`:**
```toml
[[profile.default.overrides]]
filter = 'test(filesystem) | test(fs_heavy)'
threads-required = 4
```

### 4. `unit-default`

**Characteristics:**
- Fast, isolated unit tests
- No external dependencies (Docker, network, heavy I/O)
- Examples: configuration parsing, validation logic, data structure tests

**Concurrency:** Default parallelism (typically CPU core count)

**When to use:**
- Test is purely in-memory computation
- Test has no external side effects
- Test completes in milliseconds
- Test uses mocks/fakes instead of real resources

**Note:** This is the default group; tests without explicit classification land here.

### 5. `smoke`

**Characteristics:**
- High-level end-to-end integration tests
- Tests complete user workflows
- May require Docker and stable environment
- Examples: CLI smoke tests for major commands

**Concurrency:** `max-threads = 1` (serial execution)

**When to use:**
- Test validates end-to-end workflow (like `up` + `exec` + `down`)
- Test requires predictable, clean environment
- Test serves as integration gate before release
- Test execution order matters

**Selectors in `.config/nextest.toml`:**
```toml
[[profile.default.overrides]]
filter = 'test(smoke_)'
threads-required = 1
```

### 6. `parity`

**Characteristics:**
- Compares behavior against upstream TypeScript CLI
- Requires controlled environment for deterministic comparison
- Must run serially to avoid interference

**Concurrency:** `max-threads = 1` (serial execution)

**When to use:**
- Test validates compatibility with upstream devcontainers/cli
- Test compares output formats or behavior
- Test requires reference implementation availability

**Selectors in `.config/nextest.toml`:**
```toml
[[profile.default.overrides]]
filter = 'test(parity_)'
threads-required = 1
```

### 7. `long-running`

**Characteristics:**
- End-to-end or heavy-build integration tests that can run for several minutes
- Examples: building a large context, complex up sequences, long vulnerability scans

**Concurrency:** `max-threads = 1` (serial execution by default; you can increase if safe)

**When to use:**
- Test runs longer than typical integration suites and impacts iteration time
- Not suitable for `dev-fast` workflows; run in `long-running` profile instead

**Selectors in `.config/nextest.toml`:**
```toml
[[profile.default.overrides]]
filter = 'test(integration_build) | test(integration_up) | test(integration_progress) | test(integration_vulnerability_scan)'
test-group = 'long-running'
slow-timeout = { period = "30m", terminate-after = 1 }
```

## Execution Profiles

The project defines three nextest profiles in `.config/nextest.toml`:

### `dev-fast` (Local Development)

**Purpose:** Maximum speed for local development feedback loops

**Characteristics:**
- Excludes slow smoke and parity tests
- High parallelism for unit tests
- Limited concurrency for Docker tests
- Ideal for rapid iteration during development

**Usage:**
```bash
make test-nextest-fast
# or
cargo nextest run --profile dev-fast
```

**Timing artifacts:** `artifacts/nextest/dev-fast-timing.json`

### `full` (Complete Local Validation)

**Purpose:** Run all tests with appropriate grouping before pushing

**Characteristics:**
- Includes all test groups (unit, integration, smoke, parity)
- Respects serialization for smoke/parity
- Appropriate concurrency for each group
- Use before submitting PRs

**Usage:**
```bash
make test-nextest
# or
cargo nextest run --profile full
```

**Timing artifacts:** `artifacts/nextest/full-timing.json`

### `ci` (Continuous Integration)

**Purpose:** Conservative, deterministic execution for CI environments

**Characteristics:**
- All tests execute with conservative concurrency
- Serial execution for smoke/parity enforced
- Generates structured JSON output for CI systems
- Captures timing data for performance tracking

**Usage:**
```bash
make test-nextest-ci
# or
cargo nextest run --profile ci --final-status-reporter json
```

**Timing artifacts:** `artifacts/nextest/ci-timing.json`

### `long-running` (Heavy integration & build tests)

Purpose: Run long-running integration tests that are intentionally excluded from the `dev-fast` profile to keep local iteration fast.

Characteristics:
- Tests typically run for multiple minutes (up to 30m+) and may exercise large Docker builds or complex integration flows
- Execute as a separate step in CI or locally when validating end-to-end scenarios

Usage:
```bash
make test-nextest-long-running
# or
cargo nextest run --profile long-running
```

Timing artifacts: `artifacts/nextest/long-running-timing.json`

## Classification Workflow

When adding a new test, follow this workflow to classify it into the appropriate group:

### Step 1: Identify Resource Requirements

Ask these questions about your test:

1. **Does it use Docker at all?**
   - No → Consider `unit-default` or `fs-heavy`
   - Yes → Continue to question 2

2. **Does it require exclusive Docker daemon access?**
   - Yes (manipulates daemon state, needs clean environment) → `docker-exclusive`
   - No (just runs containers with unique names) → `docker-shared`

3. **Does it perform heavy filesystem operations?**
   - Yes (large files, many I/O operations) → `fs-heavy`
   - No → `unit-default`

4. **Is it an end-to-end integration test?**
   - Yes (validates complete workflow) → `smoke`
   - Yes (compares with upstream CLI) → `parity`

### Step 2: Name Your Test Appropriately

Use test name prefixes that match the filter patterns in `.config/nextest.toml`:

```rust
// Docker-exclusive tests
#[test]
fn integration_lifecycle_full_up_down() { ... }

// Docker-shared tests  
#[test]
fn integration_build_with_cache() { ... }

// Filesystem-heavy tests
#[test]
fn fs_heavy_large_tarball_extraction() { ... }

// Smoke tests
#[test]
fn smoke_basic_up_exec_down() { ... }

// Parity tests
#[test]
fn parity_read_configuration_output() { ... }

// Unit tests (default - no special prefix needed)
#[test]
fn parse_devcontainer_json() { ... }
```

### Step 3: Update `.config/nextest.toml` if Needed

If your test doesn't match existing filter patterns, add or update a selector:

```toml
[[profile.default.overrides]]
filter = 'test(your_new_pattern)'
threads-required = 2  # Adjust based on group requirements
```

Common filter patterns:
- `test(pattern)` - Match test name containing "pattern"
- `package(name)` - Match tests in specific package
- `test(a) | test(b)` - Logical OR
- `test(a) & test(b)` - Logical AND
- `!test(a)` - Logical NOT

### Step 4: Verify Classification

After updating `.config/nextest.toml` or adding new tests, verify the assignment:

```bash
# Audit all test classifications
make test-nextest-audit

# Or manually check specific tests
cargo nextest list --status --profile full | grep your_test_name
```

Look for the `threads-required` value in the output to confirm your test is in the right group.

### Step 5: Run and Validate

Run the appropriate profile to validate your test:

```bash
# During development
make test-nextest-fast

# Before committing
make test-nextest

# Verify CI will pass
make test-nextest-ci
```

Watch for:
- Flaky failures (may need stricter serialization)
- Slow execution (may need different group)
- Resource exhaustion (may need lower concurrency)

## Auditing Test Assignments

Use the audit target to review all test classifications:

```bash
make test-nextest-audit
```

This runs `cargo nextest list --status --profile full` and shows each test with its thread requirements.

**Example output:**
```
crates/deacon/tests/integration_build.rs::integration_build_basic [threads-required=2]
crates/deacon/tests/integration_lifecycle.rs::integration_lifecycle_up [threads-required=1]
crates/deacon/tests/smoke_basic.rs::smoke_basic_workflow [threads-required=1]
crates/deacon/src/config.rs::parse_json_test [threads-required=default]
```

**Interpretation:**
- `threads-required=1`: Serial execution (docker-exclusive, smoke, parity)
- `threads-required=2-4`: Limited parallelism (docker-shared, fs-heavy)
- `threads-required=default`: Full parallelism (unit-default)

**When to audit:**
- After adding new tests
- After modifying `.config/nextest.toml`
- When investigating flaky test failures
- Before major test suite refactoring

## Remediation Steps

### Problem: New test is flaky in parallel but passes serially

**Symptoms:**
- Test passes with `make test` (serial)
- Test fails intermittently with `make test-nextest`
- Error messages suggest resource contention

**Solution:**
1. Identify if test uses shared resources (Docker, filesystem, network)
2. Move test to stricter group:
   - `docker-shared` → `docker-exclusive`
   - `unit-default` → `fs-heavy` or `docker-shared`
3. Update test name or `.config/nextest.toml` filter
4. Re-run with `make test-nextest-audit` to verify
5. Validate with `make test-nextest` (multiple runs)

**Example:**
```rust
// Before (flaky in docker-shared)
#[test]
fn integration_network_setup() { ... }

// After (stable in docker-exclusive)
#[test]  
fn integration_lifecycle_network_setup() { ... }
```

### Problem: Test requires Docker but fails with "daemon not available"

**Symptoms:**
- Test works locally but fails in CI
- Error: "Cannot connect to Docker daemon"

**Solution:**
1. Ensure test is classified in `docker-exclusive` or `docker-shared`
2. Add `requires_docker = true` annotation in `.config/nextest.toml` (if supported)
3. In CI, verify Docker daemon is running before nextest execution
4. Consider adding graceful skip with clear message:

```rust
#[test]
fn integration_docker_feature() {
    if std::env::var("CI").is_ok() && !docker_available() {
        eprintln!("Skipping test: Docker not available in CI");
        return;
    }
    // test logic
}
```

### Problem: Test runs too slowly even with grouping

**Symptoms:**
- Test takes >30 seconds to complete
- Slows down entire suite
- Not flaky, just slow

**Solution:**
1. Profile the test to identify bottleneck
2. Consider splitting into multiple smaller tests
3. If inherently slow (end-to-end workflow), move to `smoke` group
4. Add `#[ignore]` attribute and run separately:

```rust
#[test]
#[ignore] // Run with: cargo nextest run -- --ignored
fn slow_end_to_end_validation() { ... }
```

5. Document in test comment why it's slow/ignored

### Problem: Test requires specific execution order

**Symptoms:**
- Test passes alone but fails in suite
- Test depends on state from previous test
- Test name suggests ordering (test_01, test_02)

**Solution:**
1. **Preferred:** Refactor test to be independent
   - Add setup/teardown within the test
   - Don't rely on global state
2. **If unavoidable:** Move to `smoke` or `parity` group (serial)
3. Document the dependency clearly:

```rust
/// IMPORTANT: This test must run after smoke_basic_setup due to
/// shared Docker network state. Classified as smoke for serial execution.
#[test]
fn smoke_advanced_networking() { ... }
```

## Troubleshooting

### cargo-nextest not installed

**Error:**
```
Error: cargo-nextest is not installed
```

**Solution:**
```bash
cargo install cargo-nextest --locked
# Or see: https://nexte.st/book/pre-built-binaries.html
```

### Tests pass serially but fail with nextest

**Diagnostic steps:**

1. Identify failing test:
```bash
cargo nextest run --profile full 2>&1 | grep FAILED
```

2. Run just that test serially:
```bash
cargo nextest run --profile ci 'test(specific_test_name)'
```

3. Check if it's a Docker resource issue:
```bash
docker ps  # Look for leaked containers
docker system df  # Check disk usage
```

4. Review test for shared resource usage (see Remediation Steps above)

### Timing artifacts not being created

**Problem:** `artifacts/nextest/*.json` files are missing

**Solution:**

1. Verify directory exists:
```bash
ls -la artifacts/nextest/
```

2. Check `.gitignore` allows tracking:
```bash
grep nextest .gitignore
# Should see: !artifacts/nextest/
```

3. Run with explicit output:
```bash
cargo nextest run --profile ci --final-status-reporter json > artifacts/nextest/manual-timing.json
```

### Unexpected serial execution

**Problem:** Tests run slower than expected, all appear serial

**Diagnostic:**
```bash
# Check effective thread counts
cargo nextest run --profile dev-fast -v
```

Look for lines like:
```
running 42 tests across 8 binaries (4 threads)
```

If threads=1 for all tests:
1. Check `.config/nextest.toml` default profile settings
2. Verify overrides aren't too broad
3. Ensure not running in resource-constrained environment

### macOS-specific issues

**Problem:** Docker-related tests fail on macOS but pass on Linux

**Context:** Docker Desktop on macOS has different behavior:
- Different filesystem semantics (case-insensitive)
- VM-based networking
- Resource limits set in Docker Desktop preferences

**Solution:**
1. Classify macOS-failing tests as `docker-exclusive` (more conservative)
2. Add platform-specific handling:

```rust
#[test]
fn integration_docker_feature() {
    #[cfg(target_os = "macos")]
    {
        // macOS-specific setup or assertions
    }
    
    // Common test logic
}
```

3. Consider separate CI workflow for macOS with conservative settings

## Additional Resources

- [cargo-nextest Documentation](https://nexte.st/)
- [Configuration Reference](https://nexte.st/book/configuration.html)
- [Test Filtering Guide](https://nexte.st/book/filter-expressions.html)
- [GitHub: nextest Repository](https://github.com/nextest-rs/nextest)

## Contributing

When submitting PRs that add tests:

1. **Required:** Classify new tests into appropriate groups
2. **Required:** Update `.config/nextest.toml` if new patterns needed
3. **Required:** Verify with `make test-nextest-audit`
4. **Required:** Confirm `make test-nextest-ci` passes
5. **Recommended:** Include timing comparison in PR description
6. **Recommended:** Document why test is in specific group (if non-obvious)

**PR Checklist:**
- [ ] New tests have appropriate name prefixes or filters
- [ ] `.config/nextest.toml` updated if new patterns introduced
- [ ] `make test-nextest-audit` shows correct classification
- [ ] `make test-nextest-ci` passes (multiple runs if flaky history)
- [ ] Timing artifacts show acceptable performance
- [ ] Group assignment documented in test comments if non-standard

---

For questions or issues with test classification, please:
1. Check this guide first
2. Run `make test-nextest-audit` to inspect current state
3. Review `.config/nextest.toml` for existing patterns
4. Open an issue with nextest output and test description
