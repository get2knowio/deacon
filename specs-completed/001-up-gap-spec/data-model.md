# Data Model: Devcontainer Up Gap Closure

## Entities

### Command Invocation
- **Purpose**: Represents user-supplied CLI inputs for `deacon up`.
- **Fields**:
  - workspace_folder?: path
  - config_path?: path
  - override_config_path?: path
  - id_labels?: map<string, string>
  - mount_workspace_git_root: bool
  - terminal_dimensions?: { columns: u32, rows: u32 }
  - runtime_flags: { remove_existing: bool, build_no_cache: bool, expect_existing: bool, workspace_mount_consistency: enum, gpu_availability: enum, default_user_env_probe: enum, update_remote_user_uid_default: enum }
  - mounts?: list<MountSpec>
  - remote_env?: map<string, string>
  - cache_from?: list<string>
  - cache_to?: string
  - buildkit_mode: enum
  - additional_features?: map<string, value>
  - skip_feature_auto_mapping: bool
  - dotfiles?: { repository?: string, install_command?: string, target_path?: path }
  - omit_metadata_flags?: { omit_config_remote_env_from_metadata: bool, omit_syntax_directive: bool }
  - data_folders?: { container_data_folder?: path, container_system_data_folder?: path, user_data_folder?: path, container_session_data_folder?: path }
  - output_flags?: { include_configuration: bool, include_merged_configuration: bool }
  - secrets_files?: list<path>
  - runtime_paths?: { docker_path?: path, docker_compose_path?: path }
  - lifecycle_control: { skip_post_create: bool, skip_post_attach: bool, prebuild: bool, skip_non_blocking: bool }

### MountSpec
- **Purpose**: Captures validated mount entries from CLI.
- **Fields**:
  - type: enum (bind|volume)
  - source: string
  - target: string
  - external?: bool

### Resolved Devcontainer Configuration
- **Purpose**: The merged configuration after applying files, overrides, image metadata, features, dotfiles, mounts, env, and substitutions.
- **Fields**:
  - config: canonical devcontainer configuration (post-substitution)
  - merged_configuration: configuration merged with image/feature metadata and runtime overrides
  - id_labels: final labels used to find/create containers
  - features_applied: list of features and provenance
  - lifecycle_hooks: initialize, onCreate, updateContent, postCreate, postStart, postAttach
  - security: { user, gid, shell, home, capabilities, security_opts, init, privileged }
  - compose_context?: { files: list<path>, env_file?: path, project_name: string, profiles?: list<string> }

### Execution Result
- **Purpose**: Structured outcome for success or error cases.
- **Fields (success)**:
  - outcome: "success"
  - container_id: string
  - compose_project_name?: string
  - remote_user: string
  - remote_workspace_folder: string
  - configuration?: object (when include_configuration)
  - merged_configuration?: object (when include_merged_configuration)
- **Fields (error)**:
  - outcome: "error"
  - message: string
  - description: string
  - container_id?: string
  - disallowed_feature_id?: string
  - did_stop_container?: bool
  - learn_more_url?: string

## Relationships and Rules
- Command Invocation is validated and normalized into Resolved Devcontainer Configuration inputs before any runtime interaction.
- Resolved Devcontainer Configuration determines container creation/update paths and feeds Execution Result.
- MountSpec items are derived from Command Invocation and reused for docker and compose flows (conversion when compose).
- Secrets and remote env maps feed both runtime env and lifecycle env with redaction enforced in logging/output.
- Success/Failure outcomes are mutually exclusive and must emit exactly one JSON document on stdout per execution.
