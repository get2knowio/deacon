# deacon

A Rust reimplementation of the Development Containers CLI, following the [containers.dev specification](https://containers.dev).

**Status**: Work in Progress - No functional commands implemented yet.

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
