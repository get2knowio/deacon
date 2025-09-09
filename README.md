# deacon

A Rust reimplementation of the Development Containers CLI, following the [containers.dev specification](https://containers.dev).

**Status**: Work in Progress - No functional commands implemented yet.

## Quick Start

### Install from Source
```bash
git clone https://github.com/get2knowio/deacon.git
cd deacon
cargo build --release
./target/release/deacon --help
```

### Install from Release (when available)
```bash
# Download the latest release for your platform from:
# https://github.com/get2knowio/deacon/releases
curl -L https://github.com/get2knowio/deacon/releases/latest/download/deacon-v0.1.0-x86_64-linux -o deacon
chmod +x deacon
./deacon --help
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
