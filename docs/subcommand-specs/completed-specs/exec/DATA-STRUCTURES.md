# Exec Subcommand Data Structures

## ParsedInput (CLI → Internal)
```pseudocode
STRUCT ParsedInput:
    user_data_folder: string | undefined
    docker_path: string | undefined
    docker_compose_path: string | undefined
    container_data_folder: string | undefined
    container_system_data_folder: string | undefined
    workspace_folder: string | undefined
    mount_workspace_git_root: boolean
    container_id: string | undefined
    id_labels: string[] | undefined        // list of "name=value"
    config_file: URI | undefined
    override_config_file: URI | undefined
    log_level: {info|debug|trace}
    log_format: {text|json}
    term_cols: number | undefined
    term_rows: number | undefined
    default_user_env_probe: {none|loginInteractiveShell|interactiveShell|loginShell}
    remote_env_kv: string[]                // list of "name=value", value may be empty
    skip_feature_auto_mapping: boolean
    cmd: string
    args: string[]
END STRUCT
```

## ExecutionResult (Internal → Process Exit)
```pseudocode
STRUCT ExecutionResult:
    code: number | undefined               // numeric exit code, if available
    signal: number | string | undefined    // termination signal, if available
END STRUCT
```

Exit mapping: if `code` present use it; else if numeric `signal` use `128 + signal`; else if named `signal` known, map to numeric then `128 +`; else `1`.

## ContainerProperties (subset used by exec)
```pseudocode
STRUCT ContainerProperties:
    remoteWorkspaceFolder: string | undefined
    homeFolder: string
    env: map<string,string>                // container env from inspect
    remoteExec: ExecFunction               // non-PTY exec abstraction
    remotePtyExec: PtyExecFunction         // PTY exec abstraction
    user: string                           // effective remote user
END STRUCT
```

## Env Merge
```pseudocode
STRUCT EnvMerge:
    shell_env: map<string,string>          // from userEnvProbe
    cli_env: map<string,string>            // from --remote-env
    config_env: map<string,string>         // from mergedConfig.remoteEnv
    // Merge order: shell_env → cli_env → config_env
END STRUCT
```

## JSON Log Event (when `--log-format json`)
Note: The logger emits structured events; a minimal shape for command streaming is:
```json
{
  "type": "text",           // or "progress", etc.
  "level": "info",          // info|debug|trace|error
  "text": "<chunk>"         // raw command output chunk
}
```
Implementations can extend this to include timestamps, session info, and terminal dimension updates.

