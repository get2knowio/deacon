# Up Subcommand Data Structures

## Input Structures

```pseudocode
STRUCT ProvisionOptions:
    dockerPath: string?
    dockerComposePath: string?
    containerDataFolder: string?
    containerSystemDataFolder: string?
    workspaceFolder: string?
    workspaceMountConsistency: 'consistent' | 'cached' | 'delegated' | undefined
    gpuAvailability: 'all' | 'detect' | 'none' | undefined
    mountWorkspaceGitRoot: boolean
    configFile: URI?
    overrideConfigFile: URI?
    logLevel: 'info' | 'debug' | 'trace'
    logFormat: 'text' | 'json'
    log: (text: string) -> void
    terminalDimensions: { columns: number, rows: number }?
    defaultUserEnvProbe: 'none' | 'loginInteractiveShell' | 'interactiveShell' | 'loginShell'
    removeExistingContainer: boolean
    buildNoCache: boolean
    expectExistingContainer: boolean
    postCreateEnabled: boolean
    skipNonBlocking: boolean
    prebuild: boolean
    persistedFolder: string?
    additionalMounts: Mount[]
    updateRemoteUserUIDDefault: 'never' | 'on' | 'off'
    remoteEnv: map<string,string>
    additionalCacheFroms: string[]
    useBuildKit: 'auto' | 'never'
    buildxPlatform: string?
    buildxPush: boolean
    additionalLabels: string[]
    buildxOutput: string?
    buildxCacheTo: string?
    additionalFeatures: map<string, string | boolean | map<string, string | boolean>>
    skipFeatureAutoMapping: boolean
    skipPostAttach: boolean
    containerSessionDataFolder: string?
    skipPersistingCustomizationsFromFeatures: boolean
    omitConfigRemotEnvFromMetadata: boolean?
    dotfiles: DotfilesConfiguration
    experimentalLockfile: boolean?
    experimentalFrozenLockfile: boolean?
    secretsP: Promise<map<string,string>>?
    omitSyntaxDirective: boolean?
    includeConfig: boolean?
    includeMergedConfig: boolean?
END STRUCT

STRUCT Mount:
    type: 'bind' | 'volume'
    source: string
    target: string
    external: boolean?
END STRUCT

STRUCT DotfilesConfiguration:
    repository: string?
    installCommand: string?
    targetPath: string?
END STRUCT
```

## Normalized CLI Args (ParsedInput)
```pseudocode
STRUCT ParsedInput:
    workspaceFolder: string?
    providedIdLabels: string[]?
    addRemoteEnvs: string[]
    addCacheFroms: string[]
    additionalFeatures: map<string, string | boolean | map<string, string | boolean>>
    overrideConfigFile: URI?
    configFile: URI?
    // Plus all switches impacting ProvisionOptions as booleans/strings
END STRUCT
```

## Resolver and Result Structures
```pseudocode
STRUCT ResolverParameters:
    // Logging, environment, session, output sinks
END STRUCT

STRUCT DockerResolverParameters:
    common: ResolverParameters
    dockerCLI: string
    dockerComposeCLI: () -> DockerComposeCLI
    isPodman: boolean
    dockerEnv: map<string,string>
    workspaceMountConsistencyDefault: 'consistent' | 'cached' | 'delegated' | undefined
    gpuAvailability: 'all' | 'detect' | 'none'
    mountWorkspaceGitRoot: boolean
    updateRemoteUserUIDOnMacOS: boolean
    cacheMount: 'volume' | 'bind' | 'none'
    removeOnStartup: boolean | string | undefined
    buildNoCache: boolean?
    expectExistingContainer: boolean?
    additionalMounts: Mount[]
    updateRemoteUserUIDDefault: 'never' | 'on' | 'off'
    additionalCacheFroms: string[]
    buildKitVersion: { versionString: string, versionMatch?: string }?
    isTTY: boolean
    experimentalLockfile: boolean?
    experimentalFrozenLockfile: boolean?
    buildxPlatform: string?
    buildxPush: boolean
    additionalLabels: string[]
    buildxOutput: string?
    buildxCacheTo: string?
    platformInfo: { os: string, arch: string, variant?: string }
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
    userDataFolder: string
    remoteWorkspaceFolder: string?
    remoteExec: ExecFunction
    remotePtyExec: PtyExecFunction
    remoteExecAsRoot: ExecFunction?
    shellServer: ShellServer
END STRUCT

STRUCT ResolverResult:
    params: ResolverParameters
    properties: ContainerProperties
    config: CommonDevContainerConfig
    mergedConfig: CommonMergedDevContainerConfig
    resolvedAuthority: { extensionHostEnv?: map<string, string | null> }
    tunnelInformation: { environmentTunnels?: { remoteAddress: { host: string, port: number }, localAddress: string }[] }
    dockerContainerId: string
    composeProjectName: string?
END STRUCT
```

## Up Output (stdout JSON)

Success case:
```json
{
  "outcome": "success",
  "containerId": "<string>",
  "composeProjectName": "<string, optional>",
  "remoteUser": "<string>",
  "remoteWorkspaceFolder": "<string>",
  "configuration": { "...": "included when --include-configuration" },
  "mergedConfiguration": { "...": "included when --include-merged-configuration" }
}
```

Error case:
```json
{
  "outcome": "error",
  "message": "<string>",
  "description": "<string>",
  "containerId": "<string, optional>",
  "disallowedFeatureId": "<string, optional>",
  "didStopContainer": true,
  "learnMoreUrl": "<string, optional>"
}
```

Notes:
- Exit code is 0 for success, 1 for error.
- When success, background tasks are awaited via finishBackgroundTasks() before disposal.

## Lifecycle Commands and Execution
```pseudocode
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

## Compose-Specific Data
```pseudocode
STRUCT ComposeContext:
    composeFiles: string[]
    envFile: string?
    projectName: string
    serviceName: string
END STRUCT
```

## Mount Conversion (for Compose YAML)
```pseudocode
FUNCTION convertMountToVolume(m: Mount) -> string
    // Produces "type,src,dst" style entries suitable for compose "volumes" section
END FUNCTION
```

## JSON Schema (informal)
```pseudocode
SCHEMA UpResult:
  outcome: enum('success','error')
  when success:
    containerId: string
    composeProjectName?: string
    remoteUser: string
    remoteWorkspaceFolder: string
    configuration?: object
    mergedConfiguration?: object
  when error:
    message: string
    description: string
    containerId?: string
    disallowedFeatureId?: string
    didStopContainer?: boolean
    learnMoreUrl?: string
END SCHEMA
```

