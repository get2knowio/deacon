# Exec: Workspace Folder Discovery

This example demonstrates automatic container discovery using `--workspace-folder`, which finds the container associated with a workspace directory.

## Purpose

Workspace folder discovery is the most user-friendly targeting method, automatically locating the dev container for a given workspace without needing IDs or labels.

## Prerequisites

- A dev container created from this workspace
- Understanding of workspace folder paths

## Files

- `.devcontainer/devcontainer.json` - Dev container configuration in standard location
- `src/main.js` - Sample Node.js application
- `package.json` - Node.js project metadata

## Usage

1. Start the dev container:
   ```bash
   deacon up --workspace-folder .
   ```

2. Execute commands using workspace discovery:
   ```bash
   deacon exec --workspace-folder . node --version
   ```

3. Run the application:
   ```bash
   deacon exec --workspace-folder . node /workspace/src/main.js
   ```

4. Execute from parent directory (absolute path):
   ```bash
   cd .. && deacon exec --workspace-folder "$(pwd)/workspace-folder-discovery" npm --version
   ```

5. Check workspace mapping:
   ```bash
   deacon exec --workspace-folder . pwd
   ```

## Expected Behavior

- Container is discovered by matching workspace path to `devcontainer.local_folder` label
- Config is read from `.devcontainer/devcontainer.json` or `.devcontainer.json`
- If no container exists for the workspace: "Dev container not found."
- If config file doesn't exist but container is found: container still usable (no config error)

## Discovery Process

1. Resolve `--workspace-folder` to absolute path
2. Look for config at `.devcontainer/devcontainer.json`, then `.devcontainer.json`
3. Search for container with label `devcontainer.local_folder=<abs-path>`
4. Apply merged configuration and image metadata

## Notes

- Workspace folder must be absolute or will be resolved relative to CWD
- Config discovery is optional; if absent, container can still be targeted if it exists
- The `--mount-workspace-git-root` flag (default: true) affects workspace resolution when reading config
