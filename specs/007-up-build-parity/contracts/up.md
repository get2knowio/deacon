# Contract: `up` Build Parity and Metadata

## Purpose
Ensure the `up` subcommand respects BuildKit/cache-from/cache-to options for Dockerfile and feature builds, enforces skip-feature-auto-mapping with lockfile/frozen, and surfaces feature metadata in mergedConfiguration.

## Inputs (flags/config)

### Build Options
| Flag | Type | Description |
|------|------|-------------|
| `--cache-from` | repeatable string | External cache source (e.g., `type=registry,ref=<image>`). Order preserved. |
| `--cache-to` | optional string | External cache destination (e.g., `type=registry,ref=<image>`). |
| `--buildkit` | enum (auto\|never) | BuildKit usage control. `auto` respects `DOCKER_BUILDKIT` env var; `never` forces legacy build. Default: `auto`. |
| `--build-no-cache` | boolean | Build without using cache. Default: false. |

### Feature Controls
| Flag | Type | Description |
|------|------|-------------|
| `--skip-feature-auto-mapping` | boolean (hidden) | Blocks auto-added features; only explicitly declared features are resolved. |
| `--experimental-lockfile` | optional path (hidden) | Path to feature lockfile for validation. |
| `--experimental-frozen-lockfile` | boolean (hidden) | Require lockfile to exist and match config features exactly. Implies `--experimental-lockfile` if not specified. |

### Configuration Inputs
- Devcontainer configuration: resolved configuration (including extends/overrides) supplying Dockerfile, features, and order.
- Global `--workspace-folder`, `--config`, `--override-config`, `--secrets-file` options apply as with other subcommands.

## Processing Rules
1. **Build options propagation**: Apply provided `--cache-from`, `--cache-to`, and `--buildkit` options to both Dockerfile builds and feature builds via `BuildOptions` struct.
2. **BuildKit detection**: When `--buildkit=auto` (default), detect BuildKit availability. Cache options require BuildKit; if unavailable and cache options are specified, log a warning but proceed with legacy build.
3. **Cache endpoint handling**: If cache endpoints are unreachable, continue build with warnings while honoring provided options where possible.
4. **Skip feature auto-mapping**: When `--skip-feature-auto-mapping` is enabled, only explicitly declared features are resolved/built; CLI-provided features via `--additional-features` are ignored with an info log.
5. **Lockfile validation**: When `--experimental-frozen-lockfile` or `--experimental-lockfile` is specified:
   - Derive lockfile path from explicit option or default location (`devcontainer-lock.json` alongside config).
   - Validate features against lockfile before any build steps.
   - In frozen mode: halt with error on mismatch or missing lockfile.
   - In non-frozen lockfile mode: warn but continue on mismatch.
6. **Order preservation**: Maintain declaration order for features and caches when invoking builds and when emitting mergedConfiguration.

## Outputs

### Text Mode
Human-readable build progression reflecting:
- Applied build options (cache-from, cache-to, buildkit mode)
- Enforcement decisions (skip-feature-auto-mapping, lockfile validation)
- Feature metadata inclusion outcomes
- Errors emitted clearly to stderr

### JSON Mode
stdout contains the result JSON including:
- `containerId`: Running container identifier
- `remoteUser`: Effective remote user
- `remoteWorkspaceFolder`: Workspace folder path inside container
- `configuration` (if `--include-configuration`): Original resolved configuration
- `mergedConfiguration` (if `--include-merged-configuration`): Final merged configuration including:
  - All resolved features with their metadata (empty object when none)
  - Preserved declaration order from user config

All logs and diagnostics go to stderr only.

## Exit Codes
| Code | Condition |
|------|-----------|
| 0 | Success; build options applied and metadata merged as required. |
| 1 | Fail-fast conditions: lockfile missing/mismatch in frozen mode, configuration validation errors, build failures. |

Exit codes are consistent across output modes per spec.

## Edge Behaviors
- **Features lacking metadata**: Still appear in mergedConfiguration with empty metadata entries (`"metadata": {}`).
- **Build option conflicts**: Unsupported combinations cause immediate failure before build steps with descriptive error.
- **Cache reachability**: Issues do not change exit code unless they prevent build from running; warnings are emitted about degraded caching.
- **No cache options**: Default build behavior remains unchanged when no cache/buildkit options are specified.
- **BuildKit unavailable with cache options**: Logs warning and proceeds without cache support rather than failing.
