# deacon

<!-- Badges -->
<p>
  <a href="https://github.com/get2knowio/deacon/actions/workflows/ci.yml">
    <img alt="Build Status" src="https://github.com/get2knowio/deacon/actions/workflows/ci.yml/badge.svg" />
  </a>
  <a href="https://coveralls.io/github/get2knowio/deacon?branch=main">
    <img alt="Coverage Status" src="https://coveralls.io/repos/github/get2knowio/deacon/badge.svg?branch=main" />
  </a>
  <a href="https://github.com/get2knowio/deacon/blob/main/LICENSE">
    <img alt="License: MIT" src="https://img.shields.io/badge/License-MIT-green.svg" />
  </a>
  <a href="https://github.com/get2knowio/deacon/releases/latest">
    <img alt="Latest Release" src="https://img.shields.io/github/v/release/get2knowio/deacon" />
  </a>
  <img alt="Rust 2021" src="https://img.shields.io/badge/rust-2021-orange" />
</p>

A Rust implementation of the Development Containers CLI, following the [containers.dev specification](https://containers.dev).

 



## Quick Start

### Install with Script (Recommended)
```bash
curl -fsSL https://raw.githubusercontent.com/get2knowio/deacon/main/scripts/install.sh | bash
```

This will automatically detect your platform, download the latest release, verify checksums, and install to your PATH.

#### Script options (env vars)
You can steer the installer using the following environment variables:

- `DEACON_VERSION` — Specific version to install (default: latest). Accepts `v0.1.3` or `0.1.3`.
- `DEACON_BASE_URL` — Base URL for release downloads (default: GitHub Releases). Useful for mirrors.
- `DEACON_INSTALL_DIR` — Install directory (default: `/usr/local/bin` if writable, otherwise `~/.local/bin`).
- `DEACON_FORCE` — Set to `true` to overwrite an existing binary without a prompt.

Examples:
```bash
# Install a specific version
curl -fsSL https://raw.githubusercontent.com/get2knowio/deacon/main/scripts/install.sh | DEACON_VERSION=0.1.3 bash

# Install to a custom directory and overwrite without prompt
curl -fsSL https://raw.githubusercontent.com/get2knowio/deacon/main/scripts/install.sh | \
  DEACON_VERSION=v0.1.3 DEACON_INSTALL_DIR="$HOME/.local/bin" DEACON_FORCE=true bash
```

Note: On Linux, the installer auto-detects your libc (GNU vs musl) and selects the matching asset (e.g., Alpine → musl).

### Manual Installation
Download the latest release for your platform from the [releases page](https://github.com/get2knowio/deacon/releases):

```bash
# For Linux x86_64 (glibc)
curl -L https://github.com/get2knowio/deacon/releases/latest/download/deacon-v0.1.3-x86_64-unknown-linux-gnu.tar.gz -o deacon.tar.gz
tar -xzf deacon.tar.gz
sudo mv deacon /usr/local/bin/

# For Linux x86_64 (musl/Alpine)
curl -L https://github.com/get2knowio/deacon/releases/latest/download/deacon-v0.1.3-x86_64-unknown-linux-musl.tar.gz -o deacon.tar.gz
tar -xzf deacon.tar.gz
sudo mv deacon /usr/local/bin/

# For macOS x86_64
curl -L https://github.com/get2knowio/deacon/releases/latest/download/deacon-v0.1.3-x86_64-apple-darwin.tar.gz -o deacon.tar.gz
tar -xzf deacon.tar.gz
sudo mv deacon /usr/local/bin/

# For macOS ARM64 (Apple Silicon)
curl -L https://github.com/get2knowio/deacon/releases/latest/download/deacon-v0.1.3-aarch64-apple-darwin.tar.gz -o deacon.tar.gz
tar -xzf deacon.tar.gz
sudo mv deacon /usr/local/bin/

# For Windows x86_64 (PowerShell)
Invoke-WebRequest -Uri "https://github.com/get2knowio/deacon/releases/latest/download/deacon-v0.1.3-x86_64-pc-windows-msvc.zip" -OutFile "deacon.zip"
Expand-Archive -Path "deacon.zip" -DestinationPath "."
# Move deacon.exe to a directory in your PATH
```

### Install from Source
```bash
git clone https://github.com/get2knowio/deacon.git
cd deacon
cargo build --release
./target/release/deacon --help
```

#### Install from Source (Standard Build)
```bash
git clone https://github.com/get2knowio/deacon.git
cd deacon
cargo build --release
./target/release/deacon --version
```

### Install from Cargo (Future)
*Note: Publishing to crates.io is planned for a future release.*
```bash
# This will be available in the future
cargo install deacon
```

### Verify Installation
```bash
deacon --help
```

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

Deacon supports multiple container runtimes. You can choose between Docker (default) and Podman:

### Via CLI Flag
```bash
# Use Docker (default)
deacon --runtime docker up

# Use Podman (future support)
deacon --runtime podman up
```

### Via Environment Variable
```bash
# Set runtime via environment variable
export DEACON_RUNTIME=podman
deacon up

# One-time override
DEACON_RUNTIME=podman deacon up
```

### Precedence
Runtime selection follows this precedence:
1. CLI flag (`--runtime`)
2. Environment variable (`DEACON_RUNTIME`)
3. Default (docker)

Note: Podman support is currently in development. Using `--runtime podman` will show a "Not implemented yet" error with clear next steps.

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

## Roadmap

This CLI implements the DevContainer specification domains below and continues to expand coverage:

- Configuration resolution and parsing (`devcontainer.json`)
- Feature system for reusable development environment components
- Template system for scaffolding new projects
- Container lifecycle management
- Docker/OCI integration
- Cross-platform support

See the [CLI specification](docs/CLI-SPEC.md) for detailed architecture and planned features.

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
