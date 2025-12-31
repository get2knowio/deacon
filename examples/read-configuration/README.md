# Read-Configuration Examples

Self-contained examples that demonstrate the capabilities of the `read-configuration` subcommand.

How to run (from within each example directory):

```
# Pretty JSON (optional if jq is installed)
cargo run -p deacon -- read-configuration --workspace-folder "$(pwd)" | jq .

# or without jq
cargo run -p deacon -- read-configuration --workspace-folder "$(pwd)"
```

Some examples require additional flags (e.g., `--include-features-configuration`, `--include-merged-configuration`, `--override-config`, `--id-label`, or `--additional-features`). Each example's README shows the exact command to try.

Examples included:

- `basic/` — minimal config discovery and output
- `with-variables/` — variable substitution for local env and workspace folder
- `extends-chain/` — config `extends` chaining across files
- `override-config/` — overlay with `--override-config`
- `features-minimal/` — local Feature with `--include-features-configuration`
- `features-additional/` — inject Feature via `--additional-features`
- `compose/` — config referencing a Docker Compose file
- `legacy-normalization/` — legacy `containerEnv` normalized to `remoteEnv`
- `id-labels-and-devcontainerId/` — `${devcontainerId}` substitution via `--id-label`
