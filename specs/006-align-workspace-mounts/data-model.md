# Data Model: Workspace Mount Consistency and Git-Root Handling

## Entities

### WorkspaceDiscoveryInput
- `cwd`: Absolute path of CLI invocation.
- `workspace_root`: Path detected or provided as workspace root (may equal `cwd`).
- `git_root_requested`: Boolean flag indicating whether git-root mounting is enabled.
- `consistency`: Optional workspace mount consistency value provided by user.
- `runtime_target`: Enum-like indicator of output mode (docker | compose) used only for rendering, not for path selection.

### WorkspaceResolution
- `detected_workspace_root`: Final workspace root used when git-root is not requested or not found.
- `detected_git_root`: Optional path to repository top-level when discovery succeeds.
- `effective_host_path`: Host path chosen for the workspace mount (git root when requested and found; otherwise workspace root).
- `discovery_notes`: Optional human-readable note when falling back (e.g., git root missing).

### WorkspaceMountDefinition
- `host_path`: Host path to mount (from `effective_host_path`).
- `container_path`: Target path inside container (as defined by existing runtime defaults/config).
- `consistency`: Consistency value applied to the mount if provided; otherwise runtime default.
- `applies_to`: Scope of the mount rendering (docker single service | compose service list).

## Relationships
- `WorkspaceDiscoveryInput` feeds `WorkspaceResolution`.
- `WorkspaceResolution.effective_host_path` is used to build one or more `WorkspaceMountDefinition` instances for Docker and Compose outputs.
- `WorkspaceResolution.consistency` flows into each `WorkspaceMountDefinition`.

## Validation Rules
- If `git_root_requested` is true and `detected_git_root` exists, `effective_host_path` MUST use `detected_git_root`.
- If `git_root_requested` is true and `detected_git_root` is missing, `effective_host_path` MUST fall back to `detected_workspace_root` and produce a `discovery_notes` entry.
- Consistency value, when provided, MUST be applied to every generated workspace mount regardless of runtime target.
- Compose outputs MUST apply the same `effective_host_path` and `consistency` to every service receiving the workspace mount.
