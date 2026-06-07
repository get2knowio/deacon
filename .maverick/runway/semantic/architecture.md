## Workspace Layout

```
crates/deacon/   — CLI binary: argument parsing (clap), command orchestration, UI (indicatif)
crates/core/     — Library: 50+ modules for domain logic (config, runtime, OCI, features, lifecycle)
docs/subcommand-specs/*/SPEC.md  — Authoritative behavior specs (source of truth)
examples/        — Runnable demos with exec.sh scripts
.specify/memory/constitution.md  — Development principles and constraints
```

## Core Module Map (`crates/core/src/`)

| File | Responsibility |
|------|---------------|
| `config.rs` | DevContainer config resolution via json5 (JSONC), extends chain support, variable substitution |
| `container_env_probe.rs` | Shell env probing with caching; ContainerProbeMode (None/LoginShell/LoginInteractiveShell) |
| `container_lifecycle.rs` | Lifecycle phase execution; LifecycleCommandSource tracks feature vs config origin |
| `dockerfile_generator.rs` | Feature installation during Docker image build phase (BuildKit) |
| `docker.rs` | Docker client wrapper; validators for labels, image tags; PTY error detection |
| `runtime.rs` | `ContainerRuntime` trait = `Docker + ContainerOps + DockerLifecycle + Send + Sync` |
| `user_mapping.rs` | `updateRemoteUserUID` — user UID mapping (default: enabled, graceful failure) |
| `oci/` | OCI registry: auth, client, fetcher modules |
| `compose.rs` | Docker Compose integration with profile selection from `runServices` |
| `features.rs` | Feature types (OptionValue: Boolean/String/Number/Array/Object/Null), lifecycle aggregation |
| `feature_ref.rs` | Feature reference parsing: OCI refs, local paths, HTTPS tarballs |
| `lifecycle.rs` | Lifecycle command types and aggregation |
| `mount.rs` | Mount merging with feature precedence |
| `security.rs` | Security options merging |
| `errors.rs` | 462-line error taxonomy: ConfigError, DockerError, GitError, FeatureError, TemplateError, InternalError |
| `state.rs` | Container/Compose state tracking (includes ComposeState with profiles) |
| `build/` | Build domain: `mod.rs` (BuildOptions, request aggregation), `buildkit.rs` (BuildKit impl), `metadata.rs` (labels) |
| `cache/` | Multilevel caching: `keys.rs`, `memory.rs`, `disk.rs`, `multilevel.rs` (memory+disk strategy) |
| `env_probe.rs` | Refactored environment probing (host-level) |

## Key Abstractions

- **`ContainerRuntime` trait** (`runtime.rs`) — Composition trait; `RuntimeFactory` detects runtime from CLI flag > `DEACON_CONTAINER_RUNTIME` env > default docker
- **`HttpClient` trait** — OCI HTTP ops (HEAD=existence, GET=download, POST=auth); `ReqwestClient` production, `MockHttpClient`/`AuthMockHttpClient` for tests
- **`ConfigLoader::load_with_extends()`** — Full config resolution including all `extends` chains; never use single-file loading
- **`DockerfileGenerator`** — Features baked into build phase via BuildKit `FEATURE_CONTENT_SOURCE` context
- **`UserMappingService<T>`** — Async UID mapping; default enabled, graceful failure
- **`ContainerLifecycle`** — Lifecycle hooks; supports string, array (single exec), and object command formats
- **`resolve_env_and_user()`** — Shared helper for env probing with optional caching across up/exec/run-user-commands
- **`ComposeProject`** — Compose orchestration with `profiles` from `runServices`

## Command Structure

**Top-level** (`crates/deacon/src/commands/`):
- `up/` — 11+ files: args, compose, container, dotfiles, features_build, image_build, lifecycle, merged_config, ports, result, helpers
- `down.rs`, `exec.rs`, `read_configuration.rs`, `run_user_commands.rs`
- `templates.rs`, `build/` (mod.rs + result.rs), `outdated.rs`, `config.rs`

**Shared** (`commands/shared/`): config_loader, env_user, remote_env, terminal, workspace, progress

## In-Scope Commands (Consumer-Only)

`up`, `down`, `exec`, `build`, `read-configuration`, `run-user-commands`, `templates apply`, `doctor`

Feature authoring (`test`, `info`, `plan`, `package`, `publish`) permanently out of scope.

## Data Flow: `deacon up`

1. Parse CLI args (`commands/up/args.rs`)
2. Resolve config via `ConfigLoader::load_with_extends()` (full extends chain)
3. Merge feature security/mounts/lifecycle options
4. Generate Dockerfile with features via `DockerfileGenerator` (BuildKit build phase)
5. Create container via `ContainerRuntime` (hardened runArgs ordering)
6. Apply user UID mapping via `UserMappingService` (if enabled)
7. Probe container environment via `resolve_env_and_user()` (cached to `{cache_folder}/env_probe_{id}_{user}.json`)
8. Execute lifecycle hooks via `ContainerLifecycle`

For Compose mode: profiles from `runServices` forwarded via `--profile` flags.

## UI Layer (`crates/deacon/src/ui/`)

- `lifecycle_summary.rs` — Lifecycle command summary display
- `spinner.rs` — Progress spinner for long operations

## Issue Tracking

- `.beads/` — AI-native issue tracking (Dolt database backend, CLI-first for AI agents)

## Entry Point

`crates/deacon/src/main.rs` — 42 lines: clap parse → `Cli::dispatch()` async → special exit codes (2 for OutdatedExitCode or NotImplemented).
