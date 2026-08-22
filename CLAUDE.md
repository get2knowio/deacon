# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What is Deacon?

Deacon is a Rust implementation of the Development Containers CLI, following the [containers.dev specification](https://containers.dev). It provides DevContainer lifecycle management including configuration resolution, feature installation, template scaffolding, and container orchestration.

## Core Architecture

**Workspace Structure:**
- `crates/deacon/` - CLI binary crate (argument parsing, command orchestration, UI)
- `crates/core/` - Core library with domain logic (config parsing, container runtime, OCI registry, features/templates)
- upstream [devcontainers/spec](https://github.com/devcontainers/spec) (pinned commit `113500f4`) - authoritative CLI behavior; `parity/` holds the scenarios, fixtures and allowlist that record deacon's conformance against it (see the Differential Parity Suite section)
- `.specify/memory/constitution.md` - Development constitution defining principles and constraints
- `examples/` - Executable examples with `exec.sh` scripts demonstrating features

**Key Abstractions:**
- `ContainerRuntime` trait - Docker/Podman abstraction for container operations
- `HttpClient` trait - OCI registry communication (reqwest-based, consumer-side: HEAD/GET for pulls, POST for auth)
- `ConfigLoader` - DevContainer configuration resolution with extends chains
- `FeatureInstaller` - OCI feature installation and dependency resolution
- `ContainerLifecycle` - Lifecycle command execution (onCreate, postCreate, postStart, etc.)
- Container environment probe with caching - 50%+ latency improvement via `probe_container_environment()`

## Critical Development Principles

**1. Spec-Parity as Source of Truth**
- ALL behavior MUST align with the upstream [devcontainers/spec](https://github.com/devcontainers/spec) repository (commit `113500f4`, October 2025) — the single source of truth. Deacon's conformance against it, including every characterized divergence, is recorded in the repository-owned `parity/` data root (see the Differential Parity Suite section), not in prose.
- Data structures MUST match spec shapes exactly (map vs vec, field ordering, null handling)
- Configuration resolution MUST use full extends chains via `ConfigLoader::load_with_extends`
- Never implement shortcuts that deviate from spec-defined algorithms

**2. Keep the Build Green (Non-Negotiable)**
Run after EVERY code change:
```bash
cargo fmt --all && cargo fmt --all -- --check  # Format immediately
cargo clippy --all-targets -- -D warnings      # Zero tolerance
```

Development loop options:
- Fast loop (default): `make test-nextest-fast` - unit/bins/examples + doctests, excludes docker/smoke
- Targeted: `make test-nextest-unit` (unit only), `make test-nextest-docker` (docker integration)
- Full gate (before PR): `make test-nextest` - complete parallel test suite

**Fix All Failures - Even Unrelated Ones:**
If you encounter build or test failures during CI or local testing, fix them even if they're unrelated to your current work. A broken build blocks everyone. Never defer failures to "fix later" - address them before completing your current task. This includes:
- Pre-existing test failures discovered during your work
- Flaky tests that fail intermittently
- Lint or format issues in files you didn't modify
- Documentation or doctest compilation errors

**3. Consumer-Only Scope**
- Deacon implements only the consumer surface of the DevContainer spec
- In-scope commands: `up`, `down`, `exec`, `build`, `read-configuration`, `run-user-commands`, `templates apply`, `doctor`
- Feature authoring (test, info, plan, package, publish) is permanently out of scope
- The feature *installer* (fetching/installing OCI features during `up`) is consumer functionality and stays

**4. No Silent Fallbacks - Fail Fast**
- Production code MUST emit clear errors when capabilities are unavailable
- Mocks/fakes are ONLY for tests, never in runtime code paths
- Filter invalid inputs at ingress per spec (e.g., only OCI refs, only semver tags)
- Never swallow errors with unwraps or sentinel values - always propagate with `Result` and `.context()`

**5. Panic-Free, Async-Safe Implementations**
- Runtime code MUST NOT panic on expected failures: replace `unwrap`/unchecked `expect` with fallible paths and
  contextual errors.
- Async code MUST avoid blocking calls (`std::process::Command::output`, blocking file IO). Use `tokio` async
  equivalents with streamed output or offload to bounded blocking tasks.
- Prefer modular boundaries over monoliths: split large commands/clients into focused modules (e.g.,
  `up` `{args,compose,lifecycle,merged_config}`, `oci` `{auth,client,fetcher}`,
  `shared` `{config_loader,env_user,remote_env,terminal}`) and reuse shared helpers.

**6. Subcommand Consistency & Shared Abstractions**
When multiple subcommands share behavior (terminal sizing, config resolution, container targeting, env probing), use shared helpers:
- `resolve_env_and_user()` - Container environment probing with cache support
- `ConfigLoader::load_with_extends()` - Full configuration resolution
- Terminal/remote-env helpers in `commands/shared/`
- See `docs/ARCHITECTURE.md` for cross-cutting patterns (env probe caching, etc.)

## Common Development Tasks

**Build & Run:**
```bash
cargo build --release              # Production build
cargo run -- --help                # Run CLI
cargo run -- up                    # Start a devcontainer
cargo run -- read-configuration    # Parse devcontainer.json
```

**Testing Strategy:**
```bash
# Fast feedback loop (default during development)
make test-nextest-fast            # Unit/bins/examples + doctests (excludes docker/smoke)

# Targeted testing by area
make test-nextest-unit            # Super fast unit tests only
make test-nextest-docker          # Docker integration tests
make test-nextest-smoke           # High-level smoke tests

# Full validation (before PR)
make test-nextest                 # Complete parallel suite with all tests
```

**Test Groups** (configured in `.config/nextest.toml`):
- `docker-exclusive` (serial) - Exclusive Docker daemon access required
- `docker-shared` (parallel-4) - Safe concurrent Docker usage
- `fs-heavy` (parallel-4) - Significant filesystem operations
- `long-running` (serial) - Heavy end-to-end tests
- `smoke` (serial) - High-level integration tests
- `parity` (serial) - Upstream CLI comparison

**When adding new integration tests:**
1. Identify resource requirements (docker exclusive vs shared, filesystem heavy, etc.)
2. Add override rules to ALL profiles in `.config/nextest.toml`. A new docker-exclusive
   test BINARY must be added in 3 spots (mirror `run_user_commands_prebuild`): the
   `[profile.default]` override filter, the `[profile.dev-fast]` `default-filter`
   exclusion, and the `[profile.dev-fast]` override filter. When two in-flight PRs add
   binaries to the same filter line, expect a `nextest.toml` conflict — resolve to the
   UNION of both `binary(=…)` clauses.
3. Prefer most permissive group that ensures correctness (docker-shared over docker-exclusive when safe)
4. Verify with `make test-nextest` to ensure no race conditions. Tip: `cargo nextest run <substr>`
   filters by TEST NAME; to target a binary use a filterset: `cargo nextest run -E 'binary(=NAME)'`.
   Docker-gated tests that build feature images should assert the artifact is in the produced
   image (`docker run <tag> cat <marker>`), not just the JSON `outcome`.

**Code Quality:**
```bash
cargo fmt --all                   # Format code (run after EVERY change)
cargo clippy --all-targets -- -D warnings  # Lint with zero tolerance
make dev-fast                     # Fast loop: fmt + clippy + fast tests
make release-check                # Full quality gate
```
Before pushing, run the FULL workspace gate — `cargo fmt --all -- --check` and
`cargo clippy --all-targets --all-features -- -D warnings`. Running clippy with only
`-p deacon` (or skipping `--all-features`) misses fmt drift in new test files and lints in
`deacon-core`, which then fail the CI `Lint (fmt + clippy)` job.

**Running Single Tests:**
```bash
# With cargo-nextest (faster, parallel)
cargo nextest run test_name
cargo nextest run 'test(integration_)*'

# Traditional cargo test (serial)
cargo test test_name -- --test-threads=1
```

**Cross-Cutting Audits:**

After fixing N concrete bugs that share a pattern, dispatch parallel `Explore` agents
(one per pattern axis) to find more instances. The agents return precise file:line
pointers without polluting the main context. PRs #125 (substitution gaps) and #127
(local-feature dispatch) were direct outputs of this approach.

Pattern axes worth re-running periodically:
- Variable substitution coverage (any new `String` field on `DevContainerConfig` /
  `FeatureMetadata` should be checked against `apply_variable_substitution`).
- Workspace-folder resolution (any subcommand reading `--workspace-folder` should match
  `up`'s pattern: keep user path for config + identity, walk git-root only for mount source).
- State markers (any container reset / replace operation should clear markers).
- Stdio contract (any `silent: false` `ExecConfig` on the `up` flow should set
  `stdout_to_stderr: true`).
- Local-feature dispatch (any code iterating feature IDs should handle `./`, `../`,
  `/abs/path` prefixes before falling through to OCI parsing). An absolute path is
  classified LOCAL on purpose but is then REJECTED, not resolved (#495 reversed the #126
  capability on spec authority): keeping the classification is what routes it to
  `resolve_local_feature_dir`'s accurate diagnostic instead of a misleading registry 404.
- Feature resolution (any subcommand that needs resolved features should reuse
  `commands/shared/feature_resolver::resolve_features_ordered` rather than re-implementing
  the local/OCI + dependency-order loop; `read-configuration` keeps its own richer variant
  for `--additional-features`/auto-mapping/registry grouping).
- Config-relative vs workspace-relative paths: BOTH `dockerComposeFile` and local feature
  paths (`./…`) resolve against the **config dir** (`config_path.parent()` or
  `<workspace>/.devcontainer`) — the spec says `dockerComposeFile` is "relative to the
  `devcontainer.json` file" (`parity/spec/113500f4/devcontainerjson-reference.md:56`), so
  for the standard layout it is the `.devcontainer` dir, NOT the workspace folder
  (`ComposeManager::create_project`, which cites the spec). What IS workspace-relative on
  that same path: the compose project name and working dir, both derived from `base_path`.
  A plain `devcontainer.json` at the workspace root is NOT a discovery location.

VERIFY agent claims empirically before filing — the workspace-folder audit produced two
false positives this session (see "Verified Non-Bugs" below). Auditing also surfaces the
opposite: several tracked deferrals (005 T022–T026, read-config container reading #268, the
mergedConfiguration `inspect_image` labels) were already implemented — confirm against
current code before "implementing" a deferral.

## Code Patterns & Style

**Error Handling:**
- `thiserror` for domain errors in core
- `anyhow` only at binary boundaries with `.context()` for diagnostics
- Never use `unwrap()`/unchecked `expect` in runtime paths; propagate with `Result` and context
- Avoid blocking calls inside async functions; prefer `tokio` async IO or spawn bounded blocking tasks

**Misc durable patterns:**
- Files that can be written by concurrent processes/threads (e.g. the disk-cache
  `index.json`) MUST be written atomically: serialize to a unique temp file then
  `fs::rename` into place. A plain `fs::write` truncates-then-streams and a shorter
  payload over a longer file leaves trailing bytes → flaky "trailing characters" JSON
  parse errors. See `cache/disk.rs::save_index`.
- `exec`'s positional `command: Vec<String>` uses `#[arg(trailing_var_arg, allow_hyphen_values)]`
  so `deacon exec node --version` passes `--version` to the command, not clap. Any new
  subcommand that runs an arbitrary user command should do the same.
- To extend a trait used by many impls/runtimes with a new method, give it a default impl
  that delegates to an existing method (e.g. `Docker::exec_with_line_prefix` defaults to
  `exec`). Mocks and delegating runtimes then need no change; only override where the new
  behavior matters (and in the enum/wrapper runtimes that must forward it).
- **Flag-backed env vars use clap's native `env=`, never a hand-rolled `std::env::var`
  read.** A CLI flag that is also settable via the environment MUST declare it on the
  `#[arg(...)]` (`env = "DEACON_<SCREAMING_FLAG>"`), so clap owns the precedence
  (flag > env > default) and `--help` advertises `[env: …]` for free. Do NOT read the same
  var by hand below the CLI layer — resolve at the `Cli` struct and thread the value down
  (see `--runtime`/`DEACON_CONTAINER_RUNTIME`, `--log-level`, `--no-redact`,
  `--override-config`; #180). `doctor`'s env snapshot in `core::doctor` is a *diagnostic*
  read (it reports which vars are set) and is the one legitimate exception — keep its list
  in sync with the canonical names actually consumed. Env-only knobs with **no** backing
  flag (`DEACON_NO_PROMPT`, `DEACON_CACHE_DIR`, `DEACON_LOG`/`RUST_LOG`, …) stay manual.

**Logging:**
- Use `tracing` spans for workflows: `config.resolve`, `feature.install`, `lifecycle.run`
- Structured fields over string concatenation
- Respect `DEACON_LOG` / `RUST_LOG` environment variables
- JSON logging mode: `--log-format json` (all logs to stderr, results to stdout)

**Imports Organization** (rustfmt enforces):
1. Standard library (`use std::...`)
2. External crates (`use serde::...`)
3. Local modules (`use crate::...`, `use super::...`)

**Testing Requirements:**
- ALL spec-mandated tests MUST be implemented (output formats, exit codes, edge cases)
- Unit tests for pure logic, integration tests for runtime boundaries
- Deterministic and hermetic (no network) - use fixtures and mocks
- Doctests MUST compile with proper trait imports and Default implementations

**Examples Hygiene:**
- Every `examples/*/` directory MUST have `exec.sh` that runs all README scenarios
- Scripts MUST clean up all resources (containers, images, volumes)
- Pin images to specific versions (e.g., `alpine:3.18` not `latest`)
- Keep README and `exec.sh` in lockstep

**Canary Patterns (`examples/*/exec.sh`):**
- **Cross-session memory: `examples/CANARY_STATUS.md`.** Before re-running canaries, check it — `✅` rows verified at the current `main` rarely need re-running; focus on `❓`/changed/`✗` areas. After running any canary, update its row (status + date + short commit). It also records which canaries are known `⚠️ fixture` (won't pass as-is, not a deacon bug) or `🚫 deferred`, so they aren't re-investigated each session.
- Config file MUST be at `.devcontainer.json` (root) or `.devcontainer/devcontainer.json`. Plain `devcontainer.json` at workspace root is NOT a spec discovery location and will fail config load.
- When parsing `deacon up` stdout as JSON, use `python3 -c '...'`, NOT `python3 - <<'PY'`. The heredoc collides with the printf pipe for stdin and python ends up parsing its own script as JSON.
- Don't `2>&1` inside `$( ... )` if you need to parse stdout — the result JSON is on stdout only; capturing stderr alongside breaks the parse.
- When wrapping a command with env vars (e.g. `COMPOSE_PROFILES`), use `env VAR=value cmd`, not `wrapper VAR=value cmd` — the wrapper function sees `VAR=value` as a positional arg.
- A feature's `install.sh` with `#!/usr/bin/env bash` fails on `alpine` bases (no bash, exit 127). Feature canaries should use a bash-capable base (`debian:bookworm-slim`); this is an example-fixture fix, not a deacon bug.
- Running canaries INSIDE this monorepo: `up` mounts the **git root** (`/workspaces/deacon`) to the container `workspaceFolder` (intended `--mount-workspace-git-root` default), so files the example keeps next to its config land under `/workspace/examples/<area>/<name>/…`, not `/workspace/…`. Pass `--mount-workspace-git-root false` on the example's `up` to mount the workspace folder directly. Not a deacon bug.

## Code Search & Refactoring with ast-grep

Use ast-grep (command: `sg`) for searching and rewriting code instead of `find`, `grep`, or regex-based tools.
ast-grep operates on Abstract Syntax Trees (AST), enabling precise pattern matching that respects language syntax.

**When to use ast-grep:**
- Searching for specific code patterns (function calls, struct definitions, trait implementations)
- Refactoring code at scale (renaming, restructuring, migrating APIs)
- Finding usages that regex would miss or over-match
- Enforcing code conventions or detecting anti-patterns

**Basic usage:**
```bash
# Search for a pattern in Rust files
sg --pattern 'unwrap()' --lang rust

# Search for function definitions
sg --pattern 'fn $NAME($$$ARGS) -> $RET { $$$BODY }' --lang rust

# Rewrite code (dry-run by default)
sg --pattern 'println!($$$ARGS)' --rewrite 'tracing::info!($$$ARGS)' --lang rust

# Apply rewrites
sg --pattern 'old_fn($ARG)' --rewrite 'new_fn($ARG)' --lang rust --update-all
```

**Pattern syntax:**
- `$NAME` - Single metavariable (matches one AST node)
- `$$$ARGS` - Variadic metavariable (matches zero or more nodes)
- Patterns match AST structure, not text—whitespace and formatting are irrelevant

**Best practices:**
- Always specify `--lang rust` for Rust codebases
- Test patterns with search before applying rewrites
- Use `--interactive` for selective rewrites
- Prefer ast-grep over regex for any structural code transformation
- For complex refactors, write YAML rules in `sgconfig.yml`

## OCI Registry Implementation (Consumer-Side)

**HTTP Client Trait Pattern:**
- Use HEAD requests to check blob/manifest existence (not GET - avoids downloading)
- Use GET for downloading blobs and manifests during feature installation
- When modifying `HttpClient` trait, update ALL implementations: `ReqwestClient`, `MockHttpClient`, `AuthMockHttpClient`, etc.

**Common Pitfalls to Avoid:**
- Don't use GET to check blob existence (wastes bandwidth)
- Don't forget to update test mocks when changing trait methods
- Do test with realistic mock responses matching OCI distribution spec

## Workspace-Trust Gate (Host-Side Lifecycle Hooks)

`initializeCommand` runs **on the developer's host** before any container
sandboxing. Any code path that wants to exec host shell from a
workspace-resident source (e.g. `devcontainer.json`, a future dotfiles
config in the workspace) MUST go through `crates/core/src/trust.rs`.

**Resolution order** (from `cli.rs` global flags → `core::trust::resolve_policy`):

1. `--trust-workspace` or `--trust-workspace-persist` → `AlwaysAllow`
   (persist also records to `{user_data_folder}/trusted_workspaces.json`).
2. `DEACON_NO_PROMPT=1` → `Deny` (CI fail-closed).
3. Default → `Allowlist({user_data_folder}/trusted_workspaces.json)`;
   pass only if the canonicalized workspace path is in the store.

On deny, return `DeaconError::WorkspaceUntrusted { workspace, reason,
instructions }`. The display string already names the workspace and the
opt-in flags — don't reformat at higher layers.

**Adding a new host-side exec site:**
1. Resolve the policy at the CLI tier (don't pass raw flags deeper).
2. Call `check_workspace_trust(&workspace, policy).await?`.
3. Convert via `decision_to_result(decision)?` — `Trusted` becomes
   `Ok(())`, `Denied` becomes `DeaconError::WorkspaceUntrusted`.
4. If the trust source is workspace-resident, set the appropriate
   `HostTrustSource` on the consumer struct (see `DotfilesPhaseConfig`).

**This gate is deacon-specific.** The upstream containers.dev spec does
not mandate it. See `SECURITY.md` for the threat model and end-user
documentation. Refusing to add the gate to a new host-side exec site is
a security regression — surface it in code review.

## Container Environment Probe Caching

**Architecture Overview:**
All subcommands that execute commands in containers use the shared `resolve_env_and_user()` helper with optional caching:

```rust
// Pass --container-data-folder to enable caching
let env_user = resolve_env_and_user(
    &docker_client,
    &container_id,
    cli_user,
    config_remote_user,
    probe_mode,
    config_remote_env,
    &cli_env_map,
    args.container_data_folder.as_deref(),  // Cache folder
).await?;
```

**Cache behavior:**
- Location: `{cache_folder}/env_probe_{container_id}_{user}.json`
- Performance: 10-50x speedup on cache hit (90-98% latency reduction)
- Invalidation: Automatic on container ID change
- Error handling: Best-effort with graceful fallback

**Implemented in:** `up`, `exec`, `run-user-commands`
**Future subcommands:** Any command executing lifecycle hooks should implement this pattern

See `docs/ARCHITECTURE.md` for implementation checklist and code references.

## Differential Parity Suite (`crates/parity-harness/`)

**Design, vocabulary and the two authorities live in [`docs/PARITY.md`](docs/PARITY.md)** —
what a divergence is, what a tolerance is and how one goes stale, the three oracle types,
the eleven observable channels and the normalization philosophy, what to do when you find a
difference, and the maintainer's standing rule on waivers. Read it once. This section is the
operational half: how the lanes run, how to verify a change locally, and the rules that have
cost time.

**Current state is `parity/SPEC_STATUS.md`; the current verdict is the latest nightly**
(`gh run list --workflow=parity.yml --branch=main`). Neither belongs in prose — a status
snapshot in a markdown file goes stale within the day. When a scenario teaches us something,
the `SPEC_STATUS.md` row changes in the same commit.

**The lanes**, split by what a case NEEDS rather than by what it is about — which is what
lets most of the suite gate every pull request:

| Binary | Needs | Runs where |
|---|---|---|
| `parity_hermetic` | nothing | every profile, `dev-fast` included; Linux, macOS and Windows (#441) |
| `parity_registry` | an OCI registry (no daemon, no oracle) | profiles whose JOB is provisioned for `ghcr.io`: `mvp-integration` (a required PR check with a GHCR token), `ci`, `docker`, `full`, `default`, the release gate — NOT `dev-fast` (#544) |
| `parity_docker` | Docker | wherever a daemon exists; the release gate |
| `parity_differential` | Docker + the pinned oracle | `--profile parity` ONLY (nightly) |
| `parity_harness_faults` | nothing | `default`, `dev-fast` — the hermetic guard proving the comparison can fail (oracle mismatch, timeout, normalization failure, injected difference) |

`driver::lane_of` derives a case's lane from THREE prerequisites — `oracleType` (the
reference), `resourceGroup` (the daemon) and `needsRegistry` (a registry) — so a lane
never *skips* a case it cannot run — a case it cannot run is in another lane, written down
rather than discovered at run time. Live parity runs ONLY under `cargo nextest run --profile
parity`; every other profile excludes it, so those lanes are truthful by non-selection — a
green fast run never implies parity ran. There is no env-var opt-in and no silent skip: a
missing oracle, absent Docker, or a normalization failure FAILS with a cause-specific
`HarnessError`. The nightly runs `--no-fail-fast` (its job is to enumerate a work queue, not
to stop at the first item) and gates nothing; the three no-oracle lanes DO gate —
`release.yml` runs `binary(=parity_hermetic) | binary(=parity_registry) |
binary(=parity_docker)` in its verify job.

**The registry axis is DERIVED from case data, never hand-listed** (#544). A case that
reaches `ghcr.io` declares `"needsRegistry": true` in its own record, exactly as a
Docker-backed case declares its `resourceGroup`; `driver::needs_registry` reads it and
`lane_of` routes on it. Adding one stays a pure data edit. Two rules travel with it:
re-laning is a **laning** change, so nothing a moved case asserts may be rewritten to fit
the new lane (`bhv-extends-feature-version-override` keeps its `git:1.3.2` → `git:1.3.8`
pin — that pin IS the behavior); and a Docker case that also reaches a registry stays in
`parity_docker`, since every job with a daemon has network.

**`parity_hermetic`'s "no network" is enforced, not promised.** Its own
`hermetic_lane_runs_without_a_network` re-runs the lane's drivers inside a fresh user +
network namespace (`unshare --map-root-user --net`). Reproduce a suspected leak with
`unshare -rn cargo nextest run -E 'binary(=parity_hermetic)' --no-fail-fast --offline`. The
guard is `#[cfg(target_os = "linux")]` — a visible non-selection off Linux, never a runtime
skip — and FAILS with a cause-specific message when the host cannot create a namespace. If
it goes red, move the case with `needsRegistry`; do NOT weaken what the case asserts.

**`.config/nextest.toml` gotcha, learned the hard way.** Editing the profiles once silently
dropped the `default-filter` from four of them, so `dev-fast` selected every Docker binary.
`tomllib` still reports a profile as *present* when only its `[[…overrides]]` blocks
survive — so "the profile exists" is NOT the check. After any edit, assert that **every**
profile carries a filter.

**Verifying locally.** Docker works in this dev container and the pinned oracle installs
cleanly, so verify parity changes for real rather than reasoning about them:

```bash
npm install -g @devcontainers/cli@0.87.0
cargo nextest run --profile parity
```

Docker etiquette, every line of it learned from a leak:

- Docker-backed cases run in isolated temp workspaces behind the `DockerWorkspace` RAII
  guard. A run cancelled partway leaves orphans that trip the next run's resource
  reclamation guard (`all predefined address pools have been fully subnetted`).
- Reclaim by removing containers whose `devcontainer.local_folder` label names a directory
  that no longer exists **and** containers/networks carrying the `deacon-conf-` name prefix.
  A label-only sweep is not enough: Compose sidecars (`deacon-conf-*-db-1`) carry no label
  and keep their network referenced, so `docker network prune` skips it and the leaked
  network trips the guard anyway.
- **Name-marker sweeps match case-INSENSITIVELY, and a case's workspace basename must be
  lowercase** (#442). Compose lowercases its project name and derives it from the workspace
  basename, so a mixed-case `tempfile` basename names resources that match nothing and the
  sweep looks like it is working while the leak stays. `DockerWorkspace::new` creates a
  lowercase child directory for exactly this reason.
- A guard that scans daemon-global state races sibling cases mid-teardown and reports a
  false leak (#442). Scope any new guard to the resources its own tier created.

**Growth rule: every wild bug becomes a scenario before it is fixed.** Adding one is a pure
data edit in `parity/cases/<area>.json` — no new Rust. Periodically mine the reference's own
e2e fixtures and real-world configs for scenarios the suite does not have yet.

**Rules that have cost time:**

- **Measure the divergence before sizing the fix.** A diverging path that names an array
  names one opaque blob, not one field: `compare.rs::diff_paths` diffs arrays wholesale, so
  filling the fields an issue happens to name can turn *zero* cases green.
- **A "deacon follows the spec, the CLI deviates" row is only as good as the spec text
  behind it.** Grep `parity/spec/113500f4/` before trusting one — on a spec-silent output
  surface the reference is the authority, and the difference is deacon's.
- **Case data can be environment-pinned** (host OS, Docker capabilities, uid assumptions,
  image pulls). The first run on a new substrate is a measurement, not a regression.
- **Before assuming a change made things worse**, diff your run's diverging case ids against
  a recent `main` nightly; identical sets mean your change is neutral.

## Pre-Implementation Checklist

Before implementing any new subcommand or feature:
1. Read complete spec (SPEC.md, data-model.md, contracts/)
2. Verify data structures match spec shapes exactly
3. Identify all spec-defined algorithms to implement precisely
4. Plan input validation and filtering per spec requirements
5. Ensure full config resolution with extends chains if needed
6. Verify JSON schema, ordering, and exit code contracts
7. List all spec-mandated tests to implement
8. Identify existing helpers/loaders to reuse (not reimplement)
9. Plan nextest test groups for new integration tests

Document this checklist in plan.md or PR description to prevent spec drift.

**When adding a `String` / `Option<String>` field to `DevContainerConfig` or
`FeatureMetadata`:**

1. Does it hold a user-template string (image ref, env value, path, command) or a fixed
   identifier (enum-like, service name)?
2. If template: add it to `apply_variable_substitution` AND
   `apply_variable_substitution_advanced` in `crates/core/src/config.rs`. Add a unit test
   mirroring `test_substitution_covers_image_and_compose_file`. If the field lives on
   `FeatureMetadata` instead, substitute at the read site (see PR #122 for the mount
   pattern and PR #125 for the entrypoint / container_env pattern — both build a
   `SubstitutionContext` anchored to the workspace folder with `devcontainerId` from
   `compute_dev_container_id(&identity.labels())`).
3. If fixed identifier: leave it out, and add a comment explaining why (see `wait_for`
   for the pattern).

**Object-shaped raw-JSON fields carrying user templates must ALSO be substituted.**
The checklist above is worded for `String`/`Option<String>` fields, but a
`serde_json::Value` field that holds user template strings (`build`, `mounts`,
`customizations`) must be recursively substituted via
`VariableSubstitution::substitute_json_value{,_with_options}` in BOTH passes — the
reference CLI resolves `${localWorkspaceFolder}` / `${localEnv:*}` /
`${containerWorkspaceFolder}` inside them (e.g. `customizations.vscode.settings`).
`customizations` was missed this way (#312). This is distinct from the
`#[serde(flatten)] extra` UNMODELED passthrough, which stays **verbatim** — modeled
object fields carrying templates are substituted; unknown top-level keys are not.

**Config validation philosophy (constitution IV — "strict on mistakes, faithful on the
unmodeled"):** two-sided, applied consistently.
- **Modeled fields fail fast on the developer's mistakes.** Typed fields already do this
  (e.g. `forwardPorts: "3000"` → type error). Object-shaped fields stored as raw JSON MUST
  match: `features` and `customizations` use `deserialize_object_value` to reject a non-object
  with a precise message. When you add a new object-shaped `serde_json::Value` field, wire it
  to that deserializer too — don't leave it lenient (inconsistent strictness is a defect).
- **Unmodeled fields are preserved, never dropped.** `DevContainerConfig` carries
  `#[serde(flatten)] pub extra: serde_json::Map<…>` so unknown / future / editor-specific
  top-level keys round-trip through `read-configuration` instead of being silently dropped
  (a fidelity loss vs the reference). It is merged (deep, overlay-wins) in `merge_two_configs`
  and preserved **verbatim** — NOT variable-substituted. Adding any field to
  `DevContainerConfig` forces updates to the `Default` impl and `merge_two_configs` (both
  exhaustive); the compiler flags the few non-`..Default` test literals (e.g. `plugins.rs`).
  Nested-struct unknown-field preservation (HostRequirements/PortAttributes/SecretMetadata)
  is a deferred follow-up (~43 exhaustive-literal edits); top-level + untyped passthrough
  (`build`, `gpu`) already cover the real forward-compat surface.
- **Every modeled optional is `Option<T>` + `skip_serializing_if`, including the
  collections** (#398/#400). `read-configuration` must emit an authored `"capAdd": []` and
  omit an unauthored one; a bare `Vec` cannot tell those apart, and a
  `skip_serializing_if = "Vec::is_empty"` on a bare field collapses the distinction from
  the *other* direction and makes the defect permanent. Read the flat view through the
  same-named borrowing accessor (`config.cap_add()` → `&[String]`); the field
  (`config.cap_add`) is the authored-or-not `Option`. When *writing*, materialize only on a
  real contribution — `get_or_insert_default()` inside a guard, never before an `extend`
  that may be a no-op, or you re-create a key nobody wrote. Merging goes through
  `combine_if_authored`, which stays `None` when neither side authored the property.
  `mergedConfiguration` is the exception and is SYNTHESIZED, not echoed: the reference
  materializes `containerEnv`/`remoteEnv`/`portsAttributes`/`hostRequirements` there and
  deacon omits them (`bhv-readconfig-merged-computed-empties-omitted`), which is why
  `drop_absent_optional` survives on that block alone.

**Dockerfile location parity:** the canonical containers.dev form is nested
`build.dockerfile`; the top-level `dockerFile` is legacy. Any code resolving the Dockerfile
must accept BOTH (see `extract_build_config` in `commands/build/mod.rs` and `up`'s
`image_build.rs`).

**Build feature installation (`deacon build`):** all four config shapes install features
and the user `--image-name` must resolve to the FEATURE-EXTENDED image, not the base.

**Never chain a build through a daemon-local tag** (#595). A `docker-container` driver
builder runs in an isolated BuildKit container that cannot read the daemon's image store,
so a second build whose `FROM` names a tag deacon just created can only run on the docker
driver. deacon used to force that with `--builder default`, which silently overrode the
builder the user selected and took OCI export, local cache export and multi-platform output
with it — and was also why a foreign `--platform` could not resolve its own base (#593).
The shape now, which is the reference CLI's own:
- Dockerfile / image-reference: ONE BuildKit invocation over ONE document —
  `base_stage_for_features` names the stage to build on (the config's `build.target`, else
  the final stage, aliased if unnamed), `prepare_feature_layer` emits the install stage, and
  `merge_dockerfile_with_feature_stage` splices them. An image-reference config synthesizes
  `FROM <image>` into a temp context first. Driven by `execute_single_container_build` →
  `execute_docker_build(..., &BuildOverlay)`.
- Because there is only one build, anything the BASE half needs must arrive on it:
  `build.args`, `build.options`, `--target`. A missing `--build-arg` here is invisible in
  the JSON outcome and shows up only in the image (`integration_up_dockerfile_features`).
- `devcontainer.metadata` is computed BEFORE the build, from the base image resolved out of
  the Dockerfile (`dockerfile_utils::resolve_base_image`, the reference's `findBaseImage`)
  plus one entry per Feature plus the config pick, and written as a `--label` on that same
  build. There is no stamping pass.
- The primary build runs `docker buildx build`, never `docker build`: the latter always uses
  the daemon's own "default" instance and ignores the selected builder, pin or no pin. Only
  `--buildkit never` keeps `docker build` (it is the only way to reach the legacy builder).
- `--output` rides the build. Only `--push` is deferred (single-platform builds load
  locally, so the push happens from the daemon afterwards; a multi-platform build cannot
  `--load`, so its push stays on the invocation).
- Compose: `execute_compose_build_with_features` resolves the service shape and reuses
  `up::compose::resolve_compose_feature_image` (shared with `up`), which builds the service's
  Dockerfile and the Feature stage as one document too. It still `retag_image`s the produced
  image with the deterministic tag + every `--image-name`; otherwise `--image-name` points at
  the pre-feature base and the installed features are invisible — and canaries that only
  check the JSON outcome (not image contents) won't catch it, so verify with
  `docker run <tag> cat <marker>`.
- **Never build a Compose service outside Compose** (#628, #629). A `build:` service's own
  `build:` keys — `args`, `labels`, `target`, and whatever is added next — apply only if
  Compose renders the service, so both the Features-free and the Features-declaring build run
  `docker compose build` and layer ONE inline override (`ComposeBuildOverlay::to_yaml`, piped
  through `-f -`) naming ONLY what deacon adds: the merged Dockerfile, `target`,
  `additional_contexts`, and the `devcontainer.metadata` label. This is the reference CLI's
  own shape. Forwarding the keys instead is what dropped `build.args` for a year with
  `outcome: success` on both sides — enumerate nothing, override narrowly. Settings Compose
  has no document key for (`--no-cache`, `--builder`) ride the invocation; ones it has no
  flag for (`cache_from`, `cache_to`, `platforms`) ride the document.
- `up` follows the same rule: a Dockerfile config that declares Features defers its base
  build (`deferred_dockerfile_build` in `up/mod.rs`) so `container.rs` can build base +
  Features together.

## Deferral Tracking

When implementing complex features in phases (MVP-first approach):

1. **Document deferrals in research.md** with numbered decisions explaining rationale
2. **Add deferred work to tasks.md** under a "## Deferred Work" section:
   - Reference the research.md decision number
   - Include specific acceptance criteria
   - Use `[Deferral]` tag in task description
3. **A spec is NOT complete** while deferred tasks remain unresolved

Example tasks.md entry:
```markdown
## Deferred Work

- [ ] T050 [Deferral] Thread resolved FeatureMetadata through flows per research.md Decision 6
  - **Decision**: Use from_config_entry() for MVP; from_resolved() when metadata available
  - **Rationale**: Requires architectural threading beyond MVP scope
  - **Acceptance**: featureMetadata includes version, name, description from resolved features
```

When reviewing PRs, verify research.md deferrals have corresponding tasks.md entries.

## Common Anti-Patterns to Avoid

- **Data Structure Mismatch**: Using `Vec` when spec defines `map<string, T>`
- **Incomplete Resolution**: Loading top-level config only, ignoring extends chains
- **Silent Fallbacks**: Passing invalid inputs to downstream logic instead of filtering
- **Ordering Violations**: Using `BTreeMap` when spec requires declaration order (use `Vec` or `IndexMap`)
- **Exit Code Gating**: Only honoring special exit codes in one output mode (spec applies to all)
- **Test Gaps**: Implementing features without spec-mandated tests
- **Missing Nextest Config**: Adding integration tests without configuring test groups
- **Suboptimal Test Grouping**: Using docker-exclusive when docker-shared would work
- **Untracked Deferrals**: Documenting deferrals in research.md without corresponding tasks.md entries

## Verified Non-Bugs (Don't Re-File)

Things that LOOK like bugs but aren't, with rationale. Adding here prevents the
rediscover-and-investigate loop:

- **`ContainerIdentity::hash_workspace_path` walks to git root unconditionally**
  (`crates/core/src/container.rs:152`). This is SYMMETRIC across `up`/`exec`/`down`/
  `run_user_commands`, so identities match. The exec bug #111 was elsewhere (exec walked
  the path BEFORE `load_config`, loading the wrong config). Confirmed empirically by
  running `up --workspace-folder X` then `down --workspace-folder X` — same workspace_hash.
- **`cap_add`, `security_opt`, `service`, `run_services`, `wait_for` not substituted.**
  These hold enum-like / fixed identifiers, not user template strings. Per #124
  acceptance criteria.
- **`down` doesn't clear `.devcontainer-state/<phase>.json` markers.** Intentional —
  markers should survive `down && up` for resume workflows. Markers are workspace-scoped,
  not container-scoped; they get cleared on `up --remove-existing-container` (per #117) —
  on BOTH the single-container path (`commands/up/container.rs`) and the compose path
  (`commands/up/compose.rs`, which now calls `clear_markers()` symmetrically).
- **deacon `read-configuration` rejects things the reference CLI accepts** (malformed
  JSONC, missing/cyclic `extends`, wrong-typed `features`/`forwardPorts`). The reference's
  `read-configuration` is a **lenient parse-and-echo**; deacon validates eagerly and
  strictly by design (constitution IV: fail fast). These are characterized, intended
  divergences, NOT bugs — recorded in the **conformance registry** (the authoritative
  source; don't re-state the per-case detail here) under the `read-configuration` area:
  `bhv-readconfig-malformed-jsonc-rejected`,
  `bhv-readconfig-wrong-type-{features,forwardports}-rejected` (all
  `intentional-divergence`), and the `extends` family
  `bhv-readconfig-extends-{missing,cycle}-rejected` / `bhv-readconfig-extends-merged`
  (`deacon-extension`, linked from `ext-extends-resolution`), each with a migrated
  `wvr-*` waiver. Conversely, deacon **preserving** unknown fields matches the reference
  (`bhv-readconfig-unknown-field-preserved`, `follow-spec`); silently dropping them WOULD
  be a bug. The differential runner (deleted in 023 US7: `parity_corpus_errors`) and its
  Tier 1c corpus are gone; these are now the declarative `case-errors-decl-*` cases in
  `parity/cases/`, driven by the parity lane binaries. Their tolerances resolve
  against `parity/ALLOWLIST.json`.
- **The three `bhv-readconfig-extends-*` behaviors carry `reference: divergent`, not
  `not-applicable`, and that is correct** — even though every other `deacon-extension`
  behavior uses `not-applicable`. The axis records whether the reference *has the surface*,
  not whether it has the feature: `not-applicable` means the reference CLI has no concept of
  the behavior at all (`down`, `doctor`, host CA injection, user profiles — no such
  subcommand or knob exists). The reference **does** have `read-configuration` and **does**
  process a config containing `extends`; it just echoes it literally instead of resolving.
  That is an observable difference on a surface both sides expose, so it is `divergent`, and
  RULES.md line "Backs which reference status: `divergent`" is why each one is waiver-backed.
  Do not "normalize" these three to match their siblings.

## Output Streams Contract

**JSON modes** (`--output json`, `--json`):
- **stdout**: Single JSON document only (newline terminated)
- **stderr**: All logs, diagnostics, progress via tracing

**Text modes** (default):
- **stdout**: Human-readable results only
- **stderr**: All logs, diagnostics, progress via tracing

**Examples:**
```bash
# JSON mode - safe parsing
deacon read-configuration --output json > config.json 2> logs.txt

# Text mode - human readable
deacon doctor > diagnosis.txt 2> logs.txt

# Parse JSON safely
OUTPUT=$(deacon read-configuration --output json 2>/dev/null)
echo "$OUTPUT" | jq '.configuration'
```

## Makefile Targets Reference

**Fast Development Loop:**
- `make dev-fast` - fmt + clippy + fast tests (recommended during iteration)
- `make test-nextest-fast` - Unit/bins/examples + doctests (excludes docker/smoke)

**Targeted Testing:**
- `make test-nextest-unit` - Unit tests only (super fast)
- `make test-nextest-docker` - Docker integration tests
- `make test-nextest-smoke` - High-level smoke tests

**Full Validation:**
- `make test-nextest` - Complete parallel suite (before PR)
- `make release-check` - Full quality gate (fmt + clippy + test + build)

**Utilities:**
- `make test-nextest-audit` - View test group assignments
- `make fmt` - Format all code
- `make clippy` - Lint with warnings as errors
- `make coverage` - Generate coverage report with llvm-cov

## Important Files & References

**Must-Read Documentation:**
- `.specify/memory/constitution.md` - Development principles and constraints
- `AGENTS.md` - Quick reference for AI assistants
- `parity/SPEC_STATUS.md` - Hand-maintained record of deacon's behavior vs the reference CLI; `parity/cases/` holds the scenarios behind it (see Differential Parity Suite)
- `docs/ARCHITECTURE.md` - Cross-cutting patterns (env probe caching, etc.)
- `.github/copilot-instructions.md` - Detailed development guidelines

**Key Implementation Files:**
- `crates/core/src/config.rs` - Configuration resolution with extends chains
- `crates/core/src/container_env_probe.rs` - Environment probing with caching
- `crates/core/src/container_lifecycle.rs` - Lifecycle command execution
- `crates/deacon/src/commands/shared/` - Shared command helpers

**Configuration Files:**
- `.config/nextest.toml` - Test parallelization and grouping
- `Cargo.toml` - Workspace configuration
- `Makefile` - Common development tasks

## Dependencies & Toolchain

**Active Technologies:**
- Rust 1.70+ (Edition 2021, stable toolchain)
- `clap` - CLI argument parsing
- `serde`/`serde_json` - Configuration and JSON handling
- `tracing`/`tracing-subscriber` - Structured logging
- `thiserror` - Domain errors (core)
- `anyhow` - Error context (binary)
- `tokio` - Async runtime
- `reqwest` - HTTP client (rustls TLS)
- `cargo-nextest` - Parallel test execution

**Container Runtimes:**
- Docker (default, production-ready)
- Podman (supported; required CI lane runs the integration suite against rootless Podman). Podman-specific handling lives behind `CliRuntime::is_podman()` in `crates/core/src/docker.rs`: image-ref qualification (`localhost/…` / `docker.io/library/…`), `podman ps` JSON shape (`Id`+`Names` array), SELinux `--security-opt label=disable`, and rootless `--userns=keep-id` for non-root users (`podman_create_args`, mirroring upstream `getPodmanArgs`). Remaining gap: GPU passthrough is not wired for Podman (`detect_gpu_capability` short-circuits to "unavailable").

## CI/CD Requirements

GitHub Actions runs on every PR:
- Format check: `cargo fmt --all -- --check` (must pass)
- Lint: `cargo clippy --all-targets -- -D warnings` (zero warnings)
- Tests: `make test-nextest-fast` (Ubuntu), `make test-nextest-ci` (full)
- Smoke tests: `make test-nextest-smoke` (serial with Docker)
- Coverage: Minimum threshold enforced via cargo-llvm-cov
- PR title: must follow Conventional Commits (`.github/workflows/pr-title.yml`)

**PR Title Conventional-Commit Types (CI-enforced):**
The "Validate PR title follows Conventional Commits" check (amannn/action-semantic-pull-request)
allows ONLY these types: `feat`, `fix`, `perf`, `docs`, `refactor`, `ci`, `build`,
`chore`. **`test` and `style` are NOT allowed** and will fail the check / block merge.
Use `chore` for test-only or tooling PRs (e.g. `chore(tests): …`). The squash merge
uses the PR **title**, not the commit subject — so a commit may say `test(up): …` as
long as the PR title is an allowed type. Editing the title (`gh pr edit <n> --title …`)
re-triggers the check.

**Common CI Failures:**
- Trailing whitespace anywhere (run `cargo fmt --all`)
- Clippy warnings (run `cargo clippy --all-targets -- -D warnings`)
- Doctest failures (missing trait imports, missing Default implementations)
- Test race conditions (reclassify to more conservative nextest group)
- PR title using a disallowed type (`test`/`style`) — retitle to `chore`

**MSRV / `dtolnay/rust-toolchain` dependabot gotcha:** the `MSRV (cargo check)` job pins
`dtolnay/rust-toolchain@<rust-version>` (e.g. `@1.95`), where the ref IS the MSRV and MUST
equal `rust-version` in root `Cargo.toml` `[workspace.package]`. dependabot reads that ref
as an ordinary action version and tries to bump it — PR #168 pushed it to `@1.100`, a Rust
release that doesn't exist (`rustup` 404), so the job could never pass. Such bumps are
**ignored** in `.github/dependabot.yml` (github-actions ecosystem) and MUST NOT be merged.
Raising the MSRV is a deliberate, in-lockstep decision (bump the pin AND `rust-version`
together), never an automatic dependency bump.

## Release Process

Releases are **tag-triggered**: pushing a `v*.*.*` tag runs `.github/workflows/release.yml`,
which verifies (fmt/clippy/lib tests), then builds + publishes binaries for 8 targets
(Linux gnu/musl × {x86_64, aarch64}, macOS × {x86_64, aarch64}, Windows × {x86_64, aarch64})
plus per-archive SHA256, an aggregate `SHA256SUMS`, SPDX SBOMs, and SLSA build provenance,
and deploys the install script to Pages. `-rc`/pre-release tags are flagged prerelease.

To cut a release:
1. Bump the version in **all four spots** (they must stay in sync): root `Cargo.toml`
   (`deacon-core` dep `version = …`), `crates/deacon/Cargo.toml`, `crates/core/Cargo.toml`,
   and refresh `Cargo.lock` (`cargo update -p deacon -p deacon-core`). Land it via a normal
   PR (CI-gated).
2. On the merged `main` commit, push an **annotated** tag matching the crate version
   (`git tag -a vX.Y.Z -m … && git push origin vX.Y.Z`). The workflow **validates the tag
   equals `crates/deacon/Cargo.toml`'s version** and fails otherwise.
3. `workflow_dispatch` with a `version` input can re-build/re-publish an existing tag.

Stacked-PR gotcha learned cutting rc.6: merging a base PR with `--delete-branch` **closes**
any PR stacked on that branch (GitHub does not retarget to `main`). Retarget the child to
`main` first, or rebase + open a fresh PR.

## Cross-Platform / Windows Notes

CI runs the `dev-fast` nextest profile on `windows-latest` (no Docker — `dev-fast`
excludes Docker/smoke/testcontainers, so it exercises platform-agnostic logic only).
We ship Windows binaries, so keep this lane green. Hard-won lessons:

- **Main-thread stack is ~1 MiB on Windows** (vs ~8 MiB on Unix). `main.rs` does NOT
  use `#[tokio::main]`; it runs the runtime on a 16 MiB `STACK_SIZE` thread (driver +
  `thread_stack_size` for workers). Without this, deep subcommand futures (doctor,
  read-configuration) crash with `STATUS_STACK_OVERFLOW` (0xC00000FD) — and `--version`
  survives (clap short-circuits), so a `--version` smoke test will NOT catch it. Don't
  reintroduce `#[tokio::main]`.
- **`if !cfg!(target_os = "windows") { … }` does NOT prevent compilation.** `cfg!` is a
  runtime boolean; both branches still compile. To exclude unix-only APIs
  (`std::os::unix`, `PermissionsExt`, `from_mode`) on Windows, use a `#[cfg(unix)]`
  **attribute** block (or gate the whole test). nextest compiles *all* test binaries
  before filtering, so even `dev-fast`-excluded Docker tests must compile on Windows.
- **Windows paths use `\` and get canonicalized** (`\\?\` verbatim prefix, 8.3 short-name
  expansion in `SubstitutionContext`). deacon's path output is correct (the reference CLI
  also emits `\` on Windows). Make path assertions separator-agnostic
  (`.replace('\\', "/")`) for relative fragments; for substituted absolute paths compare
  the **leaf component** (`dest_dir.file_name()`), which survives canonicalization, rather
  than the full path string.
- **A fixture whose BYTES are the expectation must be pinned `-text` in `.gitattributes`.**
  Git's autocrlf rewrites LF to CRLF on a Windows checkout, so a test comparing a file's
  content to a `"…\n"` literal fails on bytes git introduced, not on anything the code did
  — that is what kept `parity_hermetic` off Windows (#441), fixed by
  `parity/fixtures/** -text`. Prefer `-text` over `eol=lf` (it matches the other pins here
  and cannot corrupt a binary fixture), and run `git add --renormalize <path>` after adding
  the rule: it must produce NO diff, or every contributor sees the tree as modified.
- **A test that holds a path and compares it against a path the CLI printed must expect
  every SPELLING of it.** deacon canonicalizes `--workspace-folder`, and canonicalization
  renames: Windows returns the `\\?\` verbatim form with 8.3 short names expanded
  (`RUNNER~1` → `runneradmin`), macOS resolves `/var` → `/private/var`. The string the test
  passed in is then not a substring of the string it gets back. The parity normalizer's
  `TokenMap::workspace` registers all three spellings for this reason.
- **`--no-fail-fast`** is set on the test step so one run reports the full failure set
  (a single failure otherwise cancels the run and hides the rest — `nextest` shows
  `N/Total tests run` when cancelled vs `Total tests run` when complete).
- Genuinely Unix-only tests (POSIX-shell `initializeCommand`, bash `install.sh`,
  `/usr/bin/true`, the Unix-only port-forward daemon / `pid_alive`) are `#[cfg(unix)]`-gated
  with a one-line reason. Tracked Windows follow-ups: host-path mount semantics and
  host-hook (`initializeCommand`) execution.

## Debugging Tips

**Enable debug logging:**
```bash
RUST_LOG=debug cargo run -- <command>
DEACON_LOG=deacon=trace,deacon_core=debug cargo run -- <command>
```

**Test a specific nextest group:**
```bash
cargo nextest run --profile full --filter-expr 'test(integration_)*'
```

**View nextest configuration:**
```bash
cargo nextest show-config test-groups
make test-nextest-audit
```

**Verify cache behavior:**
```bash
RUST_LOG=debug cargo run -- up --container-data-folder /tmp/cache
# Check /tmp/cache/env_probe_* files
```

## Active Technologies

- Rust, Edition 2024, MSRV 1.95 (`unsafe_code = "deny"` workspace-wide)
- Product deps: `clap`, `serde`/`serde_json`, `indexmap`, `tokio`, `reqwest` (rustls),
  `tracing`, `thiserror` (core), `anyhow` (binary), `nix`/`fs2` (Unix port-forward daemon)
- Test tooling: `cargo-nextest` (the sole test executor), `tempfile`
- `crates/parity-harness` is dev-only (`publish = false`) and is NOT a dependency of the
  shipped binary

## Recent Changes

- **Replaced the conformance machinery with a differential parity suite.** Deleted the
  `deacon-conformance` crate and the discovery / drift / canary / coverage / certification
  layers built on it (~90k LOC, 78% the size of the product), along with four workflows,
  three nextest profiles and the `certify` release gate. What survives is the kernel:
  scenarios as data, one normalizer, one comparison, one nightly lane that gates nothing.
  `parity/SPEC_STATUS.md` is the harvested, hand-maintained answer to "does deacon behave
  like the CLI?". See the Differential Parity Suite section.
- The five legacy `parity_*` carriers are retired, their residual coverage ported as
  declarative data. The waiver, aggregator and corpus modules are gone: the nextest exit
  code is the verdict. The data root is `parity/`, with the 14 waiver files and 11
  extension records folded into `parity/ALLOWLIST.json`.
