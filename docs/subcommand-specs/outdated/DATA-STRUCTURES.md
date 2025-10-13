# Outdated Subcommand Data Structures

## CLI/Parsed Input

```pseudocode
STRUCT ParsedInput:
    workspace_folder: string                // required
    config_file: URI?                       // must be devcontainer.json or .devcontainer.json
    output_format: 'text' | 'json'          // default 'text'
    log_level: 'info' | 'debug' | 'trace'   // default 'info'
    log_format: 'text' | 'json'             // default 'text'
    terminal_columns: number?               // implies terminal_rows
    terminal_rows: number?                  // implies terminal_columns
END STRUCT
```

## Resolved Configuration Snapshot (subset used)

```pseudocode
STRUCT DevContainerConfig:
    configFilePath: URI                     // absolute
    features: map<string, FeatureOptions>
END STRUCT

TYPE FeatureOptions = boolean | string | map<string, boolean | string>
```

## Lockfile (subset used)

```pseudocode
STRUCT Lockfile:
    features: map<string, LockFeature>
END STRUCT

STRUCT LockFeature:
    version: string               // semver
    resolved: string              // canonical ref 'registry/path@sha256:...'
    integrity: string             // digest 'sha256:...'
    dependsOn?: string[]
END STRUCT
```

## Outdated Result (JSON Output Schema)

```pseudocode
STRUCT OutdatedResult:
    features: map<string, FeatureVersionInfo>
END STRUCT

STRUCT FeatureVersionInfo:
    current?: string      // lockfile version if present, else wanted
    wanted?: string       // derived from tag/digest per rules
    wantedMajor?: string  // major(wanted)
    latest?: string       // highest published semver tag
    latestMajor?: string  // major(latest)
END STRUCT
```

Notes:
- The `features` map key is the user-declared feature identifier exactly as in `devcontainer.json` (including `:tag` or `@sha256:` when present).
- Features that cannot be versioned (e.g., local paths `./feature`, direct tarballs `https://...`, legacy identifiers without registry) are omitted from the result entirely.
- Fields may be `undefined` when information is unavailable due to registry/network issues or missing tags.

## Internal Working Types (derivation helpers)

```pseudocode
STRUCT OCIRef:
    registry: string
    namespace: string
    id: string
    path: string          // namespace/id
    resource: string      // registry/path
    tag?: string          // tag if present, implied 'latest' when none
    digest?: string       // sha256:... when specified
    version: string       // digest or tag
END STRUCT
```

