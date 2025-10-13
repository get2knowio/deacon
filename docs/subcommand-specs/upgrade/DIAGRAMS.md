# Upgrade Subcommand — Diagrams

## Sequence — Standard Upgrade (persist lockfile)

```mermaid
sequenceDiagram
    participant U as User
    participant CLI as devcontainer upgrade
    participant CFG as ConfigResolver
    participant FR as FeaturesResolver
    participant REG as OCI Registry
    participant FS as Filesystem

    U->>CLI: upgrade --workspace-folder <path>
    CLI->>CFG: discover/load devcontainer.json
    CFG-->>CLI: DevContainerConfig

    CLI->>FR: readFeaturesConfig(config)
    FR->>REG: fetch tags/manifests/blobs as needed
    REG-->>FR: versions/manifests/digests
    FR-->>CLI: FeaturesConfig

    CLI->>CLI: generateLockfile(FeaturesConfig)
    CLI->>FS: write '' to lockfilePath (truncate)
    CLI->>FS: writeLockfile(lockfile, force_init=true)
    FS-->>CLI: ok
    CLI-->>U: exit 0
```

## Sequence — Dry Run (stdout only)

```mermaid
sequenceDiagram
    participant U as User
    participant CLI as devcontainer upgrade
    participant CFG as ConfigResolver
    participant FR as FeaturesResolver

    U->>CLI: upgrade --dry-run --workspace-folder <path>
    CLI->>CFG: load config
    CFG-->>CLI: DevContainerConfig

    CLI->>FR: readFeaturesConfig(config)
    FR-->>CLI: FeaturesConfig

    CLI->>CLI: generateLockfile(FeaturesConfig)
    CLI-->>U: print lockfile JSON to stdout
    CLI-->>U: exit 0
```

## Sequence — Pin Feature Then Upgrade

```mermaid
sequenceDiagram
    participant U as User
    participant CLI as devcontainer upgrade
    participant CFG as ConfigResolver
    participant FS as Filesystem
    participant FR as FeaturesResolver

    U->>CLI: upgrade --feature <id> --target-version 2 --workspace-folder <path>
    CLI->>CFG: load config
    CFG-->>CLI: DevContainerConfig

    CLI->>FS: updateFeatureVersionInConfig(configPath, id, "2")
    FS-->>CLI: devcontainer.json updated
    CLI->>CFG: re-load config
    CFG-->>CLI: DevContainerConfig (with :2)

    CLI->>FR: readFeaturesConfig(config)
    FR-->>CLI: FeaturesConfig
    CLI->>CLI: generateLockfile()
    alt dry-run
      CLI-->>U: print lockfile JSON
    else persist
      CLI->>FS: write '' to lockfilePath
      CLI->>FS: writeLockfile(lockfile)
    end
    CLI-->>U: exit 0
```

## Error Flow — Config Not Found

```mermaid
sequenceDiagram
    participant U as User
    participant CLI as devcontainer upgrade
    participant CFG as ConfigResolver

    U->>CLI: upgrade --workspace-folder <bad>
    CLI->>CFG: discover config
    CFG-->>CLI: not found
    CLI-->>U: stderr: "Dev container config (...) not found."
    CLI-->>U: exit 1
```

