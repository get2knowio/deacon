# Data Model: Read-Configuration Spec Parity

Date: 2025-10-31
Branch: 001-read-config-parity
Spec: ./spec.md

## Entities

### ParsedInput
- workspace_folder?: string (path)
- config_file?: string (path/URI)
- override_config_file?: string (path/URI)
- user_data_folder?: string
- docker_path: string (default: "docker")
- docker_compose_path: string (default: "docker-compose" or `docker compose`)
- mount_workspace_git_root: boolean (default: true)
- container_id?: string
- id_label: string[] (each `<name>=<value>`)
- terminal_columns?: number
- terminal_rows?: number
- include_features_configuration: boolean (default: false)
- include_merged_configuration: boolean (default: false)
- additional_features: object (JSON)
- skip_feature_auto_mapping: boolean (default: false)
- log_level: enum { error, warn, info, debug, trace }
- log_format: enum { text, json }

Validation:
- At least one of container_id, id_label non-empty, workspace_folder MUST be provided
- Every id_label MUST match `/.+=.+/`
- terminal_columns and terminal_rows MUST be provided together or omitted together

### ReadConfigurationOutput
- configuration: object (DevContainerConfig after pre-container substitutions)
- featuresConfiguration?: object (present when include_features_configuration)
- mergedConfiguration?: object (present when include_merged_configuration)

Notes:
- When only container flags provided: configuration = {}
- When container selected and merged requested: metadata obtained via inspect; on failure → error
- `${devcontainerId}` computed from sorted id-label pairs
