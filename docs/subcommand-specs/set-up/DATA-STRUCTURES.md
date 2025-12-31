# Set-Up Subcommand Data Structures

## Input Structures

```pseudocode
STRUCT SetUpOptions:
    dockerPath: string?
    containerDataFolder: string?
    containerSystemDataFolder: string?
    containerSessionDataFolder: string?
    containerId: string
    configFile: URI?
    logLevel: 'info' | 'debug' | 'trace'
    logFormat: 'text' | 'json'
    terminalDimensions: { columns: number, rows: number }?
    defaultUserEnvProbe: 'none' | 'loginInteractiveShell' | 'interactiveShell' | 'loginShell'
    postCreateEnabled: boolean
    skipNonBlocking: boolean
    remoteEnv: map<string,string>
    persistedFolder: string?
    dotfiles: DotfilesConfiguration
    includeConfig: boolean
    includeMergedConfig: boolean
END STRUCT

STRUCT DotfilesConfiguration:
    repository: string?
    installCommand: string?
    targetPath: string
END STRUCT
```

## Normalized CLI Args (ParsedInput)
```pseudocode
STRUCT ParsedInput:
    containerId: string
    configFile: URI?
    logLevel: 'info' | 'debug' | 'trace'
    logFormat: 'text' | 'json'
    terminal: { columns: number, rows: number }?
    lifecycle: { postCreateEnabled: boolean, skipNonBlocking: boolean }
    remoteEnv: map<string,string>
    dotfiles: DotfilesConfiguration
    containerDataFolder: string?
    containerSystemDataFolder: string?
    containerSessionDataFolder: string?
    includeConfig: boolean
    includeMergedConfig: boolean
    dockerPath: string?
    userDataFolder: string?
END STRUCT
```

## Resolver and Container Properties
```pseudocode
STRUCT ResolverParameters:
    // Logging, environment, session, output sinks
    computeExtensionHostEnv: boolean
    defaultUserEnvProbe: 'none' | 'loginInteractiveShell' | 'interactiveShell' | 'loginShell'
    lifecycleHook: LifecycleHook
    remoteEnv: map<string,string>
    dotfilesConfiguration: DotfilesConfiguration
    containerDataFolder: string?
    containerSystemDataFolder: string?
    containerSessionDataFolder: string?
END STRUCT

STRUCT LifecycleHook:
    enabled: boolean
    skipNonBlocking: boolean
    output: Log
    onDidInput: Event<string>
    done: () -> void
END STRUCT

STRUCT ContainerProperties:
    createdAt: string?
    startedAt: string?
    osRelease: { hardware: string, id: string, version: string }
    user: string
    gid: string?
    env: map<string,string>
    shell: string
    homeFolder: string
    userDataFolder: string           // typically ~/.devcontainer
    remoteWorkspaceFolder: string?
    remoteExec: ExecFunction
    remotePtyExec: PtyExecFunction
    remoteExecAsRoot: ExecFunction?
    shellServer: ShellServer
    launchRootShellServer: () -> Promise<ShellServer>?
END STRUCT
```

## Merged Configuration Shapes
```pseudocode
STRUCT CommonDevContainerConfig:
    remoteEnv?: map<string, string | null>
    onCreateCommand?: LifecycleCommand
    updateContentCommand?: LifecycleCommand
    postCreateCommand?: LifecycleCommand
    postStartCommand?: LifecycleCommand
    postAttachCommand?: LifecycleCommand
    waitFor?: 'initializeCommand' | 'onCreateCommand' | 'updateContentCommand' | 'postCreateCommand' | 'postStartCommand' | 'postAttachCommand'
    userEnvProbe?: 'none' | 'loginInteractiveShell' | 'interactiveShell' | 'loginShell'
END STRUCT

STRUCT CommonMergedDevContainerConfig EXTENDS CommonDevContainerConfig:
    entrypoints?: string[]
    onCreateCommands?: LifecycleCommand[]
    updateContentCommands?: LifecycleCommand[]
    postCreateCommands?: LifecycleCommand[]
    postStartCommands?: LifecycleCommand[]
    postAttachCommands?: LifecycleCommand[]
END STRUCT

TYPE LifecycleCommand = string | string[] | map<string, string | string[]>

STRUCT LifecycleHooksInstallMap:
    onCreateCommand: { origin: string, command: LifecycleCommand }[]
    updateContentCommand: { origin: string, command: LifecycleCommand }[]
    postCreateCommand: { origin: string, command: LifecycleCommand }[]
    postStartCommand: { origin: string, command: LifecycleCommand }[]
    postAttachCommand: { origin: string, command: LifecycleCommand }[]
    initializeCommand: { origin: string, command: LifecycleCommand }[]
END STRUCT
```

## Set-Up Output (stdout JSON)

Success case:
```json
{
  "outcome": "success",
  "configuration": { "...": "included when --include-configuration" },
  "mergedConfiguration": { "...": "included when --include-merged-configuration" }
}
```

Error case:
```json
{
  "outcome": "error",
  "message": "<string>",
  "description": "<string>"
}
```

Notes:
- Exit code is 0 for success, 1 for error.
- The container id is not returned (caller already provided it).

