# Mount Workspace Git Root Example

This example demonstrates the `--mount-workspace-git-root` flag behavior.

## Usage

### Mount Git Root (Default)

```bash
deacon read-configuration --workspace-folder . --config devcontainer.json \
  --mount-workspace-git-root true
```

This will find the Git repository root and use that as the workspace root.

### Mount Workspace Folder As-Is

```bash
deacon read-configuration --workspace-folder . --config devcontainer.json \
  --mount-workspace-git-root false
```

This will use the provided workspace folder path directly without Git root detection.

## Expected Output

The `workspace` section will have different `rootFolderPath` and `workspaceMount` values depending on the flag:

**With Git Root (true):**
```json
{
  "workspace": {
    "rootFolderPath": "/path/to/git/repo",
    "workspaceMount": "type=bind,source=/path/to/git/repo,target=/workspaces/repo"
  }
}
```

**Without Git Root (false):**
```json
{
  "workspace": {
    "rootFolderPath": "/path/to/current/folder",
    "workspaceMount": "type=bind,source=/path/to/current/folder,target=/workspaces/folder"
  }
}
```

## What It Demonstrates

- Git root detection vs direct folder mounting
- Impact on workspace paths
- Use cases for each mode

## Use Cases

### Use `--mount-workspace-git-root true` (default) when:
- Working in a Git repository
- Want to mount the entire repository
- Need access to files outside the immediate folder
- Standard monorepo or multi-project setup

### Use `--mount-workspace-git-root false` when:
- Not in a Git repository
- Want to mount only a specific subdirectory
- Working with a non-Git project structure
- Explicit path control needed
