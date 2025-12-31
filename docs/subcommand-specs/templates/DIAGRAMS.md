# Templates Subcommand — Diagrams

## Sequence — templates apply

```mermaid
sequenceDiagram
    autonumber
    participant U as User
    participant CLI as templates apply
    participant OCI as OCI Registry
    participant FS as Workspace FS

    U->>CLI: devcontainer templates apply -w <path> -t <ref> -a <json> -f <json>
    CLI->>CLI: Parse & validate args (jsonc)
    CLI->>OCI: GET /v2/<ns>/<name>/manifests/<tag|digest>
    OCI-->>CLI: 200 Manifest (or 404)
    CLI->>OCI: GET /v2/<ns>/<name>/blobs/<digest>
    OCI-->>CLI: 200 Tar layer
    CLI->>FS: Extract to workspace (omit reserved paths)
    CLI->>FS: Substitute ${templateOption:KEY} in files
    alt features requested
        CLI->>FS: Read devcontainer.json
        CLI->>FS: Add features[id] = options
    end
    CLI-->>U: stdout { files: [...] }
```

## Sequence — templates publish

```mermaid
sequenceDiagram
    autonumber
    participant A as Author
    participant CLI as templates publish
    participant PKG as Packager
    participant OCI as OCI Registry

    A->>CLI: devcontainer templates publish -r ghcr.io -n owner/repo ./src
    CLI->>PKG: Package templates (tgz per template, collection metadata)
    loop each template
        CLI->>OCI: GET tags/list
        OCI-->>CLI: 200 { tags }
        CLI->>CLI: Compute semantic tags (major, minor, full, latest)
        CLI->>OCI: Push layer and manifest with annotations
        OCI-->>CLI: 201 Created (digest)
    end
    CLI->>OCI: Push collection metadata (latest)
    OCI-->>CLI: 201 Created (digest)
    CLI-->>A: stdout { <id>: { publishedTags, digest, version }, ... }
```

## Sequence — templates metadata

```mermaid
sequenceDiagram
    autonumber
    participant U as User
    participant CLI as templates metadata
    participant OCI as OCI Registry

    U->>CLI: devcontainer templates metadata <templateId>
    CLI->>OCI: GET manifest (tag or digest)
    OCI-->>CLI: 200 Manifest with annotations
    alt has dev.containers.metadata
        CLI-->>U: stdout parsed metadata JSON
    else missing
        CLI-->>U: stdout {}
        CLI->>U: exit non-zero
    end
```

## ASCII — Data Flow (apply)

```
┌───────────────┐
│ User Input    │
└───────┬───────┘
        │ args
        ▼
┌─────────────────────┐
│ Parse & Validate    │
└───────┬─────────────┘
        │ oci-ref
        ▼
┌─────────────────────┐      manifest      ┌───────────────┐
│ Fetch Manifest      ├───────────────────▶│ Resolve Layer │
└───────┬─────────────┘                    └───────┬───────┘
        │ blob url                                  │ digest
        ▼                                           ▼
┌─────────────────────┐  tar  ┌────────────────────────────┐
│ Download & Extract  ├──────▶│ Write Files to Workspace   │
└───────┬─────────────┘       └───────────┬────────────────┘
        │                                   │
        ▼                                   ▼
┌─────────────────────┐            ┌────────────────────────┐
│ Option Substitution │            │ Feature Injection      │
└───────┬─────────────┘            └───────────┬────────────┘
        │                                   │
        ▼                                   ▼
               ┌───────────────────────────────────────────┐
               │ stdout { files: [...] } + logs to stderr  │
               └───────────────────────────────────────────┘
```

