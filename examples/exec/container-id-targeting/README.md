# Exec: Container ID Targeting

This example demonstrates using `--container-id` to directly target a specific container for command execution.

## Purpose

When you know the exact container ID, `--container-id` provides the most direct targeting method. This is useful in automation scenarios where container IDs are tracked externally.

## Prerequisites

- A running dev container (use `deacon up` in this directory first)
- The container ID from `docker ps`

## Files

- `.devcontainer.json` - Basic dev container configuration
- `test-script.sh` - A simple script to demonstrate execution

## Usage

1. Start the dev container:
   ```bash
   deacon up --workspace-folder .
   ```

2. Get the container ID:
   ```bash
   CONTAINER_ID=$(docker ps --filter "label=devcontainer.local_folder=$(pwd)" --format "{{.ID}}" | head -n1)
   ```

3. Execute a command using the container ID:
   ```bash
   deacon exec --container-id $CONTAINER_ID echo "Hello from container"
   ```

4. Run the test script:
   ```bash
   deacon exec --container-id $CONTAINER_ID bash /workspace/test-script.sh
   ```

5. Check environment variables — you get the **container's own** environment,
   with none of the config's `remoteEnv` applied:
   ```bash
   deacon exec --container-id $CONTAINER_ID env
   ```

6. Contrast: naming the workspace gives deacon a config to resolve, so
   `remoteEnv` **is** applied:
   ```bash
   deacon exec --workspace-folder . --mount-workspace-git-root false env | grep CONTAINER_ENV_VAR
   # CONTAINER_ENV_VAR=set-by-config
   ```

## Expected Behavior

- Commands execute directly in the specified container
- No config file discovery is performed
- Exit codes propagate from the executed command
- stdout/stderr are streamed to the terminal

## Notes

- Container ID can be abbreviated (first 12 characters typically sufficient)
- This method bypasses workspace/config discovery entirely
- The container must be running; stopped containers will error

### Targeting mode determines config fidelity

`--container-id` names a *container*, not a *workspace*. deacon loads no
`devcontainer.json` on this path, so nothing from the config applies — notably
`remoteUser` and `remoteEnv`. That is what "bypasses workspace/config discovery
entirely" means above, and step 5 asserts it positively so the two targeting
modes stay legible.

If you want config semantics, name the config: `--workspace-folder` (step 6),
`--config`, or `--id-label`.

A fuller `--container-id` — one that recovers the merged config from the
container's `devcontainer.metadata` label, the way `set-up` and
`read-configuration` already do — would need deacon to *write* that label in the
first place; today it inherits the base image's label verbatim and emits none of
its own. Tracked separately; until then, this example documents the behavior
that actually exists rather than the one we might prefer.
