# Exec: Container ID Targeting

This example demonstrates using `--container-id` to directly target a specific container for command execution.

## Purpose

When you know the exact container ID, `--container-id` provides the most direct targeting method. This is useful in automation scenarios where container IDs are tracked externally.

## Prerequisites

- A running dev container (use `deacon up` in this directory first)
- The container ID from `docker ps`

## Files

- `devcontainer.json` - Basic dev container configuration
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

5. Check environment variables:
   ```bash
   deacon exec --container-id $CONTAINER_ID env | grep CONTAINER_
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
