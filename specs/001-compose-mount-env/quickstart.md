# Quickstart: Compose mount & env injection

1) Prepare a compose project with a clear primary service and any external volumes declared. Ensure env-files and project name are set as desired.

2) Run the `up` subcommand with CLI mounts and remote env, e.g.:
```
deacon up --service <primary> --mount /host/path:/work -e KEY=VALUE --remote-env FOO=BAR
```
Include `--mount-workspace-git-root` when Git root mount is desired. Select profiles and env-files as usual; project naming remains as configured.

3) Verify inside the primary service container:
- CLI mounts (including Git root when enabled) are present at requested paths.
- Remote env values are visible and override conflicting env-file/service defaults.
- External volumes remain attached with existing data intact.
- Profiles/env-files/project naming are preserved (only selected services start, resource names maintain the configured prefix).
- Attempt a run with a deliberately missing external volume to confirm compose surfaces an error and no bind fallback is injected.

4) Testing cadence:
- Fast loop: `make test-nextest-fast`
- Docker-focused changes: `make test-nextest-docker` (and smoke if required)
- Pre-PR: `make test-nextest` plus `cargo fmt --all && cargo fmt --all -- --check` and `cargo clippy --all-targets -- -D warnings`

5) Performance check:
- Measure `deacon up` startup time with and without injection to ensure no noticeable additional delay; record observations here.

6) Validation recording:
- Note outcomes for injected mounts/env visibility (US1), external volume preservation and missing-volume error behavior (US2), and profiles/env-files/project naming retention (US3) in this quickstart as scenarios are exercised.
