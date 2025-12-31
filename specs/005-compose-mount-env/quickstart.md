# Quickstart: Compose mount & env injection

## Implementation Summary

Mount and environment injection for compose workflows is now implemented without creating temporary override files. The injection uses stdin piping (`docker compose -f -`) to pass an inline YAML override to docker compose, ensuring:

- No temp files left on disk
- Mounts/env are applied only to the primary service
- External volumes remain untouched
- Profiles, env-files, and project naming are preserved

## Usage

1) Prepare a compose project with a clear primary service and any external volumes declared. Ensure env-files and project name are set as desired.

2) Run the `up` subcommand with CLI mounts and remote env, e.g.:
```bash
deacon up --service <primary> --mount /host/path:/work -e KEY=VALUE --remote-env FOO=BAR
```
Include `--mount-workspace-git-root` when Git root mount is desired. Select profiles and env-files as usual; project naming remains as configured.

3) Verify inside the primary service container:
- CLI mounts (including Git root when enabled) are present at requested paths.
- Remote env values are visible and override conflicting env-file/service defaults.
- External volumes remain attached with existing data intact.
- Profiles/env-files/project naming are preserved (only selected services start, resource names maintain the configured prefix).
- Attempt a run with a deliberately missing external volume to confirm compose surfaces an error and no bind fallback is injected.

## Testing Cadence

- Fast loop: `make test-nextest-fast`
- Docker-focused changes: `make test-nextest-docker` (and smoke if required)
- Pre-PR: `make test-nextest` plus `cargo fmt --all && cargo fmt --all -- --check` and `cargo clippy --all-targets -- -D warnings`

## Validation Results

### US1: Mount/Env Injection (T006-T009)
- [x] Implemented inline YAML injection via stdin (`-f -`)
- [x] No temporary override files created
- [x] CLI mounts parsed and converted to compose volume format
- [x] Remote env applied with CLI precedence over env-files/service defaults
- [x] Env merge helper: `ComposeProject::merge_env_with_cli_precedence()`

### US2: External Volumes & Git Root (T010-T011)
- [x] External volumes preserved (injection only adds volumes, doesn't modify top-level declarations)
- [x] Missing external volumes surface compose errors (no bind fallback)
- [x] `mount_workspace_git_root` handled upstream, resolved path passed to compose flow

### US3: Profiles/Env-files/Project Naming (T012-T013)
- [x] `ComposeCommand` threads profiles via `--profile` flags
- [x] Env-files passed via `--env-file` flags
- [x] Project name passed via `-p` flag
- [x] Injection targets only primary service (non-target services unaffected)

## Performance Notes

The stdin-based injection approach has minimal overhead compared to file-based overrides:
- No disk I/O for temp file creation/cleanup
- Single process invocation with piped input
- YAML generation is O(n) where n = mounts + env vars

No measurable delay observed in typical workflows.
