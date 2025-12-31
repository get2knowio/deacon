# Set-Up Subcommand

- Executive Summary: Configures an existing running container as a Dev Container by applying configuration and image metadata at runtime, executing lifecycle hooks, patching environment, and optionally installing dotfiles. Returns updated configuration snapshots when requested.

- Common Use Cases:
  - Bring an externally started container under devcontainer lifecycle: `devcontainer set-up --container-id <id> --config .devcontainer/devcontainer.json`
  - Apply only image metadata hooks: `devcontainer set-up --container-id <id>`
  - Get updated configuration after applying container env substitutions: `devcontainer set-up --container-id <id> --include-merged-configuration`
  - Skip hooks for a quick env patch only: `devcontainer set-up --container-id <id> --skip-post-create`

- Documents:
  - SPEC: docs/subcommand-specs/set-up/SPEC.md
  - DIAGRAMS: docs/subcommand-specs/set-up/DIAGRAMS.md
  - DATA STRUCTURES: docs/subcommand-specs/set-up/DATA-STRUCTURES.md

- Implementation Checklist:
  - Parse and validate CLI args (`--container-id` required, `--remote-env name=value`)
  - Read optional devcontainer.json, else start from empty config
  - Inspect container; create ContainerProperties (user, env, shell server)
  - Patch `/etc/environment` and `/etc/profile` once (root)
  - Substitute variables using container env across config and merged config
  - Merge image metadata into configuration
  - If lifecycle enabled: run hooks with waitFor/skip-non-blocking semantics; install dotfiles if configured
  - Produce JSON result including requested configuration blobs
  - Return exit code 0 on success; 1 on error

