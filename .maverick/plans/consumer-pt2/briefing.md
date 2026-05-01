# Pre-Flight Briefing: consumer-pt2

## In Scope

- Exec signal exit-code mapping and numeric error exits (Bead 6)
- Exec --remote-env flag rename with hidden --env alias and validation parity (Bead 7)
- Exec container-aware variable substitution wiring after merged config resolution (Bead 8)
- Exec working-directory fallback to container user home with reuse of env probe data (Bead 9)
- Exec --mount-workspace-git-root flag plumbing for config discovery (Bead 10)
- Exec JSON log-format PTY forcing via force_tty_if_json wiring (Bead 11)
- Exec direct --container-id running-state validation with clear non-zero failure (Bead 12)
- Up Compose overrideCommand support for keeping services alive through lifecycle (Bead 13)
- Up Compose feature-resolution/image-extension threading and metadata merge (Bead 14)
- ConfigLoader circular extends detection plus max-depth guard (Bead 15)
- Retry logic for transient network failures in image pull/build and OCI feature download (Bead 16)
- Spec-aligned test coverage updates for the above behaviors, including exec and compose paths

## Out of Scope

- Experimental lockfile support (--experimental-lockfile, --experimental-frozen-lockfile)
- --skip-feature-auto-mapping
- Windows PTY fallback / WSL2 path translation
- --platform support for cross-architecture builds
- Feature authoring commands

## Scope Boundaries

- Reuse existing substitution engine; do not change its internals
- Do not modify lifecycle command execution logic
- Preserve existing CLI signatures except adding new flags or aliases; keep --env as hidden deprecated alias
- Maintain workspace unsafe_code = forbid and Rust 2021/tokio constraints
- Bead 14 depends on Compose changes from Bead 13
- Direct-container exec error handling should produce numeric process exit behavior rather than bubbling Rust errors

## Relevant Modules

- crates/deacon/src/commands/exec.rs
- crates/deacon/src/cli.rs
- crates/deacon/src/commands/shared/config_loader.rs
- crates/deacon/src/commands/shared/env_user.rs
- crates/deacon/src/commands/shared/remote_env.rs
- crates/deacon/src/commands/read_configuration.rs
- crates/deacon/src/commands/up/compose.rs
- crates/deacon/src/commands/up/container.rs
- crates/deacon/src/commands/up/features_build.rs
- crates/deacon/src/commands/up/merged_config.rs
- crates/core/src/docker.rs
- crates/core/src/runtime.rs
- crates/core/src/container.rs
- crates/core/src/config.rs
- crates/core/src/compose.rs
- crates/core/src/variable.rs
- crates/core/src/container_env_probe.rs
- crates/core/src/retry.rs

## Existing Patterns

- `exec` currently exits with raw `ExecResult.exit_code`; `CliRuntime::exec` sets `-1` when `ExitStatus::code()` is absent, and `ExecResult` has no signal field yet.
- Direct `--container-id` resolution uses `container::resolve_container()` which returns inspected containers without enforcing `state == "running"`; label/workspace paths already filter running containers.
- `exec` working dir fallback is hard-coded to `/` when no config context exists; env probe only returns env/user, not home directory.
- Container-aware substitution already exists in `read_configuration`: before-container substitution with `${devcontainerId}`, then container substitution with `SubstitutionContext { container_env, container_workspace_folder }`; `exec` currently merges labels via `ConfigMerger::resolve_effective_config()` but never applies the container substitution pass.
- CLI wiring for exec is inverted from the spec ask: clap exposes `--env` and adds visible alias `--remote-env`; hidden backwards-compatible alias should be the other way around. Validation already allows empty values via `NormalizedRemoteEnv::parse`, while `ContainerSelector::parse_labels()` enforces non-empty values with `^.+=.+$`.
- Global `force_tty_if_json` is threaded into exec/up, but it is a standalone global flag, not automatically derived from `--log-format json`; `compute_should_use_tty()` will force PTY whenever that bool is true.
- Workspace git-root behavior exists for `up` and `read-configuration`, but exec config loading uses shared `load_config()` without a `mount_workspace_git_root` input, so exec currently cannot mirror that resolution behavior.
- Compose `up` starts projects via `ComposeManager::start_project()` using `ComposeProject::generate_injection_override()` for env/mount injection; the override model is the existing extension point for compose `overrideCommand` and feature-image rewrites.
- Traditional container `up` already implements `overrideCommand` in `core::docker` by appending `/bin/sh -c 'sleep infinity || tail -f /dev/null'`; compose flow lacks analogous override injection.
- Traditional container flow builds feature-extended images in `up/features_build.rs` and threads `resolved_features` into lifecycle + merged config; compose flow passes `None` for `resolved_features` in both fresh-start and reconnect merged-config paths.
- `ConfigLoader::resolve_extends_chain()` already performs canonical-path cycle detection and removes paths from `visited` on unwind; missing piece for the PRD is a depth limit and possibly tightening the emitted cycle message if needed.
- Shared async retry infrastructure already exists in `core::retry`; current compose container-id acquisition already uses manual exponential backoff, so network retry work should likely reuse `retry_async` rather than add another bespoke loop.

## Integration Points

- `crates/deacon/src/cli.rs` constructs `ExecArgs` and threads global options into `crates/deacon/src/commands/exec.rs`; changing flag names/semantics requires updates in both files and exec tests.
- `crates/deacon/src/commands/exec.rs` depends on `crates/deacon/src/commands/shared/{config_loader,env_user,remote_env}.rs` plus `deacon_core::{container,docker,config,container_env_probe}` for container selection, merged config, env probing, and exec dispatch.
- `crates/deacon/src/commands/read_configuration.rs` is the closest reference implementation for container-aware substitution that exec should reuse through `deacon_core::variable::SubstitutionContext` and `DevContainerConfig::apply_variable_substitution()`.
- `crates/core/src/runtime.rs` is a thin pass-through over `crates/core/src/docker.rs`; any `ExecResult` shape change must be propagated through these wrappers and mocks/tests in `crates/core/src/docker.rs`.
- `crates/core/src/container.rs` centralizes selector validation and container lookup; running-state validation for direct container-id exec is best added either here or immediately after `resolve_container()` in exec so all call sites stay consistent.
- `crates/deacon/src/commands/up/compose.rs` builds on `crates/core/src/compose.rs::ComposeProject` and `ComposeManager`; compose overrideCommand/feature-image support likely belongs in `ComposeProject` override generation or project mutation before `start_project()`.
- `crates/deacon/src/commands/up/container.rs` + `up/features_build.rs` already produce `resolved_features` and extended image tags for single-container flows; compose feature support should reuse that pipeline rather than reimplement feature resolution.
- `crates/deacon/src/commands/up/merged_config.rs` already accepts optional `resolved_features`; compose only needs to thread them through once available.
- `crates/core/src/config.rs` backs shared config loading for exec/read-configuration/outdated; adding extends-depth enforcement there will affect multiple commands consistently.
- `crates/core/src/retry.rs` is reusable for Docker pull/build and OCI fetcher retries, while `crates/core/src/oci/fetcher.rs` and `crates/core/src/docker.rs` are the concrete operation sites for transient network handling.

## Success Criteria

- BEAD-06-01: ExecResult struct in crates/core/src/docker.rs must include an optional signal field (e.g., signal: Option<i32>) to carry signal information from Docker exec results
- BEAD-06-02: When Docker reports exit code indicating signal termination (negative exit code or -1), the signal field in ExecResult must be populated with the signal number
- BEAD-06-03: In crates/deacon/src/commands/exec.rs, after receiving ExecResult: if exit_code >= 0, use it directly; if signal is Some(n), exit with 128 + n; otherwise exit with 1
- BEAD-06-04: Process killed by SIGTERM must produce exit code 143 (128 + 15); process killed by SIGKILL must produce exit code 137 (128 + 9)
- BEAD-06-05: When docker exec itself fails (e.g., container stopped), the exec command must exit with a non-zero numeric code and emit an error on stderr, rather than returning a Rust-level Err up the call stack
- BEAD-06-06: Both PTY and non-PTY exec paths must produce correct POSIX signal exit codes
- BEAD-07-01: ExecArgs field must be renamed from 'env: Vec<String>' to 'remote_env: Vec<String>' and the clap CLI flag must change from --env to --remote-env
- BEAD-07-02: The old --env flag must be preserved as a hidden deprecated alias mapping to the same remote_env field for backwards compatibility
- BEAD-07-03: --remote-env values must accept empty values (e.g., FOO=) matching the regex /.+=.*/ — this is already handled by NormalizedRemoteEnv::parse but must be verified through tests
- BEAD-07-04: --id-label values must reject empty values (e.g., key=) — ContainerSelector::parse_labels at container.rs:790 uses regex ^.+=.+$ which already enforces non-empty values; verify through tests
- BEAD-07-05: All existing tests and internal references to the old 'env' field in ExecArgs must be updated to use 'remote_env'
- BEAD-08-01: After ConfigMerger::resolve_effective_config produces the merged config in exec.rs:529-553, container-aware variable substitution must be applied using SubstitutionContext with the container's environment variables
- BEAD-08-02: ${containerEnv:VAR} references in remoteEnv and other merged config string fields must resolve to the actual environment variable values from the running container
- BEAD-08-03: ${containerWorkspaceFolder} references must resolve to the workspace folder path inside the container
- BEAD-08-04: Substitution must occur after metadata merging (ConfigMerger::resolve_effective_config) but before environment probe results are used to build the final exec environment
- BEAD-08-05: Host-side ${localEnv:VAR} substitution must continue to work correctly in exec context alongside container-aware substitution
- BEAD-08-06: The existing substitution engine (deacon_core::variable::SubstitutionContext) must be reused — no reimplementation of the substitution logic
- BEAD-09-01: When exec is invoked with --container-id or --id-label (no workspace config context), the working directory must fall back to the container user's home directory instead of '/' (currently hardcoded at exec.rs:516)
- BEAD-09-02: Working directory resolution must follow the fallback chain: CLI --workdir > config workspaceFolder > container user's home directory > '/' as last resort
- BEAD-09-03: If the environment probe has already determined the user, that information should be reused to determine the home directory without an extra container query
- BEAD-09-04: CLI --workdir override must always take precedence regardless of other context
- BEAD-10-01: ExecArgs struct must include a mount_workspace_git_root: bool field (default true), matching the spec and the existing UpArgs field at up/args.rs:236
- BEAD-10-02: The flag must be threaded through to config resolution (load_config/ConfigLoader) to control whether workspace folder resolution walks up to the git repository root
- BEAD-10-03: The flag must have no effect when --container-id or --id-label are the only resolution mechanism (no config context)
- BEAD-10-04: --mount-workspace-git-root false must use the workspace folder path as-is without walking up to git root
- BEAD-11-01: When --log-format json is active, force_tty_if_json in ExecArgs must be set to true at the CLI argument construction site
- BEAD-11-02: When force_tty_if_json is true, a PTY must be allocated even when stdin/stdout are not terminals (the compute_should_use_tty function at exec.rs:75 already supports this via force_tty parameter)
- BEAD-11-03: Default text log format without TTY must NOT allocate a PTY — existing behavior must be unchanged
- BEAD-11-04: PTY mode in JSON format must produce correct merged stdout/stderr stream output suitable for JSON consumers
- BEAD-12-01: After resolving a container via --container-id through resolve_container/find_container_by_id at container.rs:1019-1021, the exec command must check that the container's state field is 'running'
- BEAD-12-02: If the container is found but not in running state, the command must output 'Dev container is not running.' on stderr and exit with a non-zero code
- BEAD-12-03: Label-based resolution (--id-label) and workspace-based resolution paths must consistently filter by running state — verify these paths already do so
- BEAD-12-04: The state check must happen before any exec attempt, providing a clear user-facing error rather than an opaque Docker error
- BEAD-13-01: In the Compose flow (crates/deacon/src/commands/up/compose.rs), when overrideCommand is true (default per config.override_command.unwrap_or(true)), the primary service's command must be overridden with a long-running process (e.g., sleep infinity or equivalent)
- BEAD-13-02: The override must be applied either via a Compose override file with 'command: ["sleep", "infinity"]' or via docker compose CLI arguments — matching the approach used in docker.rs:1710-1721
- BEAD-13-03: When overrideCommand is explicitly false, the Compose service must run its natural command without modification
- BEAD-13-04: The override must ensure the container stays running through all lifecycle phases (onCreate through postAttach) and remains available for exec
- BEAD-13-05: Existing Compose tests must continue to pass after adding overrideCommand support
- BEAD-14-01: When a Compose-based devcontainer config includes features, feature resolution must be performed and the resolved features passed to the metadata merging step — currently compose.rs:369 passes None for resolved_features
- BEAD-14-02: A feature-extended image must be built (using the existing features_build.rs pipeline) and the Compose service updated to use it before running compose up
- BEAD-14-03: mergedConfiguration output must include feature-contributed settings (remoteEnv, remoteUser, customizations, etc.) when features are present in Compose mode
- BEAD-14-04: When the Compose config has no features, the flow must remain completely unchanged — no regression
- BEAD-14-05: This bead depends on Bead 13 (Compose overrideCommand) — compose changes from Bead 13 must land first
- BEAD-15-01: ALREADY PARTIALLY IMPLEMENTED — config.rs:1678 already uses a HashSet<PathBuf> for cycle detection and config.rs:1723 checks for cycles. Verify the existing implementation produces a clear error message including the path that caused the cycle
- BEAD-15-02: A maximum extends depth limit (e.g., 32 levels) must be enforced as an additional safeguard beyond cycle detection. If the chain exceeds this depth, return 'Extends chain too deep (max 32)'
- BEAD-15-03: Deep but non-circular extends chains within the depth limit must continue to work correctly
- BEAD-15-04: The error message for circular extends must include the full cycle path (e.g., 'Circular extends detected: A -> B -> A')
- BEAD-16-01: A retry module already exists at crates/core/src/retry.rs with RetryConfig (max_attempts=3, exponential backoff, jitter). This infrastructure must be wired into docker pull, docker build, and OCI feature download operations
- BEAD-16-02: Only transient network errors (connection refused, timeout, DNS resolution failure) must trigger retries — authentication failures (401/403), not-found (404), and other non-transient errors must fail immediately
- BEAD-16-03: Retry delays must follow exponential backoff pattern (1s, 2s, 4s base delays for the 3 retries as specified in the PRD)
- BEAD-16-04: Each retry attempt must be logged at warn level with format: 'Retrying <operation> after network error (attempt N/3): <error>'
- BEAD-16-05: No retry logic must be added to cached operations or local-only operations — only network-facing operations
- CROSS-01: All changes must pass cargo fmt --all -- --check and cargo clippy --all-targets -- -D warnings with zero warnings
- CROSS-02: All changes must pass make test-nextest-fast (unit/bins/examples + doctests)
- CROSS-03: No existing CLI flag signatures may be removed — only new flags or aliases may be added (except the --env to --remote-env rename which preserves --env as hidden alias)
- CROSS-04: unsafe_code = 'forbid' workspace policy must be maintained — no unsafe code in any changes
- CROSS-05: All new integration tests must be added to appropriate test groups in .config/nextest.toml with proper override rules
- CROSS-06: No unwrap() or unchecked expect() in runtime paths — all errors must be propagated with Result and .context()
- CROSS-07: No blocking calls inside async functions — use tokio async equivalents

## Test Scenarios

- BEAD-06-T01: Unit test — construct ExecResult with exit_code=-1 and signal=Some(15), verify exec handler would produce exit code 143
- BEAD-06-T02: Unit test — construct ExecResult with exit_code=-1 and signal=Some(9), verify exit code 137
- BEAD-06-T03: Unit test — construct ExecResult with exit_code=42 and signal=None, verify exit code 42 (unchanged behavior)
- BEAD-06-T04: Unit test — construct ExecResult with exit_code=0 and signal=None, verify exit code 0 (success unchanged)
- BEAD-06-T05: Integration test — exec against stopped container, verify non-zero exit and error message on stderr
- BEAD-06-T06: Unit test — ExecResult with exit_code=-1 and signal=None (ambiguous), verify exit code 1
- BEAD-07-T01: Unit test — parse ExecArgs with --remote-env FOO=BAR, verify remote_env field populated correctly
- BEAD-07-T02: Unit test — parse ExecArgs with --remote-env FOO= (empty value), verify accepted and parsed as name=FOO value=empty
- BEAD-07-T03: Unit test — parse ExecArgs with --env FOO=BAR (hidden alias), verify same behavior as --remote-env
- BEAD-07-T04: Unit test — parse --id-label key= (empty value), verify rejected with error
- BEAD-07-T05: Unit test — parse --id-label key=val, verify accepted
- BEAD-07-T06: Verify --remote-env appears in --help output and --env does not (hidden)
- BEAD-08-T01: Unit test — config with remoteEnv containing ${containerEnv:PATH}, mock container env with PATH=/usr/bin, verify substitution resolves to /usr/bin
- BEAD-08-T02: Unit test — config with ${containerWorkspaceFolder} reference, verify resolves to the actual workspace folder path in container
- BEAD-08-T03: Unit test — config with ${localEnv:HOME} reference, verify still resolves to host HOME value in exec context
- BEAD-08-T04: Unit test — config with no substitution variables, verify pass-through unchanged
- BEAD-08-T05: Unit test — verify substitution ordering: happens after ConfigMerger::resolve_effective_config, before env probe usage
- BEAD-09-T01: Unit/Integration test — exec --container-id <id> with no config context, verify working directory is user's home (e.g., /home/vscode) not '/'
- BEAD-09-T02: Unit test — exec --container-id <id> --workdir /tmp, verify working directory is /tmp (CLI override)
- BEAD-09-T03: Unit test — exec with config context providing workspaceFolder, verify workspace folder used (not home)
- BEAD-09-T04: Unit test — exec --container-id <id> when home directory cannot be determined, verify fallback to '/'
- BEAD-10-T01: Unit test — ExecArgs with mount_workspace_git_root=true (default), verify workspace resolves from git root
- BEAD-10-T02: Unit test — ExecArgs with mount_workspace_git_root=false, verify workspace folder used as-is
- BEAD-10-T03: Unit test — ExecArgs with --container-id and mount_workspace_git_root, verify flag has no effect
- BEAD-11-T01: Unit test — build_exec_config with force_tty_if_json=true and non-TTY stdin/stdout, verify tty=true in ExecConfig
- BEAD-11-T02: Unit test — build_exec_config with force_tty_if_json=false and non-TTY stdin/stdout, verify tty=false (default behavior)
- BEAD-11-T03: Unit test — verify that CLI --log-format json sets force_tty_if_json=true in ExecArgs
- BEAD-12-T01: Integration test (mock Docker) — exec --container-id <stopped-container>, verify error message 'Dev container is not running.' and non-zero exit
- BEAD-12-T02: Integration test (mock Docker) — exec --container-id <running-container>, verify command executes normally
- BEAD-12-T03: Unit test — verify resolve_container via --id-label path filters by running state
- BEAD-12-T04: Unit test — verify workspace-based resolve_target_container filters by state='running' (already at exec.rs:184)
- BEAD-13-T01: Integration test — Compose config with short-lived command (e.g., echo hello) and overrideCommand=true (default), verify container stays running after lifecycle
- BEAD-13-T02: Integration test — Compose config with overrideCommand=false, verify container runs its natural command (may exit)
- BEAD-13-T03: Unit test — verify override command injection generates correct Compose override or CLI args with 'sleep infinity' equivalent
- BEAD-13-T04: Unit test — verify lifecycle commands execute successfully in Compose mode with override active
- BEAD-14-T01: Integration test — Compose config with features defined, verify feature-extended image is built and used by compose service
- BEAD-14-T02: Unit test — verify mergedConfiguration output includes feature metadata (remoteEnv, customizations) in Compose mode
- BEAD-14-T03: Integration test — Compose config without features, verify flow unchanged (no feature build step)
- BEAD-14-T04: Unit test — verify resolved_features is passed to merged config builder instead of None
- BEAD-15-T01: Unit test — circular extends (A extends B extends A), verify error returned with cycle path, no hang
- BEAD-15-T02: Unit test — self-referencing extends (A extends A), verify error with clear message
- BEAD-15-T03: Unit test — deep non-circular chain (20 levels), verify loads successfully
- BEAD-15-T04: Unit test — chain exceeding max depth (e.g., 33 levels), verify 'Extends chain too deep' error
- BEAD-15-T05: Unit test — verify error message includes the path that caused the cycle
- BEAD-16-T01: Unit test — mock transient network error (connection refused) during docker pull, verify 3 retry attempts with exponential backoff
- BEAD-16-T02: Unit test — mock 401 auth failure during docker pull, verify immediate failure (no retry)
- BEAD-16-T03: Unit test — mock 404 not found during OCI feature download, verify immediate failure (no retry)
- BEAD-16-T04: Unit test — mock transient error followed by success on retry 2, verify operation succeeds
- BEAD-16-T05: Unit test — verify each retry is logged at warn level with operation name, attempt number, and error
- BEAD-16-T06: Unit test — verify retry backoff timing follows 1s, 2s, 4s pattern (within jitter tolerance)
- CROSS-T01: Run cargo fmt --all -- --check after all changes, verify zero formatting issues
- CROSS-T02: Run cargo clippy --all-targets -- -D warnings after all changes, verify zero warnings
- CROSS-T03: Run make test-nextest-fast, verify all existing tests pass
- CROSS-T04: Grep for unwrap() and expect() in new/modified runtime code paths, verify none present

## Risks & Challenges

- BEAD-06 SIGNAL MODEL IS WRONG: The PRD assumes Docker reports signal termination as a negative exit code or -1. In reality, `docker exec` returns the exit code from the process inside the container — which for signal-killed processes is already 128+signal on Linux (the shell convention). Docker does NOT report raw signal numbers. The code at docker.rs:1432 uses `ExitStatus::code().unwrap_or(-1)`, where -1 means the *host-side* process had no exit code (e.g., killed by signal on the host). Adding `signal: Option<i32>` to ExecResult and trying to reverse-engineer the signal number from Docker's output is solving the wrong problem. The real fix is much simpler: just pass through Docker's exit code (which is already 128+N for signal deaths). The -1 case is a host-side anomaly, not a container signal. Building signal extraction logic based on wrong assumptions will produce incorrect exit codes.
- BEAD-06 PTY COMPLICATION UNADDRESSED: In PTY mode (docker.rs:1490), the exit status comes from `child.wait()` on the *host-side* docker process, not the in-container process. If the host-side docker process is killed by a signal (e.g., user hits Ctrl+C), `ExitStatus::code()` returns None and the code defaults to -1. But the *container* process may have exited differently. The PRD conflates host-side signal death with container-side signal death. These are fundamentally different scenarios requiring different handling, and the PRD treats them as one.
- BEAD-07 HIDDEN ALIAS WRONG DIRECTION: The codebase analysis correctly identifies that the current code exposes --env with --remote-env as a *visible* alias (inverted from spec). However, the PRD's acceptance criteria says '--env FOO=BAR still works (hidden alias)' but doesn't address whether the clap derive macro can easily support hidden aliases for the *old* name while making the *new* name primary. Clap's `#[arg(alias = ...)]` doesn't support `hide = true` on the alias itself — you'd need `#[arg(long = "remote-env", visible_alias = ..., alias = "env")]` or `#[arg(long = "remote-env", hide = true)]` which hides the *primary*, not the alias. Implementation may require clap builder API instead of derive, which is a larger refactor than estimated.
- BEAD-08 ORDERING CONTRADICTION: The PRD says 'Substitution must occur after metadata merging but before environment probe results are used.' But looking at exec.rs:529-568, the config merge happens at line 532 via ConfigMerger::resolve_effective_config, then config_remote_env and config_remote_user are extracted at lines 551-552 and fed into resolve_env_and_user at line 558. The env probe *returns* the container environment, which is needed *as input* to container-aware substitution. So you need the probe result to perform ${containerEnv:VAR} substitution, but the PRD says substitution must happen before the probe. This is a chicken-and-egg problem: you need container env vars to substitute ${containerEnv:VAR}, but the PRD says to substitute before the probe that provides those vars. The actual solution requires either a two-pass approach or a different ordering than what the PRD specifies.
- BEAD-09 HOME DIRECTORY NOT AVAILABLE FROM PROBE: The PRD says 'If the environment probe has already been run, reuse that information to determine the home directory.' But looking at the env_user.rs shared helper and container_env_probe.rs, the probe returns environment variables and effective user — not the user's home directory. The HOME env var may or may not be set in the container environment. To reliably get the home directory, you'd need to either: (a) parse /etc/passwd inside the container (extra exec call), (b) rely on HOME being set (fragile), or (c) use UserInfo::default_home_dir() heuristic from user_mapping.rs (assumes /home/<user> convention). None of these are mentioned in the PRD, and each has failure modes.
- BEAD-12 LABEL PATH DOES NOT FILTER BY RUNNING STATE: The PRD says 'Label-based and workspace-based paths already filter by running state. Verify this.' But examining container.rs:1025-1032, find_containers_by_labels() does NOT filter by state. It calls docker.list_containers() with label filters and returns whatever Docker returns (which includes stopped containers by default, depending on Docker API filter behavior). The workspace-based path in exec.rs:182-184 does filter `.filter(|c| c.state == "running")`, but the label-based path in resolve_container() does NOT. This means the PRD's assumption that only the direct --container-id path needs fixing is wrong — the label path has the same bug.
- BEAD-13 COMPOSE OVERRIDE COMPLEXITY UNDERESTIMATED: The PRD says 'Risk: Medium' but the compose override mechanism is more complex than described. ComposeProject::generate_injection_override() generates a YAML override file for env/mount injection. Adding command overrides to this requires understanding the interaction between `command:`, `entrypoint:`, and the service's existing Dockerfile CMD/ENTRYPOINT. If a service has a custom ENTRYPOINT, overriding just `command:` may not produce the desired behavior. The docker.rs approach uses `/bin/sh -c 'sleep infinity || tail -f /dev/null'` as both entrypoint and args — but in Compose, the entrypoint/command split is handled differently. Incorrect override could break services that have multi-stage entrypoints (e.g., tini + app).
- BEAD-14 FEATURE IMAGE EXTENSION FOR COMPOSE IS ARCHITECTURALLY COMPLEX: The PRD says 'build a feature-extended image (using the existing features_build.rs pipeline) and update the Compose service to use it.' This understates the complexity. The features_build.rs pipeline is tightly coupled to the single-container Docker flow — it builds a new image by extending the base image with feature layers. For Compose, you'd need to: (1) determine the primary service's image, (2) build a feature-extended version, (3) rewrite the compose file or override to point to the new image, (4) handle the case where the service uses `build:` instead of `image:` (feature extension on build-context services requires a different approach). The PRD doesn't address the build: vs image: distinction at all.
- BEAD-15 PARTIALLY ALREADY DONE — RISK OF DUPLICATE WORK: The codebase analysis and config.rs:1722-1733 show cycle detection is already implemented with HashSet<PathBuf>. The PRD acknowledges this ('ALREADY PARTIALLY IMPLEMENTED') but still creates a full bead for it. The only missing piece is a depth limit. This is ~5 lines of code. The effort allocated (full bead with 5 test scenarios) is disproportionate and may lead implementers to over-engineer or accidentally break the working cycle detection.
- BEAD-16 RETRY MODULE SEMANTICS MISMATCH: The existing retry.rs uses `max_attempts` as the number of *retries* (excluding the initial attempt) — the loop runs `0..=config.max_attempts` which means total attempts = max_attempts + 1. The PRD says 'retry up to 3 times with exponential backoff (1s, 2s, 4s)' — if max_attempts=3, that's 4 total attempts (1 initial + 3 retries). But the PRD's backoff values (1s, 2s, 4s) describe 3 delays, meaning 4 attempts. The default RetryConfig has base_delay=100ms and the PRD wants 1s/2s/4s, so you'd need base_delay=1s. But the existing retry.rs default is 100ms for a reason (compose container-id acquisition). Changing it would break existing callers. Each call site needs its own RetryConfig — this isn't mentioned.
- CROSS-CUTTING: std::process::exit() IN EXEC CREATES UNTESTABILITY: exec.rs:590 calls `std::process::exit(result.exit_code)` directly. Bead 6 adds signal mapping logic before this exit call. But `std::process::exit()` cannot be tested in unit tests — it terminates the test process. The PRD's test scenarios (BEAD-06-T01 through T04) test ExecResult construction, not the actual exit path. The actual signal-to-exit-code mapping logic in the execute() function is untestable without refactoring to return the exit code instead of calling process::exit. This is a pre-existing design issue that the PRD should address but doesn't.
- BEAD-11 ALREADY WIRED BUT PRD THINKS IT ISN'T: The PRD says 'the CLI layer must set this field based on the actual --log-format value.' Looking at cli.rs:1207, `force_tty_if_json: self.force_tty_if_json` is already threaded from the global CLI flag to ExecArgs. The spec says PTY should be forced when log_format is JSON, but the current implementation makes it a *separate* boolean flag (--force-tty-if-json) rather than deriving it from --log-format. The PRD is correct that this needs fixing, but the fix is more nuanced: you need to either auto-set force_tty_if_json when log_format==json (changing the semantics of the existing flag), or add new derivation logic. The PRD doesn't address the fact that the existing --force-tty-if-json flag would become partially redundant.

## Blind Spots

- NO PODMAN CONSIDERATION: The PRD never mentions Podman, but the codebase supports it as an alternative runtime. Bead 6's signal handling may differ on Podman (which uses conmon, not Docker's containerd shim). Bead 13's compose override may interact differently with podman-compose. The PRD should at minimum note where Podman behavior diverges.
- NO CONSIDERATION OF DOCKER API VERSION DIFFERENCES: Docker's exec inspect API (used to get exit codes) has different behavior across API versions. Older Docker versions may not report signal information the same way. The PRD assumes uniform Docker behavior.
- BEAD-08 MISSING: WHAT HAPPENS TO containerSubstitute FOR --container-id EXEC?: When exec is invoked with --container-id (no config context), there's no merged config to substitute. But the PRD only discusses the config-context path. The no-config path should explicitly skip substitution, and the PRD should state this.
- BEAD-14 MISSING: MULTI-SERVICE COMPOSE CONFIGS: The PRD discusses 'primary service' but doesn't address what happens when the compose config has multiple services with features. Which service gets the feature-extended image? How does the feature resolution interact with service-specific build contexts?
- TEST STRATEGY GAP: All test scenarios for Beads 6, 8, 9, 11, 12 describe unit tests with mock Docker clients. But the actual bugs these fix would only be caught by integration tests against real Docker. The PRD should specify which tests are integration (docker-shared/docker-exclusive) vs unit.
- BEAD-09 RACE CONDITION: If the container user changes between the env probe (which determines the user) and the home directory lookup, the home directory could be wrong. This is unlikely but worth noting since the PRD emphasizes reusing probe data.
- BEAD-13 MISSING: COMPOSE v1 vs v2 DIFFERENCES: The compose override mechanism may behave differently between docker-compose (v1, Python) and docker compose (v2, Go plugin). The PRD's ComposeManager abstraction may not account for these differences in command override behavior.
- NO ROLLBACK PLAN: With 11 beads touching exec.rs, docker.rs, container.rs, compose.rs, and config.rs simultaneously, there's no discussion of how to handle partial implementation failure. If Bead 14 fails but Bead 13 lands, what's the fallback?
- BEAD-07 MISSING: CLI HELP TEXT MIGRATION: Renaming --env to --remote-env changes the user-facing help text. Users reading `deacon exec --help` will see --remote-env instead of --env. The PRD doesn't discuss whether to add a deprecation warning when --env is used, which would help users migrate.
- BEAD-16 MISSING: IDEMPOTENCY OF RETRIED OPERATIONS: Docker pull is idempotent, but docker build may not be (e.g., if a partial build left dangling layers). OCI feature download to a temp directory may also not be idempotent if the previous attempt left partial files. The PRD assumes all retried operations are safe to retry without cleanup.

## Open Questions

- Bead 6: Does Docker actually report signal numbers anywhere in its exec output, or does the container's exit code already encode 128+signal per shell convention? If the latter, the entire signal field addition to ExecResult is unnecessary — just pass through the exit code. What does `docker exec` return when a process inside the container is killed by SIGKILL? Is it 137, -1, or something else? This should be empirically tested before implementing.
- Bead 7: Can clap's derive macro support a hidden alias for the *old* flag name while making the *new* name primary? Has this pattern been validated? If not, what's the fallback — builder API?
- Bead 8: How should the chicken-and-egg between container env probe (which provides containerEnv values) and container-aware substitution (which needs those values to resolve ${containerEnv:VAR}) be resolved? Does the reference implementation do two passes?
- Bead 9: Which method should be used to determine the container user's home directory — parsing $HOME from probe env, running `getent passwd` in the container, or using the UserInfo::default_home_dir() heuristic? Each has different reliability/performance tradeoffs.
- Bead 11: Should --force-tty-if-json be deprecated/removed once PTY forcing is automatically derived from --log-format json? Or should both mechanisms coexist?
- Bead 12: Should the label-based resolution path (container.rs:1025-1032) also be fixed to filter by running state? The PRD's codebase analysis says it 'already filters' but the code shows it does NOT.
- Bead 13: How should overrideCommand interact with Compose services that use a custom ENTRYPOINT? Should the override replace both entrypoint and command, or only command?
- Bead 14: How should feature extension work when the Compose primary service uses `build:` context instead of `image:`? The features_build.rs pipeline assumes a base image to extend — what's the base when it's a build context?
- Bead 16: Should the retry configuration (delays, max attempts) be user-configurable via environment variables or config, or hardcoded? The existing RetryConfig is serializable (serde), suggesting it was designed for configurability.
- Cross-cutting: The suggested sequencing puts Bead 13 first, but Bead 6 and 7 are marked High priority and are independent. Given that exec changes (6, 7, 8) are lower risk and can be parallelized, should they be prioritized to unblock spec-compliance testing earlier?
