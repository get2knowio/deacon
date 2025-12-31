# Data Model: Exec Subcommand

Date: 2025-11-16

## Entities

### TargetContainer
- id: string (container ID) — optional
- labels: map<string, string> — optional, repeatable filters
- workspace_folder: string (absolute path) — optional
- selection_precedence: enum { container_id, id_label, workspace_folder }
- state: enum { running, stopped, unknown }

Validation:
- At least one of `id`, `labels`, or `workspace_folder` MUST be provided (FR-001).
- `labels` entries MUST be `name=value` with non-empty name and value (FR-002).

### EffectiveConfiguration
- user_env_probe: enum { loginInteractiveShell, loginShell, interactiveShell, none }
- remote_env_config: map<string, string>
- remote_user_config: string | null
- runtime_paths:
  - docker_path: string | null
  - docker_compose_path: string | null
  - container_data_folder: string | null
  - container_system_data_folder: string | null

Validation:
- If config not found during discovery, error: "Dev container config (<path>) not found." (FR-006).

### ExecutionEnvironment
- base_shell_env: map<string, string> (from userEnvProbe)
- config_env: map<string, string> (from config remoteEnv)
- cli_env: map<string, string> (from repeated --remote-env)
- effective_env: map<string, string> (merge result; CLI wins) (FR-004)
- remote_user_effective: string (CLI --user overrides config) (FR-013)

## Merge Rules
1. Start with `base_shell_env` per `user_env_probe` (or default from `--default-user-env-probe`) (FR-005).
2. Overlay `config_env` from configuration `remoteEnv` (FR-004).
3. Overlay `cli_env` from CLI `--remote-env` entries; allow empty values (FR-003).

## Execution Semantics
- PTY Allocation: enabled when stdin/stdout are TTYs; forced when `--log-format json` is set; size overridable via `--terminal-columns` and `--terminal-rows` (FR-009).
- Exit Codes: propagate child exit code; if terminated by signal N, report 128+N; default to 1 if unknown (FR-010).
- Logging: level selectable; do not mix logs with stdout in JSON mode; logs to stderr (FR-012).

## Relationships
- TargetContainer uses EffectiveConfiguration to determine runtime params and env probing strategy.
- ExecutionEnvironment derived from EffectiveConfiguration plus CLI overlays.
