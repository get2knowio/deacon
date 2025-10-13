## Data Structures

```pseudocode
STRUCT RunUserCommandsArgs:
  userDataFolder: string | undefined            // --user-data-folder (host persisted path)
  dockerPath: string | undefined                // --docker-path
  dockerComposePath: string | undefined         // --docker-compose-path
  containerDataFolder: string | undefined       // --container-data-folder
  containerSystemDataFolder: string | undefined // --container-system-data-folder
  workspaceFolder: string | undefined           // --workspace-folder
  mountWorkspaceGitRoot: boolean                // --mount-workspace-git-root
  containerId: string | undefined               // --container-id
  idLabel: string[] | undefined                 // --id-label (repeatable)
  configPath: string | undefined                // --config
  overrideConfigPath: string | undefined        // --override-config
  logLevel: 'info' | 'debug' | 'trace'          // --log-level
  logFormat: 'text' | 'json'                    // --log-format
  terminalRows: number | undefined              // --terminal-rows
  terminalColumns: number | undefined           // --terminal-columns
  defaultUserEnvProbe: 'none'|'loginInteractiveShell'|'interactiveShell'|'loginShell'
  skipNonBlocking: boolean                      // --skip-non-blocking-commands
  prebuild: boolean                              // --prebuild
  stopForPersonalization: boolean                // --stop-for-personalization
  remoteEnv: string[]                            // --remote-env (repeatable: KEY=VALUE)
  skipFeatureAutoMapping: boolean                // --skip-feature-auto-mapping (hidden)
  skipPostAttach: boolean                        // --skip-post-attach
  dotfilesRepository: string | undefined         // --dotfiles-repository
  dotfilesInstallCommand: string | undefined     // --dotfiles-install-command
  dotfilesTargetPath: string                     // --dotfiles-target-path
  containerSessionDataFolder: string | undefined // --container-session-data-folder
  secretsFile: string | undefined                // --secrets-file
END STRUCT

STRUCT ParsedInput:
  args: RunUserCommandsArgs
  providedIdLabels: string[] | undefined
  addRemoteEnvs: string[]
  configFile: URI | undefined
  overrideConfigFile: URI | undefined
END STRUCT

STRUCT ResolverParameters:
  // logging/context
  package: PackageConfiguration
  sessionId: string
  sessionStart: Date
  cliHost: CLIHost
  output: Log
  env: ProcessEnv
  cwd: string
  // behavior
  defaultUserEnvProbe: UserEnvProbe
  lifecycleHook: LifecycleHook
  remoteEnv: Record<string,string>
  prebuild: boolean
  skipPostAttach: boolean
  containerDataFolder: string | undefined
  containerSystemDataFolder: string | undefined
  containerSessionDataFolder: string | undefined
  dotfilesConfiguration: DotfilesConfiguration
  secretsP: Promise<Record<string,string>> | undefined
END STRUCT

STRUCT ContainerProperties:
  createdAt: string | undefined
  startedAt: string | undefined
  osRelease: { hardware: string, id: string, version: string }
  user: string
  gid: string | undefined
  env: Record<string,string>
  shell: string            // e.g., /bin/bash
  homeFolder: string       // e.g., /home/vscode
  userDataFolder: string   // e.g., /home/vscode/.devcontainer
  installFolder: string    // e.g., /workspaces/.devcontainer
  remoteWorkspaceFolder: string | undefined
  remoteExec: ExecFunction
  remotePtyExec: PtyExecFunction
  shellServer: ShellServer
  launchRootShellServer?: () -> Promise<ShellServer>
END STRUCT

TYPE LifecycleCommand = string | string[] | Map<string, string|string[]>

STRUCT LifecycleHooksInstallMap:
  onCreateCommand: Array<{ origin: string, command: LifecycleCommand }>
  updateContentCommand: Array<{ origin: string, command: LifecycleCommand }>
  postCreateCommand: Array<{ origin: string, command: LifecycleCommand }>
  postStartCommand: Array<{ origin: string, command: LifecycleCommand }>
  postAttachCommand: Array<{ origin: string, command: LifecycleCommand }>
  initializeCommand: Array<{ origin: string, command: LifecycleCommand }>
END STRUCT

STRUCT MergedDevContainerConfig:
  // effective values after merging image metadata and config
  onCreateCommands?: LifecycleCommand[]
  updateContentCommands?: LifecycleCommand[]
  postCreateCommands?: LifecycleCommand[]
  postStartCommands?: LifecycleCommand[]
  postAttachCommands?: LifecycleCommand[]
  waitFor?: 'initializeCommand'|'onCreateCommand'|'updateContentCommand'|'postCreateCommand'|'postStartCommand'|'postAttachCommand'
  userEnvProbe?: 'none'|'loginInteractiveShell'|'interactiveShell'|'loginShell'
  remoteEnv?: Record<string, string | null>
  containerEnv?: Record<string, string | null>
  remoteUser?: string | undefined
  containerUser?: string | undefined
  // ...other merged properties (mounts, entrypoints, customizations, etc.)
END STRUCT

STRUCT ExecutionResultSuccess:
  outcome: 'success'
  result: 'skipNonBlocking' | 'prebuild' | 'stopForPersonalization' | 'done'
END STRUCT

STRUCT ExecutionResultError:
  outcome: 'error'
  message: string
  description: string
END STRUCT

TYPE ExecutionResult = ExecutionResultSuccess | ExecutionResultError
```

### JSON Output Schema (stdout)

```json
// Success
{ "outcome": "success", "result": "done" }

// Error
{ "outcome": "error", "message": "...", "description": "..." }
```

