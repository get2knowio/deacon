# Features Plan Data Structures

## Plan Output (JSON)
```json
{
  "order": ["ghcr.io/owner/collection/featA:1", "ghcr.io/owner/collection/featB:1"],
  "graph": {
    "ghcr.io/owner/collection/featA:1": [],
    "ghcr.io/owner/collection/featB:1": ["ghcr.io/owner/collection/featA:1"]
  }
}
```

## ResolvedFeature (conceptual)
```json
{
  "id": "<canonical id>",
  "source": "<registry ref>",
  "options": { "opt": "value" },
  "metadata": { /* FeatureMetadata */ }
}
```

## FeatureMetadata (subset)
```json
{
  "id": "string",
  "version": "string",
  "installsAfter": ["string"],
  "dependsOn": { "string": true }
}
```

