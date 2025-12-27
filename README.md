# deacon

A fast, Rust-based [Dev Containers](https://containers.dev) CLI.

<!-- Badges -->
<p>
  <a href="https://github.com/get2knowio/deacon/releases/latest">
    <img alt="Latest Release" src="https://img.shields.io/github/v/release/get2knowio/deacon" />
  </a>
  <a href="https://github.com/get2knowio/deacon/actions/workflows/ci.yml">
    <img alt="Build Status" src="https://github.com/get2knowio/deacon/actions/workflows/ci.yml/badge.svg" />
  </a>
  <a href="https://github.com/get2knowio/deacon/blob/main/LICENSE">
    <img alt="License: MIT" src="https://img.shields.io/badge/License-MIT-green.svg" />
  </a>
</p>

## Install

```bash
curl -fsSL https://get2knowio.github.io/deacon/install.sh | bash
```

<details>
<summary>Other installation methods</summary>

### macOS (Homebrew) - Coming Soon
```bash
brew install get2knowio/tap/deacon
```

### Manual Download
Download from [releases](https://github.com/get2knowio/deacon/releases/latest):

| Platform | Architecture | Download |
|----------|--------------|----------|
| Linux | x86_64 | [deacon-linux-x86_64.tar.gz](https://github.com/get2knowio/deacon/releases/latest/download/deacon-v0.2.0-x86_64-unknown-linux-gnu.tar.gz) |
| Linux | ARM64 | [deacon-linux-arm64.tar.gz](https://github.com/get2knowio/deacon/releases/latest/download/deacon-v0.2.0-aarch64-unknown-linux-gnu.tar.gz) |
| Linux (musl) | x86_64 | [deacon-linux-musl-x86_64.tar.gz](https://github.com/get2knowio/deacon/releases/latest/download/deacon-v0.2.0-x86_64-unknown-linux-musl.tar.gz) |
| macOS | x86_64 | [deacon-darwin-x86_64.tar.gz](https://github.com/get2knowio/deacon/releases/latest/download/deacon-v0.2.0-x86_64-apple-darwin.tar.gz) |
| macOS | ARM64 (Apple Silicon) | [deacon-darwin-arm64.tar.gz](https://github.com/get2knowio/deacon/releases/latest/download/deacon-v0.2.0-aarch64-apple-darwin.tar.gz) |
| Windows | x86_64 | [deacon-windows-x86_64.zip](https://github.com/get2knowio/deacon/releases/latest/download/deacon-v0.2.0-x86_64-pc-windows-msvc.zip) |
| Windows | ARM64 | [deacon-windows-arm64.zip](https://github.com/get2knowio/deacon/releases/latest/download/deacon-v0.2.0-aarch64-pc-windows-msvc.zip) |

### From Source
```bash
git clone https://github.com/get2knowio/deacon.git
cd deacon
cargo build --release
./target/release/deacon --help
```

### Installer Options
The install script supports these environment variables:
- `DEACON_VERSION` — Specific version (default: latest)
- `DEACON_INSTALL_DIR` — Install directory (default: `~/.local/bin`)
- `DEACON_FORCE=true` — Overwrite existing binary without prompt

```bash
# Install specific version
curl -fsSL https://get2knowio.github.io/deacon/install.sh | DEACON_VERSION=0.2.0 bash
```

</details>

## Quick Start

```bash
# Start a dev container
deacon up

# Run a command in the container
deacon exec -- npm install

# Stop and remove the container
deacon down
```

Verify installation:
```bash
deacon --version
```

## In Progress

The following features are planned but not yet ready for use:

| Feature | Status | Notes |
|---------|--------|-------|
| Docker Compose profiles | Planned | Basic Compose works, profile selection coming soon |
| Features installation during `up` | Experimental | Feature config merges, but installation may be incomplete |
| Dotfiles (container-side) | Planned | Host-side dotfiles work, container clone/install coming |
| `--expect-existing-container` | Planned | Flag exists but validation not implemented |
| Port forwarding | Planned | Flags exist, functionality deferred |
| Podman runtime | Planned | Docker is fully supported; Podman coming later |

For the full roadmap, see [docs/MVP-ROADMAP.md](docs/MVP-ROADMAP.md).

## Examples

Self-contained categorized examples live under [`examples/`](examples/README.md):

- Configuration: variable substitution, lifecycle commands basics (`examples/configuration/`)
- Container Lifecycle: lifecycle command execution, ordering, and variables (`examples/container-lifecycle/`)
- Feature Management: minimal & with-options feature manifests (`examples/feature-management/`)
- Template Management: minimal & with-options templates including Dockerfile and assets (`examples/template-management/`)

Try one:
```bash
cd examples/feature-management/minimal-feature
deacon features test . --json
```

See the full details and additional commands in `examples/README.md`.

## Runtime Selection

Deacon uses Docker as its container runtime. Podman support is planned for a future release.

```bash
# Explicitly select Docker (optional, it's the default)
deacon --runtime docker up

# Or via environment variable
DEACON_RUNTIME=docker deacon up
```

## Runtime Configuration

### Logging
Deacon supports both human-readable text and structured JSON logging formats.

**Text Logging (Default):**
```bash
deacon --help  # Standard text output
```

**JSON Logging:**
```bash
export DEACON_LOG_FORMAT=json
deacon doctor  # Structured JSON logs for machine parsing
```

The JSON format is useful for CI/CD systems and log aggregation tools that need structured data.

### Quiet mode and spinner (TTY only)

When running in a real terminal (stderr is a TTY) and using the default text output, deacon shows a small spinner during long-running operations like `up` and `down`. In these spinner sessions, if you haven't set `DEACON_LOG`, `RUST_LOG`, or `--log-level`, the default log level is temporarily set to `warn` so routine progress noise stays out of your way. JSON mode or non‑TTY environments (CI, redirections) do not render a spinner and keep the previous logging behavior.

Tips:
- Want details with the spinner? Set `RUST_LOG=info` or use `--log-level info`.
- Prefer structured logs? Use `--log-format json` (no spinner) and parse stderr.

Color and accessibility:
- Help/usage output uses automatic color when writing to a terminal. Spinner/status messages also use subtle colors (yellow for in‑progress, green for success, red for failures).
- Respecting your environment, color is disabled when not writing to a TTY and when `NO_COLOR` is set (see https://no-color.org/). To force-disable colors, export `NO_COLOR=1`.

### PTY Allocation for JSON Log Mode

When using JSON logging format (`--log-format json`), lifecycle commands (onCreate, postCreate, etc.) run without PTY (pseudo-terminal) allocation by default. This is ideal for non-interactive scripts and automated environments.

However, if your lifecycle commands need interactive terminal behavior while maintaining structured JSON logs, you can force PTY allocation:

**Via CLI Flag:**
```bash
deacon up --log-format json --force-tty-if-json
```

**Via Environment Variable:**
```bash
# Enable PTY allocation
export DEACON_FORCE_TTY_IF_JSON=true
deacon up --log-format json

# Disable PTY allocation (default)
export DEACON_FORCE_TTY_IF_JSON=false
deacon up --log-format json
```

**Truthy values** (case-insensitive): `true`, `1`, `yes`
**Falsey values**: `false`, `0`, `no`, or unset

**Precedence:**
1. CLI flag (`--force-tty-if-json`)
2. Environment variable (`DEACON_FORCE_TTY_IF_JSON`)
3. Default (no PTY allocation)

**Important Notes:**
- This setting only applies when `--log-format json` is active
- With PTY allocation enabled, interactive commands work correctly while JSON logs remain structured on stderr and machine-readable output stays on stdout
- Without PTY allocation (default), lifecycle commands run in non-interactive mode

## Output Streams

Deacon follows a strict stdout/stderr separation contract to ensure reliable machine-readable output:

### Stream Usage Contract

1. **JSON Output Modes** (`--output json`, `--json` flags):
   - **stdout**: Single JSON document (newline terminated), nothing else
   - **stderr**: All logs, diagnostics, and progress messages via `tracing`
   - **Guarantee**: Scripts parsing stdout will receive only valid JSON

2. **Text Output Modes** (default):
   - **stdout**: User-facing result summaries and human-readable reports only
   - **stderr**: All logs, diagnostics, and progress messages via `tracing`
   - **Note**: Text format content may evolve; use JSON modes for stable parsing

3. **Error Conditions**:
   - **Non-zero exit**: stdout may be empty unless partial results are explicitly supported
   - **All errors**: Logged to stderr, never stdout

### Examples

```bash
# JSON mode - stdout contains only JSON, logs go to stderr
deacon read-configuration --output json > config.json 2> logs.txt

# Text mode - stdout contains human-readable results, logs to stderr  
deacon doctor > diagnosis.txt 2> logs.txt

# Parsing JSON output safely
OUTPUT=$(deacon features plan --json 2>/dev/null)
echo "$OUTPUT" | jq '.order'
```

### Integration Guidelines

- **Automation/CI**: Always use JSON output modes (`--json`, `--output json`) for reliable parsing
- **Human Use**: Default text modes provide better readability and context
- **Logging**: Use `--log-level` and `--log-format` to control stderr verbosity and format

### Docker Integration
All Docker functionality is available when Docker is installed and running. If the Docker daemon isn’t available, deacon emits a clear runtime error with guidance to install or start Docker.

## Usage

### Reading DevContainer Configuration
The `read-configuration` command loads, processes, and outputs your devcontainer.json:

```bash
# In a directory with .devcontainer/devcontainer.json or .devcontainer.json
deacon read-configuration

# With explicit config path
deacon read-configuration --config /path/to/devcontainer.json

# Include merged configuration (with extends resolution)
deacon read-configuration --include-merged-configuration

# With debug logging
deacon read-configuration --log-level debug
```

**Example output:**
```json
{
  "name": "my-dev-container",
  "image": "mcr.microsoft.com/devcontainers/base:ubuntu",
  "workspaceFolder": "/workspaces/my-project",
  "features": {
    "ghcr.io/devcontainers/features/docker-in-docker:2": {}
  },
  "customizations": {
    "vscode": {
      "extensions": ["ms-python.python"]
    }
  }
}
```

**Variable Substitution:**
Variables like `${localWorkspaceFolder}` are automatically replaced:
```json
{
  "workspaceFolder": "${localWorkspaceFolder}/src",
  "containerEnv": {
    "PROJECT_ROOT": "${localWorkspaceFolder}"
  }
}
```
becomes:
```json
{
  "workspaceFolder": "/home/user/project/src",
  "containerEnv": {
    "PROJECT_ROOT": "/home/user/project"
  }
}
```

### Logging

Deacon uses the `tracing` ecosystem for structured logging.

- Default log level: `info`
- Default log format: text (human-readable)
- CLI flag overrides: `--log-level` (`error|warn|info|debug|trace`), `--log-format` (`text|json`)
- Environment overrides (take precedence before CLI flag processing sets `RUST_LOG`):
  - `DEACON_LOG`: Full filter specification (e.g. `DEACON_LOG=deacon=debug,deacon_core=debug`)
  - `RUST_LOG`: Standard Rust filter fallback if `DEACON_LOG` unset

When you specify `--log-level`, the CLI sets `RUST_LOG` internally to `deacon=<level>,deacon_core=<level>` prior to initializing the subscriber. Use `DEACON_LOG` for advanced per-module filtering; it will be honored as-is (and will emit a warning and fall back to `info` if invalid).

Guidance on levels:
- `info`: High-level milestones and user‑visible state changes (container start/stop, template application summary, configuration load boundaries)
- `debug`: Detailed decision points (config discovery paths, feature resolution steps, variable substitution reports)
- `trace`: Very fine‑grained internals (iteration over collections, per-file copy decisions) – typically for deep troubleshooting only
- `warn`: Recoverable issues or unexpected states deviating from normal expectations
- `error`: Failures causing command termination or skipped critical workflow phases

Examples:
```bash
# Increase verbosity for troubleshooting configuration issues
deacon read-configuration --log-level debug

# Use JSON logs (machine parsing / CI ingestion)
deacon up --log-format json

# Advanced module filtering (show trace for config, keep others at info)
DEACON_LOG=deacon_core::config=trace,deacon=info deacon read-configuration
```

If you disable secret redaction (`--no-redact`), secret values may appear in logs—avoid in shared terminals or CI unless strictly necessary.

### Development Build
```bash
cargo run -- --help
cargo test
```

### Running Tests

The project supports both traditional `cargo test` and parallel execution via [cargo-nextest](https://nexte.st/).

#### Standard Test Commands (Serial Execution)
```bash
# Run all tests serially (default)
make test

# Fast feedback loop: unit + bins + examples + doctests (no integration)
make test-fast

# Development fast loop: fmt-check + clippy + fast tests
make dev-fast
```

#### Parallel Test Execution with cargo-nextest

For faster feedback, install [cargo-nextest](https://nexte.st/):
```bash
cargo install cargo-nextest --locked
# Or follow: https://nexte.st/book/pre-built-binaries.html
```

Then use the nextest targets:
```bash
# Fast parallel tests (excludes smoke/parity tests)
make test-nextest-fast

# Full test suite with parallel execution and test grouping
make test-nextest

# CI-aligned conservative profile
make test-nextest-ci
```

**Test Groups**: The project organizes tests into groups based on resource requirements:
- **docker-exclusive**: Tests requiring exclusive Docker daemon access (serial)
- **docker-shared**: Tests that can share Docker daemon (limited parallelism)
- **fs-heavy**: Filesystem-intensive tests (limited parallelism)
- **unit-default**: Fast unit tests with high parallelism
- **smoke**: High-level integration tests (serial)
- **parity**: Upstream CLI comparison tests (serial)

Timing data is automatically captured in `artifacts/nextest/` for performance analysis.

**Fallback Behavior**: If cargo-nextest is not installed, the Make targets will fail with clear installation instructions. Always keep the standard `make test` working as a fallback.

#### Test Classification Checklist

When adding new tests, classify them into the appropriate group:

1. **Does the test use Docker?**
   - No → `unit-default` or `fs-heavy`
   - Yes → Continue to step 2

2. **Does it require exclusive Docker daemon access?**
   - Yes (manipulates daemon state) → `docker-exclusive`
   - No (just runs containers) → `docker-shared`

3. **Does it perform heavy filesystem operations?**
   - Yes (large files, many I/O ops) → `fs-heavy`

4. **Is it an end-to-end integration test?**
   - Yes (validates complete workflow) → `smoke`
   - Yes (compares with upstream CLI) → `parity`

**Audit test assignments:**
```bash
# List all tests with their group classifications
make test-nextest-audit
```

**Example test naming for automatic classification:**
```rust
// Docker-exclusive tests
#[test]
fn integration_lifecycle_full_up_down() { ... }

// Docker-shared tests  
#[test]
fn integration_build_with_cache() { ... }

// Smoke tests
#[test]
fn smoke_basic_workflow() { ... }
```

#### Troubleshooting Test Issues

**Flaky tests in parallel execution:**
- Test passes with `make test` but fails with `make test-nextest`
- **Solution**: Reclassify to more conservative group (e.g., `docker-shared` → `docker-exclusive`)
- Update test name or `.config/nextest.toml` filter
- Verify with `make test-nextest-audit`
- Validate with multiple runs of `make test-nextest`

**Tests requiring specific order:**
- **Preferred**: Refactor test to be independent
- **If unavoidable**: Move to `smoke` group (serial execution)
- Document the dependency in test comments

**Slow tests:**
- Profile to identify bottleneck
- Consider splitting into smaller tests
- Move inherently slow end-to-end tests to `smoke` group
- Use `#[ignore]` for very slow tests: `cargo nextest run -- --ignored`

For comprehensive troubleshooting, classification workflows, and remediation steps, see [docs/testing/nextest.md](docs/testing/nextest.md).

## Roadmap

This CLI implements the DevContainer specification domains below and continues to expand coverage:

- Configuration resolution and parsing (`devcontainer.json`)
- Feature system for reusable development environment components
- Template system for scaffolding new projects
- Container lifecycle management
- Docker/OCI integration
- Cross-platform support

See the [CLI specification](docs/subcommand-specs/*/SPEC.md) for detailed architecture and planned features.

For detailed, language-agnostic designs of specific subcommands, see:
- Features Test: `docs/subcommand-specs/features-test/SPEC.md`
- Features Package: `docs/subcommand-specs/features-package/SPEC.md`
- Features Publish: `docs/subcommand-specs/features-publish/SPEC.md`
- Features Info: `docs/subcommand-specs/features-info/SPEC.md`
- Features Plan: `docs/subcommand-specs/features-plan/SPEC.md`

### Binary Authenticity & Code Signing

Current release artifacts (tar.gz / zip) are **not yet code signed**. Integrity is provided via `SHA256SUMS` published with every release. You should always:

```bash
# Download archive and checksum listing
curl -LO https://github.com/get2knowio/deacon/releases/download/<version>/SHA256SUMS
grep '<archive-filename>' SHA256SUMS | sha256sum -c -
```

Planned enhancements (tracked in issue: Code Signing):
- GPG detached signature for `SHA256SUMS` (`SHA256SUMS.asc`)
- macOS codesign + notarization
- Windows Authenticode signature
- Supply chain provenance (SLSA build attestation)

Until signatures are in place, rely on checksum verification and the GitHub release provenance. If you need reproducible build parity, the workflow captures the exact `rustc -Vv` used in each release (`RUSTC_VERSION.txt` asset). Deterministic builds for comparison can be performed via:

```bash
rustup toolchain install $(grep '^release:' RUSTC_VERSION.txt | cut -d':' -f2 | xargs)
cargo build --release --locked --all-features
```

If you have requirements around signed binaries and would like to help accelerate this, comment on the tracking issue once it is opened.

## Contributing

See [CONTRIBUTING.md](./CONTRIBUTING.md) for development workflow, testing guidelines, and contribution requirements.

## Continuous Integration

CI runs via GitHub Actions and uses the Makefile + cargo-nextest:

- Lint: rustfmt check, cargo check, clippy, and doctests
- Test (Ubuntu): `make test-nextest-fast` with Docker available
- Smoke (Ubuntu): `make test-smoke` (serial, Docker required)
- Nextest CI (Ubuntu/macOS): `make test-nextest-ci` with timing artifact `artifacts/nextest/ci-timing.json`
- Other OS (macOS/Windows): runs unit + non‑smoke integration tests and separate smoke tests; macOS uses Colima for Docker

Notes:
- Networked integration tests run only in selected jobs (smoke and nextest-ci) via `DEACON_NETWORK_TESTS=1` to keep the fast test job hermetic.
- Test grouping and concurrency are configured in `.config/nextest.toml`. See docs/testing/nextest.md for details.

## Test Coverage

We use cargo-llvm-cov (LLVM source-based coverage) locally and in CI.

- Install toolchain addon and helper:
	- rustup component add llvm-tools-preview
	- cargo install cargo-llvm-cov

- Run coverage locally and open HTML report:
	- cargo llvm-cov --workspace --open

- Generate LCOV for external services:
	- cargo llvm-cov --workspace --lcov --output-path lcov.info

CI enforces a minimum line coverage threshold (see MIN_COVERAGE in `.github/workflows/ci.yml`). To try the same locally:

- cargo llvm-cov --workspace --fail-under-lines 80

Coverage reporting is published to Coveralls for the `main` branch and PRs: https://coveralls.io/github/get2knowio/deacon
