# Build Subcommand Data Structures

## CLI/Parsed Input

```pseudocode
STRUCT ParsedInput:
    workspace_folder: string
    config_file: URI?           // must be devcontainer.json or .devcontainer.json
    log_level: 'info' | 'debug' | 'trace'
    log_format: 'text' | 'json'
    no_cache: boolean
    image_names: string[]       // repeatable --image-name
    cache_from: string[]        // repeatable --cache-from
    cache_to: string?           // --cache-to (buildx only)
    buildkit_mode: 'auto' | 'never'
    platform: string?           // os/arch[/variant] (buildx only)
    push: boolean               // buildx only
    output: string?             // buildx only, mutually exclusive with push
    labels: string[]            // repeatable --label
    additional_features: map<string, string | boolean | map<string, string | boolean>>
    skip_feature_auto_mapping: boolean
    skip_persist_customizations: boolean
    experimental_lockfile: boolean
    experimental_frozen_lockfile: boolean
    omit_syntax_directive: boolean
END STRUCT
```

## Resolver Parameters (subset used by build)

```pseudocode
STRUCT DockerResolverParameters:
    common: {
        cliHost: CLIHost
        env: map<string,string>
        output: Log
        persistedFolder: string
        logLevel: 'info' | 'debug' | 'trace'
        logFormat: 'text' | 'json'
        skipPersistingCustomizationsFromFeatures: boolean
        omitSyntaxDirective: boolean
    }
    dockerCLI: () -> DockerCLI
    dockerComposeCLI: () -> DockerComposeCLI
    platformInfo: { os: string, arch: string, variant?: string }
    buildNoCache: boolean
    useBuildKit: 'auto' | 'never'
    buildKitVersion?: { versionMatch: string }  // resolved from environment when available
    buildxPlatform?: string
    buildxPush?: boolean
    buildxOutput?: string
    buildxCacheTo?: string
    additionalCacheFroms: string[]
    additionalLabels: string[]
    skipFeatureAutoMapping: boolean
    experimentalLockfile?: boolean
    experimentalFrozenLockfile?: boolean
END STRUCT
```

## Build Results

```pseudocode
STRUCT BuildSuccessResult:
    outcome: 'success'
    imageName: string | string[]
END STRUCT

STRUCT BuildErrorResult:
    outcome: 'error'
    message: string
    description?: string
END STRUCT

TYPE ExecutionResult = BuildSuccessResult | BuildErrorResult
```

## Feature Build Options

```pseudocode
STRUCT ImageBuildOptions:
    dstFolder: string                  // working dir for generated files
    dockerfileContent: string          // appended dockerfile content for features
    overrideTarget: string             // build target to use
    dockerfilePrefixContent: string    // optional # syntax line and ARG base
    buildArgs: map<string,string>      // base image and feature args
    buildKitContexts: map<string,string> // name->path for build contexts
    securityOpts: string[]
END STRUCT
```

## Compose Override Result (subset)

```pseudocode
STRUCT ComposeExtendResult:
    overrideImageName?: string       // if original service image was overridden
    labels?: map<string,string>
END STRUCT
```

