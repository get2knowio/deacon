# Rust CI Steps Composite Action

This composite action encapsulates common Rust CI steps used across deacon's workflows to reduce duplication and improve maintainability.

## Purpose

Provides standardized steps for:
- Code checkout
- Rust toolchain installation with configurable components
- Cargo registry and target caching
- Lint checks (formatting, clippy, doctests)
- Build execution
- Test execution (unit, integration, smoke)

## Inputs

### `job-type` (required)
Type of CI job to run. Determines which steps are executed.

**Options:**
- `lint` - Runs formatting checks, clippy, and doctests
- `test` - Builds project and runs unit + integration tests (excluding smoke tests)
- `smoke` - Builds project and runs smoke tests only

### `os-name` (optional)
Operating system name for display purposes.

**Default:** `ubuntu`

**Options:** `ubuntu`, `macos`, `windows`

### `rust-components` (optional)
Comma-separated list of Rust toolchain components to install.

**Default:** `rustfmt,clippy`

**Examples:** `rustfmt,clippy`, `llvm-tools-preview`

## Usage Examples

### Lint Job
```yaml
- name: Run Rust CI steps (lint)
  uses: ./.github/actions/rust-ci-steps
  with:
    job-type: lint
    os-name: ubuntu
```

### Test Job
```yaml
- name: Run Rust CI steps (test)
  uses: ./.github/actions/rust-ci-steps
  with:
    job-type: test
    os-name: macos
```

### Smoke Test Job
```yaml
- name: Run Rust CI steps (smoke)
  uses: ./.github/actions/rust-ci-steps
  with:
    job-type: smoke
    os-name: windows
```

## Notes

- This action handles common Rust CI steps only
- OS-specific setup (Docker, Colima, etc.) should be handled in the workflow file
- Cache keys use `runner.os` and `Cargo.lock` hash for optimal cache hits
- All shell commands use bash for cross-platform compatibility
- Test discovery uses portable `find` and `sed` commands that work on Linux, macOS, and Windows Git Bash

## Maintenance

When updating CI behavior:
1. Update this composite action for changes affecting all workflows
2. Update individual workflow files for OS-specific changes
3. Keep job names consistent to maintain branch protection rules
4. Test changes on all target platforms (Ubuntu, macOS, Windows)
