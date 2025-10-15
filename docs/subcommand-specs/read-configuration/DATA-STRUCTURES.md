# Read-Configuration Subcommand Data Structures

## CLI/Parsed Input

```pseudocode
STRUCT ParsedInput:
    user_data_folder: string?                  // accepted but unused by this command
    docker_path: string                        // default "docker"
    docker_compose_path: string                // default "docker-compose" (v2 via docker compose handled upstream)
    workspace_folder: string?                  // used to discover config and id-labels
    mount_workspace_git_root: boolean          // default true; influences workspace discovery/mount
    container_id: string?                      // optional direct container selection
    id_label: string[]                         // repeatable; format <name>=<value>
    config_file: URI?                          // explicit devcontainer.json path
    override_config_file: URI?                 // optional override; required when no base config exists
    log_level: 'info' | 'debug' | 'trace'      // default 'info'
    log_format: 'text' | 'json'                // default 'text' (affects stderr logging only)
    terminal_columns: number?                  // implies terminal_rows
    terminal_rows: number?                     // implies terminal_columns
    include_features_configuration: boolean    // include Features resolution section
    include_merged_configuration: boolean      // include config merged with image metadata
    additional_features: map<string, string | boolean | map<string, string | boolean>>
    skip_feature_auto_mapping: boolean         // hidden/testing
END STRUCT
```

## Output Payload

```pseudocode
STRUCT ReadConfigurationOutput:
    configuration: DevContainerConfig                    // substituted config from file(s); may be {}
    workspace: WorkspaceConfig?                          // resolved workspaceFolder/mounts; omitted if none
    featuresConfiguration: FeaturesConfig?               // present if requested or needed
    mergedConfiguration: MergedDevContainerConfig?       // present if requested
END STRUCT
```

## DevContainerConfig (selected fields)

```pseudocode
STRUCT DevContainerConfig:
    configFilePath: URI
    // One of the following entry points is required in valid configs, but may be absent if only container metadata is used:
    image?: string
    dockerFile?: string | { file: string, context?: string, target?: string, buildArgs?: map<string,string> }
    dockerComposeFile?: string | string[]
    service?: string                                     // compose service name when using compose
    // Common properties (subset)
    name?: string
    remoteUser?: string | number
    containerUser?: string | number                      // older property; normalized by updateFromOldProperties
    remoteEnv?: map<string, string>
    containerEnv?: map<string, string>                   // older property; normalized to remoteEnv in output
    features?: map<string, string | boolean | map<string, string | boolean>>
    mounts?: (string | Mount)[]
    runArgs?: string[]
    postCreateCommand?: string | string[] | { command?: string, commandWithArgs?: string[] }
    postStartCommand?: string | string[]
    postAttachCommand?: string | string[]
    onCreateCommand?: string | string[] | { command?: string, commandWithArgs?: string[] }
    updateContentCommand?: string | string[]
    initializeCommand?: string | string[]
    customizations?: map<string, any>
    hostRequirements?: HostRequirements
    workspaceFolder?: string
    workspaceMount?: string | Mount
END STRUCT
```

## WorkspaceConfig (subset)

```pseudocode
STRUCT WorkspaceConfig:
    workspaceFolder: string            // substituted; may be adjusted by config.workspaceFolder
    workspaceMount?: string | Mount
    configFolderPath: string           // host path for .devcontainer folder
    rootFolderPath: string             // host root workspace path
END STRUCT
```

## FeaturesConfig

```pseudocode
STRUCT FeaturesConfig:
    featureSets: FeatureSet[]
    dstFolder?: string                 // working destination (computed)
END STRUCT

STRUCT FeatureSet:
    features: Feature[]
    sourceInformation: SourceInformation
    internalVersion?: string
    computedDigest?: string
END STRUCT

STRUCT Feature:
    id: string
    options?: map<string, any>              // Supports all JSON types: string, boolean, number, array, object, null
    customizations?: map<string, any>
    init?: boolean
    privileged?: boolean
    mounts?: Mount[] | string[]
    remoteEnv?: map<string, string>
END STRUCT

UNION SourceInformation = GithubSourceInformation | DirectTarballSourceInformation | FilePathSourceInformation | OCISourceInformation
```

## MergedDevContainerConfig (shape)

```pseudocode
STRUCT MergedDevContainerConfig:
    configFilePath: URI
    // Entry point preserved from base config
    image?: string
    dockerFile?: string | { file: string, context?: string, target?: string, buildArgs?: map<string,string> }
    dockerComposeFile?: string | string[]
    service?: string
    // Resolved/merged properties (selected):
    remoteEnv?: map<string,string>              // last-wins per key (Features then base overrides)
    containerUser?: string | number
    remoteUser?: string | number
    runArgs?: string[]
    mounts?: (string | Mount)[]                 // deduplicated by target; normalized formats mixed
    hostRequirements?: HostRequirements
    customizations?: map<string, any>
    // Lifecycle commands merged according to spec/metadata
    onCreateCommand?: string | string[] | { command?: string, commandWithArgs?: string[] }
    updateContentCommand?: string | string[]
    postCreateCommand?: string | string[]
    postStartCommand?: string | string[]
    postAttachCommand?: string | string[]
END STRUCT
```

## Supporting Types (subset)

```pseudocode
STRUCT Mount:
    type: 'bind' | 'volume'
    source?: string
    target: string
    external?: boolean
END STRUCT

STRUCT HostRequirements:
    gpu?: 'optional' | boolean | { cores?: number, memory?: string | number }
END STRUCT
```

