# Up/Exec Consistency Plan

The list below breaks down the work needed to align `up` and `exec` user experience and internal plumbing. Treat each task as an incremental PR; all should cite the referenced files to avoid drift.

## Task 1 – Centralize terminal sizing logic
- Extract `TerminalDimensions` from `crates/deacon/src/commands/up.rs` into a shared helper (e.g., `commands/shared/terminal.rs`).
- Update `cli.rs` validation to call the helper once, and pass the normalized dimensions into both `UpArgs` and `ExecArgs`.
- Thread the dimensions through `ExecArgs`→`build_exec_config` so PTY sizing actually affects `ExecConfig` as required by `specs-completed/007-exec-subcommand/spec.md` FR-009.
- Acceptance: `deacon exec --terminal-columns 132 --terminal-rows 40 …` issues a resized PTY (verify via integration test that inspects `stty size`).

## Task 2 – Share config/secrets selection across subcommands
- Introduce a helper (e.g., `commands/shared/config_loader.rs`) that encapsulates the "workspace vs --config vs --override-config" decision plus `secrets_files` handling.
- Swap both `execute_up` and `execute_exec` (and any other subcommand that loads configs) to call the helper so the same error mapping and discovery semantics apply.
- Ensure the helper returns `DeaconError::Config` variants so `up` keeps JSON contracts and `exec` surfaces identical text.
- Acceptance: invoking `deacon exec --override-config …` actually uses the override and produces the same error wording as `up` when files are missing.

## Task 3 – Single source of truth for container targeting inputs
- Expand `ContainerSelector::new` to accept `override_config_path` when workspace discovery is needed, or create a thin wrapper that wires the new config helper + selector.
- Modify `normalize_and_validate_args` (up) to rely on this common selector parsing instead of manually splitting `id_label` strings.
- Update CLI parsing tests to assert that both subcommands reject malformed labels with the same message (the regex-backed error from `ContainerSelector`).
- Acceptance: `deacon up --id-label foo` and `deacon exec --id-label foo` both fail with identical text and exit behavior.

## Task 4 – Align remote environment flag handling
- Move `NormalizedRemoteEnv::parse` into the shared helper module and reuse it for the `exec` `--env/--remote-env` flag.
- Ensure empty values (`FOO=`) survive end-to-end by covering both commands with tests.
- Acceptance: parity test verifies that both commands accept empty remote env values and expose the same validation message on format errors.

## Task 5 – Propagate compose-specific options to exec
- Thread CLI compose inputs already supported by `up` (`--env-file`, future `--container-data-folder`, etc.) through `ExecArgs`.
- Update `resolve_compose_target_container` to create the `ComposeProject` using the same CLI-provided env files and docker path as `up` so container IDs align.
- Acceptance: scenario test where `up` relies on an extra env file should allow `exec` to target the resulting compose service without manual docker tweaks.

## Task 6 – Unify environment probe + user resolution
- Factor the environment merge flow (`probe -> config remoteEnv -> CLI overrides`) into a shared utility that returns both `effective_env` and `effective_user`.
- Ensure `up` (during lifecycle execution) and `exec` share this logic so defaults like `default_user_env_probe` behave identically.
- Acceptance: add a regression test showing that setting `--default-user-env-probe login-shell` affects both subcommands equally when the config omits `userEnvProbe`.

Work through the tasks in order; each step removes duplicate code and eliminates UX drift between `up` and `exec`.
