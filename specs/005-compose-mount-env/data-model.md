# Data Model: Compose mount & env injection

## Entities

### ComposeProject
- **Fields**: compose_files (ordered list of file paths), profiles (selected profile names), env_files (ordered list), project_name (string), services (map service_name → ComposeService), external_volumes (set of volume names flagged external).
- **Relationships**: Owns ComposeService entries; references ExternalVolume definitions by name.
- **Validation Rules**: Preserve declaration order for files/env-files; require project_name to remain consistent across injections; external_volumes must not be replaced or mutated by injection.

### ComposeService
- **Fields**: name (string), mounts (ordered list of Mount), environment (ordered map env_key → env_value), volumes (ordered list of VolumeRef), profiles (set of profile names).
- **Relationships**: Belongs to ComposeProject; may reference ExternalVolume entries.
- **Validation Rules**: Injection targets only the primary service unless user explicitly broadens scope; injected mounts/env must preserve existing compose-defined entries (CLI env overrides same keys, mounts append without dropping existing service mounts).

### PrimaryServiceSelection
- **Fields**: service_name (string), source (auto-detected or user-specified).
- **Relationships**: Resolves to one ComposeService within ComposeProject.
- **Validation Rules**: Must resolve exactly one service; errors if ambiguous or missing.

### Mount
- **Fields**: source_path (host), target_path (container), type (bind), options (rw/ro, consistency flags).
- **Relationships**: Used within ComposeService.mounts; includes mountWorkspaceGitRoot when enabled.
- **Validation Rules**: Paths must be normalized; mountWorkspaceGitRoot follows same option set and target-path rules as other CLI mounts; injection must not drop existing service mounts.

### RemoteEnvironmentEntry
- **Fields**: key (string), value (string), source (remote/CLI).
- **Relationships**: Merged into ComposeService.environment.
- **Validation Rules**: CLI/remote entries override duplicate keys from env-files/service defaults; non-conflicting keys remain untouched; null/empty not allowed for injected entries.

### VolumeRef
- **Fields**: name (string), is_external (bool), mount_point (container path), options (e.g., rw).
- **Relationships**: References ExternalVolume when is_external is true.
- **Validation Rules**: External volume refs must stay intact; injection must not rewrite or rename them.

### ExternalVolume
- **Fields**: name (string), driver/options (opaque), scope (external).
- **Relationships**: Referenced by VolumeRef entries.
- **Validation Rules**: Must remain external; absence should surface compose error rather than auto-creation by injection logic.

## State/Flow Notes
- Injection occurs before compose up execution so mounts/env are present at container start.
- Profiles/env-files/project naming configured on the project are preserved throughout injection; no generated override files.
- Non-target services retain their original mounts/env unless user explicitly opts in to broader injection.
