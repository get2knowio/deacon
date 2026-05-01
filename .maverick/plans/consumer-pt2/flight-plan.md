---
name: consumer-pt2
version: '1'
created: '2026-05-01'
objective: "Close spec-compliance gaps in the exec subcommand (Beads 6\u201312) and\
  \ residual up gaps (Beads 13\u201316) to reach full equivalency with the DevContainer\
  \ spec"
tags:
- exec
- up
- compose
- spec-compliance
- rust
- devcontainer
- phase-2
- signal-exit-codes
- variable-substitution
- retry
- overrideCommand
- features
scope:
  in_scope:
  - 'Bead 6: ExecResult signal field + POSIX exit code mapping (128+signal) in exec.rs,
    docker.rs; error-path numeric exit'
  - "Bead 7: Rename ExecArgs.env \u2192 remote_env, --env as hidden alias, validate\
    \ --remote-env allows empty values, verify --id-label rejects empty values"
  - 'Bead 8: Wire SubstitutionContext container-aware substitution into exec after
    ConfigMerger::resolve_effective_config; reuse existing substitution engine'
  - 'Bead 9: Working directory fallback to container user home (from probe HOME env
    or heuristic) instead of hardcoded /'
  - 'Bead 10: Add mount_workspace_git_root flag to ExecArgs; thread to config resolution;
    no-op for direct container-id paths'
  - 'Bead 11: Derive force_tty_if_json from --log-format json at CLI construction
    site in cli.rs'
  - 'Bead 12: Validate container running state after resolve_container for --container-id
    path; verify/fix label-path running-state filtering'
  - 'Bead 13: Compose overrideCommand support via override file injection with sleep-infinity
    command'
  - 'Bead 14: Thread feature resolution and image extension into Compose flow; pass
    resolved_features to merged_config (depends on Bead 13 landing first)'
  - 'Bead 15: Add max-depth guard (32 levels) to ConfigLoader::load_with_extends;
    verify existing cycle detection message quality'
  - 'Bead 16: Wire retry.rs into docker pull/build and OCI fetcher for transient network
    errors with per-call-site RetryConfig (base_delay=1s, max_attempts=3)'
  - Update all existing tests referencing ExecArgs.env to use remote_env
  - Add new tests for all beads; assign integration tests to appropriate nextest.toml
    groups
  out_of_scope:
  - Experimental lockfile support (--experimental-lockfile, --experimental-frozen-lockfile)
  - --skip-feature-auto-mapping
  - Windows PTY fallback / WSL2 path translation
  - --platform support for cross-architecture builds
  - Feature authoring commands (test, info, plan, package, publish)
  - Podman-specific signal handling or podman-compose override differences
  - Docker API version compatibility shims
  - Multi-service Compose feature extension (beyond primary service)
  - User-configurable retry parameters via env vars or config file
  - Deprecation warning emission when legacy --env alias is used
  - Removing or deprecating --force-tty-if-json flag
  - Modifying substitution engine internals
  - Modifying lifecycle command execution logic
  boundaries: []
---

# consumer-pt2

## Objective

Close spec-compliance gaps in the exec subcommand (Beads 6–12) and residual up gaps (Beads 13–16) to reach full equivalency with the DevContainer spec

## Context

# Background

Deacon is a Rust DevContainer CLI (`crates/deacon` + `crates/core`). Phase 1 brought `up` to ~95% spec equivalency. This Phase 2 closes remaining gaps in `exec` (Beads 6–12) and `up` (Beads 13–16).

## Key Risk Mitigations

**Bead 6 — Signal model:** Docker's `exec` already returns 128+signal as the container exit code per Linux shell convention. The `-1` sentinel from `ExitStatus::code().unwrap_or(-1)` is a *host-side* anomaly (host process killed), not a container signal. Empirically verify what `docker exec` returns for SIGKILL/SIGTERM before adding a `signal` field — the fix may simply be correct pass-through of Docker's exit code with `-1` mapped to `1`.

**Bead 7 — Hidden alias direction:** Current code has `--env` primary with `--remote-env` as visible alias (inverted). Fix: make `--remote-env` primary, `--env` hidden. Clap derive `#[arg(long = "remote-env", alias = "env")]` plus a separate `#[arg(long = "env", hide = true, ...)]` or builder API may be needed — validate clap approach before committing.

**Bead 8 — Chicken-and-egg ordering:** `${containerEnv:VAR}` substitution requires container env vars from the probe, but the probe's *result* is what provides those vars. Resolution: run the env probe first to get container environment, then apply container-aware substitution to the merged config using those values, then use the substituted config to build the final exec environment. This is the opposite order from what the PRD text says — the probe must precede substitution.

**Bead 12 — Label path gap:** `find_containers_by_labels()` in `container.rs` does NOT filter by running state (contrary to PRD assumption). Fix both `--container-id` and `--id-label` paths.

**Bead 16 — RetryConfig:** Existing `retry.rs` default is 100ms base delay (for compose container-id polling). Network retry needs `base_delay = 1s`. Use per-call-site `RetryConfig` instances rather than changing the global default.

## Suggested Implementation Order

Bead 13 → Bead 14 → Bead 6 → Bead 8 → Bead 7 → Bead 9 → Bead 12 → Bead 11 → Bead 10 → Bead 15 → Bead 16

## Key Files

- `crates/deacon/src/commands/exec.rs` — main exec command (ExecArgs, execute, exit handling)
- `crates/deacon/src/cli.rs` — CLI argument construction, force_tty_if_json wiring
- `crates/deacon/src/commands/shared/{config_loader,env_user,remote_env}.rs` — shared helpers
- `crates/deacon/src/commands/read_configuration.rs` — reference for container-aware substitution
- `crates/deacon/src/commands/up/compose.rs` — Compose flow (overrideCommand, features)
- `crates/deacon/src/commands/up/{features_build,merged_config}.rs` — feature pipeline
- `crates/core/src/docker.rs` — ExecResult, docker pull/build, overrideCommand reference (line 1710)
- `crates/core/src/container.rs` — container resolution, label filtering, running-state logic
- `crates/core/src/config.rs` — ConfigLoader extends chain, cycle detection (line 1678+)
- `crates/core/src/retry.rs` — existing retry infrastructure
- `crates/core/src/oci/fetcher.rs` — OCI feature download (retry wiring target)
- `crates/core/src/variable.rs` — SubstitutionContext (reuse, do not modify internals)
- `.config/nextest.toml` — test group configuration (must be updated for new integration tests)


## Success Criteria

- [ ] BEAD-06: ExecResult carries optional signal field; exit code mapping follows 128+signal for signal deaths, direct code for normal exits, 1 for ambiguous failures (Verification: Unit tests for SIGTERM→143, SIGKILL→137, exit 42→42, ambiguous→1; integration test for stopped-container error path)
- [ ] BEAD-07: ExecArgs field renamed to remote_env with --remote-env as primary flag; --env preserved as hidden deprecated alias; --remote-env accepts empty values (FOO=); --id-label rejects empty values (Verification: Unit tests for --remote-env, --remote-env FOO= (empty), --env alias, --id-label key= rejection, --id-label key=val acceptance; --help shows --remote-env not --env)
- [ ] BEAD-08: Container-aware variable substitution applied after ConfigMerger::resolve_effective_config using SubstitutionContext; ${containerEnv:VAR} and ${containerWorkspaceFolder} resolve correctly; existing ${localEnv:VAR} still works (Verification: Unit tests for containerEnv substitution, containerWorkspaceFolder substitution, localEnv pass-through, no-variable pass-through; ordering verified (after merge, input uses probe-provided container env))
- [ ] BEAD-09: Working directory fallback chain: CLI --workdir > config workspaceFolder > container user home > '/'; home directory derived from probe data (HOME env var or heuristic) without extra container query (Verification: Tests for no-config home fallback, --workdir override, config workspaceFolder precedence, home-not-determinable last-resort fallback to /)
- [ ] BEAD-10: ExecArgs includes mount_workspace_git_root: bool (default true); threaded to config resolution; has no effect when --container-id/--id-label only (Verification: Unit tests for default git-root resolution, false=as-is, no-effect with --container-id)
- [ ] BEAD-11: force_tty_if_json in ExecArgs set to true when --log-format json is active at CLI argument construction; PTY allocated for JSON mode regardless of TTY state; text mode without TTY unchanged (Verification: Unit tests for JSON format forces PTY, text format without TTY does not allocate PTY, CLI wiring sets the flag correctly)
- [ ] BEAD-12: Direct --container-id exec validates container is in running state before exec; stopped container produces 'Dev container is not running.' on stderr with non-zero exit; label-based and workspace-based paths verified to filter by running state (Verification: Integration test for stopped container error; running container works; label-path running-state filter verified (fix if absent))
- [ ] BEAD-13: Compose flow applies overrideCommand (default true) by injecting sleep-infinity equivalent into primary service via override file or CLI args; overrideCommand=false leaves service command unmodified; lifecycle completes successfully (Verification: Integration tests for short-lived command stays alive with override=true, natural command with override=false; unit test for override injection correctness)
- [ ] BEAD-14: Compose flow with features resolves and builds feature-extended image, updates service to use it, and passes resolved_features to metadata merging; Compose without features unchanged (Verification: Integration test for feature-extended image built and used; unit test for mergedConfiguration including feature metadata; regression test for no-features path)
- [ ] BEAD-15: ConfigLoader enforces max extends depth of 32; existing cycle detection verified to emit clear message including cycle path; deep non-circular chains within limit work (Verification: Unit tests for A→B→A cycle error, self-referencing extends, 20-level chain success, 33-level depth exceeded error; error message includes cycle path)
- [ ] BEAD-16: Transient network errors in docker pull, docker build, and OCI feature download retry up to 3 times with 1s/2s/4s exponential backoff using existing retry.rs infrastructure; 401/403/404 fail immediately; each retry logged at warn level (Verification: Unit tests for transient failure retries 3x, 401/404 immediate failure, retry-then-success, warn-level logging with attempt number, backoff timing)
- [ ] CROSS: All changes pass cargo fmt, cargo clippy -D warnings, make test-nextest-fast; no unwrap/unchecked expect in runtime paths; no blocking calls in async; unsafe_code=forbid maintained; new integration tests added to nextest.toml groups (Verification: CI gate: fmt check, clippy zero warnings, full fast test suite green; grep for unwrap/expect in modified runtime files returns zero hits)

## Scope

### In

- Bead 6: ExecResult signal field + POSIX exit code mapping (128+signal) in exec.rs, docker.rs; error-path numeric exit
- Bead 7: Rename ExecArgs.env → remote_env, --env as hidden alias, validate --remote-env allows empty values, verify --id-label rejects empty values
- Bead 8: Wire SubstitutionContext container-aware substitution into exec after ConfigMerger::resolve_effective_config; reuse existing substitution engine
- Bead 9: Working directory fallback to container user home (from probe HOME env or heuristic) instead of hardcoded /
- Bead 10: Add mount_workspace_git_root flag to ExecArgs; thread to config resolution; no-op for direct container-id paths
- Bead 11: Derive force_tty_if_json from --log-format json at CLI construction site in cli.rs
- Bead 12: Validate container running state after resolve_container for --container-id path; verify/fix label-path running-state filtering
- Bead 13: Compose overrideCommand support via override file injection with sleep-infinity command
- Bead 14: Thread feature resolution and image extension into Compose flow; pass resolved_features to merged_config (depends on Bead 13 landing first)
- Bead 15: Add max-depth guard (32 levels) to ConfigLoader::load_with_extends; verify existing cycle detection message quality
- Bead 16: Wire retry.rs into docker pull/build and OCI fetcher for transient network errors with per-call-site RetryConfig (base_delay=1s, max_attempts=3)
- Update all existing tests referencing ExecArgs.env to use remote_env
- Add new tests for all beads; assign integration tests to appropriate nextest.toml groups

### Out

- Experimental lockfile support (--experimental-lockfile, --experimental-frozen-lockfile)
- --skip-feature-auto-mapping
- Windows PTY fallback / WSL2 path translation
- --platform support for cross-architecture builds
- Feature authoring commands (test, info, plan, package, publish)
- Podman-specific signal handling or podman-compose override differences
- Docker API version compatibility shims
- Multi-service Compose feature extension (beyond primary service)
- User-configurable retry parameters via env vars or config file
- Deprecation warning emission when legacy --env alias is used
- Removing or deprecating --force-tty-if-json flag
- Modifying substitution engine internals
- Modifying lifecycle command execution logic

## Constraints

- Do not modify variable substitution engine internals — only wire its existing API
- Do not modify lifecycle command execution logic
- Preserve --env as a hidden alias; do not break existing --env usage
- Maintain unsafe_code = 'forbid' workspace policy — no unsafe blocks
- All changes must pass cargo clippy --all-targets -- -D warnings (zero warnings)
- All changes must pass cargo fmt --all -- --check
- No unwrap() or unchecked expect() in runtime code paths
- No blocking calls inside async functions — use tokio async equivalents
- New integration tests must be added to .config/nextest.toml with appropriate group overrides
- Bead 14 must not land before Bead 13 compose changes are in place
- Each call site using retry must use its own RetryConfig — do not change the default RetryConfig shared by existing callers
- Rust edition 2021, async runtime tokio
- Investigate empirically whether Docker already encodes 128+signal before adding signal extraction logic to ExecResult — avoid solving the wrong problem
- Verify label-based container resolution running-state filtering before assuming it works; fix if absent (Bead 12 blind spot)
