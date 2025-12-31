# Nextest Helper Scripts

This directory contains helper scripts for cargo-nextest integration with the deacon project.

## Scripts

### `assert-installed.sh`
Preflight check that verifies cargo-nextest is available before running nextest commands. Provides actionable installation guidance if missing.

**Usage:**
```bash
./scripts/nextest/assert-installed.sh
```

**Exit codes:**
- `0`: cargo-nextest is installed and available
- `1`: cargo-nextest is missing

### `capture-timing.sh`
Helper script to capture and record timing data from nextest runs for performance analysis and comparison.

**Usage:**
```bash
./scripts/nextest/capture-timing.sh <profile> <output-path>
```

**Arguments:**
- `profile`: The nextest profile name (e.g., `dev-fast`, `full`, `ci`)
- `output-path`: Where to write the timing JSON artifact

**Example:**
```bash
./scripts/nextest/capture-timing.sh dev-fast artifacts/nextest/dev-fast-timing.json
```

## Purpose

These scripts support the test parallelization workflow by:

1. Ensuring cargo-nextest is available before execution (fail-fast principle)
2. Capturing timing data for performance analysis and CI metrics
3. Providing consistent behavior across local development and CI environments

## Related Documentation

- Main nextest guide: `docs/testing/nextest.md`
- Quickstart: `specs/001-nextest-parallel-tests/quickstart.md`
- Timing artifacts: `artifacts/nextest/README.md`
