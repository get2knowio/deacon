# Up Subcommand Diagrams

## Sequence – Dockerfile/Image Flow
```mermaid
sequenceDiagram
    autonumber
    participant U as User
    participant CLI as deacon up
    participant DC as Docker/BuildKit
    participant FS as Filesystem

    U->>CLI: devcontainer up --workspace-folder ...
    CLI->>CLI: Parse/validate args
    CLI->>CLI: createDockerParams()
    CLI->>FS: read devcontainer.json / override
    CLI->>CLI: substitute variables
    CLI->>DC: list/inspect by id labels
    alt Container exists
        CLI->>DC: start existing container (if not running)
        DC-->>CLI: running container
    else No container
        CLI->>DC: build/extend image (Features, labels)
        DC-->>CLI: built image
        CLI->>DC: docker run (labels, mounts, env)
        DC-->>CLI: new container id
    end
    CLI->>DC: inspect container (metadata)
    CLI->>CLI: merge configuration with metadata
    CLI->>DC: exec lifecycle hooks (onCreate, updateContent, postCreate, ...)
    CLI-->>U: JSON result (containerId, remoteUser, remoteWorkspaceFolder)
```

## Sequence – Docker Compose Flow
```mermaid
sequenceDiagram
    autonumber
    participant U as User
    participant CLI as deacon up
    participant DCC as Docker Compose
    participant FS as Filesystem

    U->>CLI: devcontainer up --workspace-folder ...
    CLI->>CLI: Parse/validate args
    CLI->>FS: read devcontainer.json / override
    CLI->>CLI: resolve compose files + project name
    CLI->>DCC: docker compose config (profiles, env-file)
    CLI->>DCC: docker compose up -d
    DCC-->>CLI: service container
    CLI->>DCC: find service container id (labels)
    CLI->>CLI: merge configuration with container image metadata
    CLI->>DCC: exec lifecycle hooks in container
    CLI-->>U: JSON result (containerId, composeProjectName, remoteUser)
```

## Error Flow – Lifecycle Command Failure
```mermaid
sequenceDiagram
    autonumber
    participant CLI as deacon up
    participant C as Container

    CLI->>C: exec lifecycle command(s)
    C-->>CLI: non-zero exit / signal
    CLI->>CLI: mark lifecycle failed; stop further user commands
    CLI-->>CLI: outcome=error (message, description)
```

## Data Flow Overview
```mermaid
flowchart TD
    A[Args + Env] --> B[Parse & Normalize]
    B --> C[Docker Params]
    C --> D[Resolve Config]
    D -->|No Compose| E[Dockerfile/Image Flow]
    D -->|Compose| F[Compose Flow]
    E --> G[Inspect + Metadata]
    F --> G
    G --> H[Merge Config]
    H --> I[setupInContainer]
    I --> J[Up Result JSON]
```

