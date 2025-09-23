# deacon

A Rust reimplementation of the Development Containers CLI, following the [containers.dev specification](https://containers.dev).

**Status**: Early Development - Basic configuration reading and CLI framework implemented.

## Current Implementation Status

### ✅ Implemented Features
- **Configuration Discovery**: Finds devcontainer.json in workspace
- **Configuration Loading**: Parses JSON-with-comments (JSONC) format
- **Variable Substitution**: Replaces workspace and environment variables
- **Read Configuration Command**: `deacon read-configuration` outputs processed JSON
- **CLI Framework**: Complete command structure with help and logging
- **Error Handling**: Proper error messages for missing files and invalid JSON

### 🚧 In Progress
- **Feature System**: Basic parsing implemented, installation simulation in tests
- **Plugin Support**: Configuration parsing ready, plugin execution framework needed
- **Extends Resolution**: Configuration inheritance partially implemented

### 📋 Planned Features
- **Container Lifecycle**: Building and running containers
- **Docker Integration**: Real Docker operations (currently simulated)
- **Template System**: DevContainer template management
- **OCI Registry Support**: Pulling features and templates from registries

### 🧪 Test Coverage
- **Unit Tests**: Core functionality well-tested
- **Integration Tests**: CLI commands and basic workflows
- **End-to-End Tests**: Complete workflow validation (7 scenarios, runtime < 30s)

## Quick Start

### Install with Script (Recommended)
```bash
curl -fsSL https://raw.githubusercontent.com/get2knowio/deacon/main/scripts/install.sh | sh
```

This will automatically detect your platform, download the latest release, verify checksums, and install to your PATH.

### Manual Installation
Download the latest release for your platform from the [releases page](https://github.com/get2knowio/deacon/releases):

```bash
# For Linux x86_64
curl -L https://github.com/get2knowio/deacon/releases/latest/download/deacon-v0.1.0-x86_64-unknown-linux-gnu.tar.gz -o deacon.tar.gz
tar -xzf deacon.tar.gz
sudo mv deacon /usr/local/bin/

# For macOS x86_64
curl -L https://github.com/get2knowio/deacon/releases/latest/download/deacon-v0.1.0-x86_64-apple-darwin.tar.gz -o deacon.tar.gz
tar -xzf deacon.tar.gz
sudo mv deacon /usr/local/bin/

# For macOS ARM64 (Apple Silicon)
curl -L https://github.com/get2knowio/deacon/releases/latest/download/deacon-v0.1.0-aarch64-apple-darwin.tar.gz -o deacon.tar.gz
tar -xzf deacon.tar.gz
sudo mv deacon /usr/local/bin/

# For Windows x86_64 (PowerShell)
Invoke-WebRequest -Uri "https://github.com/get2knowio/deacon/releases/latest/download/deacon-v0.1.0-x86_64-pc-windows-msvc.zip" -OutFile "deacon.zip"
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
- Feature Management: minimal & with-options feature manifests (`examples/feature-management/`)
- Template Management: minimal & with-options templates including Dockerfile and assets (`examples/template-management/`)

Try one:
```bash
cd examples/feature-management/minimal-feature
deacon features test . --json
```

See the full details and additional commands in `examples/README.md`.

## Feature Flags & Build Variants

The workspace uses Cargo feature flags to keep the default binary lean while enabling advanced capabilities on demand.

| Feature (crate) | Default | Purpose | When to Enable |
|-----------------|---------|---------|----------------|
| `docker` (`deacon`, `deacon-core`) | ON (default) | Docker / container lifecycle integration | Disable only for pure config or analysis builds without Docker present |
| `config` (`deacon`) | OFF | Additional configuration format support (reserves optional `toml` dep) | Building a full “all capabilities” release or future extended config workflows |
| `json-logs` (`deacon-core`) | OFF | Structured JSON logging output via `tracing-subscriber` | CI ingestion / machine parsing of logs |
| `plugins` (`deacon`, `deacon-core`) | OFF | Experimental plugin / extension hooks (scaffolding) | Evaluating or developing plugin system (unstable) |

### Common Build Profiles

```bash
# Default (docker only)
cargo build --release

# Minimal (no Docker; config & plugins omitted)
cargo build --release --no-default-features

# Full feature set (intended production / Homebrew style release)
cargo build --release --no-default-features --features "docker,config,plugins"

# Full + JSON logs (for structured logging distributions)
cargo build --release --no-default-features --features "docker,config,plugins,json-logs"
```

If you built without `config` and examples referencing `read-configuration` fail unexpectedly, rebuild with the appropriate feature set (see above).

To inspect enabled features at runtime you can compare output sizes or run `cargo tree -F deacon/config` for dependency changes.


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

This CLI will implement the DevContainer specification including:

- Configuration resolution and parsing (`devcontainer.json`)
- Feature system for reusable development environment components
- Template system for scaffolding new projects
- Container lifecycle management
- Docker/OCI integration
- Cross-platform support

See the [CLI specification](docs/CLI-SPEC.md) for detailed architecture and planned features.

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
