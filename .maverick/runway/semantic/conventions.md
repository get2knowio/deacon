## Error Handling

- **`thiserror`** for domain errors in `crates/core` — structured enums with `#[error("...")]` messages
- **`anyhow`** only at binary boundary in `crates/deacon` — always add `.context("descriptive message")`
- **Never** `unwrap()` or bare `expect()` in runtime paths — propagate with `Result`
- Validate/filter invalid inputs at ingress per spec; don't pass invalid state downstream
- Error enums: `ConfigError`, `DockerError`, `GitError`, `FeatureError`, `TemplateError`, `InternalError` → wrapped by `DeaconError`

```rust
// Core (thiserror)
#[derive(Debug, thiserror::Error)]
pub enum ConfigError { ... }

// Binary (anyhow)
config.load().context("failed to load devcontainer.json")?;
```

## Async Safety

- Never call blocking APIs (`std::process::Command::output`, blocking file IO) inside `async fn`
- Use `tokio` async equivalents or `tokio::task::spawn_blocking` for CPU-heavy work
- All container operations are async

## Imports Organization (rustfmt enforced)

1. `use std::...`
2. `use external_crate::...`
3. `use crate::...` / `use super::...`

## Naming Conventions

- Files: `snake_case.rs`; command files match CLI names: `read_configuration.rs`
- Structs/Traits/Enums: `PascalCase`; Functions/variables: `snake_case`
- Integration test files: `tests/integration_<area>.rs`
- Feature flags: `default` for full CLI, `--no-default-features` for MVP

## Logging

- `tracing` spans for workflows: `config.resolve`, `feature.install`, `lifecycle.run`
- Structured fields: `tracing::debug!(container_id = %id, "starting")`
- `DEACON_LOG` / `RUST_LOG` env vars; JSON mode: `--log-format json` (logs to stderr)

## Output Streams Contract

| Mode | stdout | stderr |
|------|--------|--------|
| JSON (`--output json`) | Single JSON document only | All logs/diagnostics |
| Text (default) | Human-readable results | All logs/diagnostics |

## Testing Patterns

- **Unit tests**: `#[cfg(test)] mod tests` in same file, pure logic only
- **Integration tests**: `crates/core/tests/integration_*.rs`, fixtures/mocks (no network)
- **Doctests**: must compile — include required trait imports and `Default` impls
- `MockHttpClient` / `AuthMockHttpClient` for OCI registry tests

## Nextest Test Groups (`.config/nextest.toml`)

| Group | Threads | Use When |
|-------|---------|----------|
| `docker-exclusive` | 1 | Exclusive Docker daemon access |
| `docker-shared` | 8 | Safe concurrent Docker usage |
| `docker-slow-shared` | 2 | Heavy Docker tests with overlap |
| `fs-heavy` | 4 | Significant filesystem ops |
| `env-probe` | 12 | Host environment probing only |
| `long-running` | 2 | End-to-end build tests |
| `smoke-lite/heavy/cli` | 4/2/8 | Smoke tests by weight |
| `parity/parity-cli` | 2/8 | Upstream CLI comparison |

**Profiles**: dev-fast (no docker/smoke), default, full, docker, ci, mvp-integration

When adding integration tests, add override rules to ALL profiles in `.config/nextest.toml`.

## OCI HttpClient Trait

- HEAD requests for blob/manifest existence (never GET for existence)
- GET for downloading blobs and manifests
- When changing `HttpClient` trait, update ALL implementations

## Examples Hygiene

- Every `examples/*/` has `exec.sh` running all README scenarios
- Scripts clean up all resources; pin images to specific versions (`alpine:3.18`)

## Code Search

Use `sg` (ast-grep) for structural code search/rewrite over `grep`/`rg` for Rust patterns.
