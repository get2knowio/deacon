# Features Info Data Structures

## Info JSON Output
```json
{
  "manifest": { /* OCIManifest object */ },
  "canonicalId": "registry/namespace/name@sha256:...",
  "publishedTags": ["1", "1.2", "1.2.3", "latest"]
}
```

## Feature Reference (conceptual)
```json
{
  "resource": "registry/namespace/name",
  "id": "registry/namespace/name:tag"
}
```

## Dependency Graph (text only)
- Mermaid sequence/flow text produced from computed worklist.

