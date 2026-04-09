## Common Anti-Patterns

| Anti-Pattern | Description | Where to Look |
|---|---|---|
| Data structure mismatch | `Vec` when spec defines `map<string, T>` | Config deserialization, feature types |
| Incomplete resolution | Loading top-level config only, skipping `extends` chains | Any command using config |
| Silent fallbacks | Passing invalid input downstream instead of filtering at ingress | CLI arg parsing, config loading |
| Ordering violations | `BTreeMap` where spec requires declaration order (use `IndexMap`) | Config fields, feature maps |
| Exit code gating | Special exit codes honored only in one output mode | All subcommands with JSON output |
| Mock in runtime | Test fakes in production code paths | Container runtime |
| Blocking in async | `std::process::Command` inside `async fn` | Any new async command |
| Missing nextest config | Integration tests without `.config/nextest.toml` entries | New test files |
| GET for existence | OCI blob existence must use HEAD, not GET | OCI client code |
| Missing mock updates | Changing `HttpClient` trait without updating all mock impls | OCI trait changes |

## Hotspot Files

- `crates/core/src/features.rs` — Feature types, security/mount/lifecycle merging
- `crates/core/src/lifecycle.rs` / `container_lifecycle.rs` — Lifecycle formats and execution
- `crates/core/src/config.rs` — Config resolution and merge rules (bugs in PR #14)
- `crates/core/src/docker.rs` — Docker client, runArgs ordering (recently hardened)
- `crates/core/src/dockerfile_generator.rs` — Feature installation in build phase
- `crates/deacon/src/commands/up/` — Complex 11-file orchestration submodule
- `.config/nextest.toml` — Updated with each new integration test

## Change Categories (from git history)

1. **Spec compliance fixes** — Merge rules, data shapes, exit codes (PRs #13, #14, #15)
2. **Lifecycle format support** — String/array/object commands, aggregation
3. **Feature installation** — Moved from runtime to build phase via `DockerfileGenerator`
4. **User mapping** — `updateRemoteUserUID` per spec (default: true, graceful failure)
5. **Compose integration** — Profile selection from `runServices`
6. **Docker hardening** — runArgs passthrough ordering
7. **Scope reduction** — Removing feature-authoring commands (#11)
8. **Shared abstractions** — `CliRuntime` rename, shared helpers, module splits
9. **Refactoring cycles** — CliDocker → CliRuntime naming, indicating runtime layer uncertainty
10. **Build domain extraction** — First-class build module with BuildKit support and metadata
11. **Multilevel caching** — Memory+disk cache infrastructure for performance
12. **Named config search** — Config discovery by name (spec 014)
13. **Beads issue tracking** — AI-native issue tracking initialized

## Risk Areas

### Config Resolution
- Must use `ConfigLoader::load_with_extends()`, never single-file loading
- `ConfigMerger` rules for booleans/arrays are subtle (fixed in PR #14)
- `extends` chain ordering affects final merged values

### Lifecycle Command Handling
- Three formats: string, `string[]` (single exec command, NOT multiple), object `{command, env}`
- Feature lifecycle aggregated and run after container lifecycle
- `LifecycleCommandSource` tracks origin (feature with ID vs config)

### Feature Installation (Build Phase)
- Features install during Docker build via `DockerfileGenerator`, NOT in running containers
- Level-by-level installation per plan; BuildKit `FEATURE_CONTENT_SOURCE` context
- Feature options passed as ENV vars in Dockerfile
- `OptionValue` enum extended to support Number/Array/Object/Null (backward compatible)

### Docker runArgs Ordering
- runArgs placed after Deacon flags and before image name
- Compose mode correctly ignores runArgs

### Container Environment Probe Caching
- Cache key: container ID + user → auto-invalidates on container change
- `--container-data-folder` flag threads through to enable caching
- Best-effort: cache failures fall back gracefully
- Shell selection: prefer $SHELL → /etc/passwd → try zsh → bash → sh

### JSON Output Mode
- stdout = ONLY JSON document; all `tracing` to stderr
- Exit code contracts apply regardless of output mode

## Pre-PR Checklist

1. Spec alignment verified (`docs/subcommand-specs/*/SPEC.md`)
2. Data structures match spec shapes (map vs vec, field ordering)
3. Full extends chain resolution used
4. Input validated/filtered at ingress per spec
5. Exit codes correct in all output modes
6. Spec-mandated tests implemented
7. New integration tests in nextest groups in ALL profiles
8. `cargo fmt --all` and `cargo clippy --all-targets -- -D warnings` pass

## Deferral Tracking

- Document in `research.md` with numbered decisions
- Add to `tasks.md` under `## Deferred Work` with `[Deferral]` tag
- Spec NOT complete while deferred tasks remain
