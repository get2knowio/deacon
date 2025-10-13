# Upgrade Subcommand — Data Structures

## ParsedInput

```pseudocode
STRUCT ParsedInput:
    workspace_folder: string        // absolute path
    config_file: URI | undefined    // absolute file URI when provided
    docker_path: string             // default 'docker'
    docker_compose_path: string     // default 'docker-compose'
    log_level: 'error'|'info'|'debug'|'trace'
    dry_run: boolean
    feature: string | undefined     // hidden flag
    target_version: string | undefined  // hidden flag, 'X' | 'X.Y' | 'X.Y.Z'
END STRUCT
```

## DevContainerConfig (subset used)

```pseudocode
STRUCT DevContainerConfig:
    configFilePath: URI
    features: map<string, string|boolean|object> | undefined
    // other fields are not material to the upgrade algorithm
END STRUCT
```

## Lockfile

```pseudocode
// Lockfile written adjacent to config
STRUCT Lockfile:
    features: map<string, LockfileFeature>
END STRUCT

STRUCT LockfileFeature:
    version: string     // resolved semantic version, e.g., '2.11.1'
    resolved: string    // concrete source, e.g., 'registry/path@sha256:<digest>' or tarball URI
    integrity: string   // computed digest (sha256)
END STRUCT
```

Notes:
- Feature keys are the user-declared identifiers (e.g., `ghcr.io/org/feat:2`).
- The features map is sorted by key during generation to ensure deterministic serialization.

## Feature Identifier Helpers

```pseudocode
FUNCTION get_feature_id_without_version(id: string) -> string:
    // Remove last tag or digest component from the identifier
    // Regex: /[:@][^/]*$/
    IF id MATCHES /[:@][^/]*$/ THEN
        RETURN id.substring(0, match.index)
    ELSE
        RETURN id
    END IF
END FUNCTION
```

## Lockfile Path Derivation

```pseudocode
FUNCTION get_lockfile_path(config_or_path: DevContainerConfig | string) -> string:
    config_path = (is string) ? config_or_path : config_or_path.configFilePath.fsPath
    dir = dirname(config_path)
    base = basename(config_path)
    name = base.starts_with('.') ? '.devcontainer-lock.json' : 'devcontainer-lock.json'
    RETURN join(dir, name)
END FUNCTION
```

## Logger Interface (stderr)

```pseudocode
STRUCT Logger:
    write(text: string, level: LogLevel)
END STRUCT

enum LogLevel { Error, Info, Debug, Trace }
```

## Execution Context

```pseudocode
STRUCT CLIHost:
    cwd: string
    env: map<string,string>
    platform: 'linux'|'darwin'|'win32'|'wsl'
    arch: 'x64'|'arm64'|...
    exec: (command, args, options) -> ExecResult
    path: PathOps   // join, dirname, etc.
END STRUCT
```

## Feature Resolution I/O (abstract)

```pseudocode
STRUCT FeaturesConfig:
    featureSets: FeatureSet[]
END STRUCT

STRUCT FeatureSet:
    features: Feature[]           // first item supplies version
    sourceInformation: SourceInfo // 'oci' | 'direct-tarball'
    computedDigest: string
END STRUCT

STRUCT Feature:
    id: string
    version: string
    dependsOn: map<string,string> | undefined
END STRUCT

STRUCT SourceInfo:
    type: 'oci' | 'direct-tarball'
    userFeatureId: string
    featureRef?: { registry: string, path: string }
    tarballUri?: string
END STRUCT
```

