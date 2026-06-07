## Language & Runtime

- **Rust 1.70+**, Edition 2021, stable toolchain
- Workspace with 2 crates: `crates/deacon` (binary), `crates/core` (library)
- `unsafe_code = "forbid"` workspace-wide

## Key Dependencies

### Core Library (`crates/core`)
| Crate | Purpose |
|-------|---------|
| `tokio` (rt, process, macros, io-util) | Async runtime |
| `reqwest 0.12` (rustls-tls, **no OpenSSL**) | HTTP client for OCI registry |
| `serde` / `serde_json` / `json5` | Serialization, JSONC parsing |
| `thiserror 1.0` | Domain error types |
| `anyhow 1.0` | Error context (binary boundary) |
| `tracing` / `tracing-subscriber` (env-filter, json) | Structured logging |
| `indexmap` | Ordered maps (spec-required declaration order) |
| `sha2` / `blake3` | Content hashing (OCI digests) |
| `tar` / `zip` / `flate2` | Archive extraction (features, templates) |
| `regex` / `semver` | Pattern matching, version parsing |
| `sysinfo` / `directories-next` | System info |
| `chrono` / `bincode` / `base64` / `fastrand` | Utilities |

### CLI Binary (`crates/deacon`)
| Crate | Purpose |
|-------|---------|
| `clap 4.5` (derive + color) | CLI argument parsing |
| `indicatif` / `console` / `atty` | Progress bars, terminal UI |
| `toml 0.8` | TOML parsing |
| `shell-words 1.1` | Shell argument splitting |

## Build System

- **Cargo** (standard Rust toolchain) + **Makefile** convenience targets
- Feature flags: `--no-default-features` (MVP: up/exec/down/read-configuration) vs default (full)
- `.cargo/config.toml` — workspace-level config

## Testing

- **`cargo-nextest`** — primary test runner (parallel, 11 test groups)
- **Profiles** in `.config/nextest.toml`: dev-fast, default, full, docker, ci, mvp-integration
- **Coverage**: `cargo-llvm-cov` with CI threshold enforcement

```bash
make dev-fast             # fmt + clippy + fast tests (default dev loop)
make test-nextest-fast    # Unit/bins/examples + doctests (no docker/smoke)
make test-nextest-unit    # Unit only (fastest)
make test-nextest-docker  # Docker integration
make test-nextest-smoke   # Serial smoke tests
make test-nextest         # Full suite (pre-PR gate)
make release-check        # Full quality gate
```

## Code Quality

- `cargo fmt --all` — rustfmt, enforced in CI
- `cargo clippy --all-targets -- -D warnings` — zero warnings
- `ast-grep` (`sg`) — structural code search/refactoring

## CI/CD (GitHub Actions)

- `ci.yml` — lint (fmt + clippy) → test-fast (Ubuntu + macOS-14) → test-integration (Ubuntu + Docker + GHCR token)
- `coverage.yml` — llvm-cov with threshold
- `release.yml` — cross-compiled: Linux (x86_64/arm64/musl), macOS (x86_64/arm64), Windows (x86_64/arm64)
- `semantic-pr.yml` — PR title linting
- `labeler.yml` / `sync-labels.yml` / `clean-branches.yml` — automation

## Container Runtimes

- **Docker** — default, production-ready
- **Podman** — in development (via `RuntimeKind` enum + `RuntimeFactory`)
- Detection: CLI flag > `DEACON_CONTAINER_RUNTIME` env > default

## AI Tooling

- **speckit** — spec-driven feature workflow (specify > clarify > plan > tasks > implement)
- `.specify/memory/constitution.md` — development principles
- `.claude/agents/` — rust-code-reviewer, spec-compliance-reviewer, speckit-rust-implementer, tech-debt-delegator
- **Maverick** — multi-provider agent config; `.maverick/runway/` for runtime state
- **Beads** — AI-native issue tracking; `.beads/` dir with Dolt database backend, CLI-first workflow
