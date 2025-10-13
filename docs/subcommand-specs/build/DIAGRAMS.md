# Build Subcommand Diagrams

## Sequence – Dockerfile/Image Flow
```mermaid
sequenceDiagram
    autonumber
    participant U as User
    participant CLI as deacon build
    participant FS as Filesystem
    participant DC as Docker/BuildKit

    U->>CLI: devcontainer build --workspace-folder ...
    CLI->>CLI: Parse/validate args
    CLI->>FS: read devcontainer.json
    CLI->>CLI: resolve features + metadata
    alt BuildKit enabled
        CLI->>DC: buildx build [--platform|--push|--output|--cache-*]
        DC-->>CLI: built image loaded/pushed
    else Legacy build
        CLI->>DC: docker build [--no-cache] -t <name>
        DC-->>CLI: built image
    end
    CLI->>DC: docker tag (if multiple --image-name)
    CLI-->>U: JSON { outcome: success, imageName: ... }
```

## Sequence – Compose Flow
```mermaid
sequenceDiagram
    autonumber
    participant U as User
    participant CLI as deacon build
    participant FS as Filesystem
    participant DCC as Docker Compose

    U->>CLI: devcontainer build --workspace-folder ...
    CLI->>FS: read devcontainer.json + compose files
    CLI->>CLI: compute override for features/labels
    CLI->>DCC: docker compose config (read-only)
    CLI->>CLI: determine original image name for service
    alt --image-name provided
        CLI->>DCC: tag original image to provided names
    else
        CLI-->>U: imageName = default compose image name
    end
    CLI-->>U: JSON { outcome: success, imageName: ... }
```

## Error Flow – Mutually Exclusive push/output
```mermaid
sequenceDiagram
    autonumber
    participant CLI as deacon build

    CLI->>CLI: validate args
    CLI-->>CLI: detects --push and --output
    CLI-->>CLI: outcome=error ("--push true cannot be used with --output.")
```

## Error Flow – Compose unsupported flags
```mermaid
sequenceDiagram
    autonumber
    participant CLI as deacon build
    participant DCC as Docker Compose

    CLI->>DCC: detect compose config
    CLI-->>CLI: --platform/--push/--output/--cache-to present
    CLI-->>CLI: outcome=error ("... not supported.")
```

## Data Flow Overview
```mermaid
flowchart TD
    A[Args + Env] --> B[Parse & Normalize]
    B --> C[Resolve Config]
    C -->|Dockerfile| D[Build + Extend]
    C -->|Image| D
    C -->|Compose| E[Compose Override + Tag]
    D --> F[Return JSON]
    E --> F
```

