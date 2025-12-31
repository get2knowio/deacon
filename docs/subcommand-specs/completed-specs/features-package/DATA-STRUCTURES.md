# Features Package Data Structures

## Feature Metadata (excerpt)
Derived from `devcontainer-feature.json`.

```json
{
  "id": "string",
  "version": "string",
  "name": "string",
  "description": "string",
  "options": { "k": {"type": "string|boolean", "default": "..."} },
  "installsAfter": ["otherFeatureId"],
  "dependsOn": { "featureId": true }
}
```

## devcontainer-collection.json (Features)
```json
{
  "sourceInformation": { "source": "devcontainer-cli" },
  "features": [
    {
      "id": "<id>",
      "version": "<version>",
      "name": "<name>",
      "description": "<desc>",
      "options": { },
      "installsAfter": [],
      "dependsOn": {}
    }
  ]
}
```

## Packaging Summary (conceptual)
```json
{
  "mode": "single|collection",
  "outputDir": "./output",
  "artifacts": ["<id>.tgz", "devcontainer-collection.json"]
}
```

