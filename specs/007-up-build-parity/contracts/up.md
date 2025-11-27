# Contract: `up` Build Parity and Metadata

## Purpose
Ensure the `up` subcommand respects BuildKit/cache-from/cache-to/buildx options for Dockerfile and feature builds, enforces skip-feature-auto-mapping with lockfile/frozen, and surfaces feature metadata in mergedConfiguration.

## Inputs (flags/config)
- Build options: cache-from (repeatable), cache-to (repeatable), buildx/builder selection, BuildKit enablement as supported by environment.
- Feature controls: skip-feature-auto-mapping toggle, lockfile path, frozen mode flag.
- Devcontainer configuration: resolved configuration (including extends/overrides) supplying Dockerfile, features, and order.

## Processing Rules
- Apply provided BuildKit/cache-from/cache-to/buildx options to both Dockerfile builds and feature builds; reject runs if requested builder/BuildKit is unavailable.
- If cache endpoints are unreachable, continue build with warnings while honoring provided options where possible.
- When skip-feature-auto-mapping is enabled, only explicitly declared features are resolved/built; templates/defaults do not add features.
- Under lockfile + frozen, any mismatch or missing lockfile halts before build with a clear error describing the discrepancy.
- Maintain declaration order for features and caches when invoking builds and when emitting mergedConfiguration.

## Outputs
- Text mode: human-readable build progression reflecting applied build options, enforcement decisions, and metadata inclusion outcomes; errors emitted clearly to stderr.
- JSON mode: stdout contains only the mergedConfiguration (including build_options_applied, enforcement markers, and feature metadata); logs to stderr only.

## Exit Codes
- 0: Success; build options applied and metadata merged as required.
- Non-zero: Fail-fast conditions (unsupported BuildKit/buildx, lockfile missing/mismatch in frozen mode, other validation errors); ensure exit codes are consistent across output modes per spec.

## Edge Behaviors
- Features lacking metadata still appear in mergedConfiguration with empty metadata objects.
- Build option conflicts or unsupported combos cause immediate failure before build steps.
- Cache reachability issues do not change exit code unless they prevent build from running; they must emit warnings about degraded caching.
