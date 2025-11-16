# Features Plan Data Structures

## Behavior Notes

### Input Validation
- `--additional-features` must be a JSON object (map); arrays, strings, numbers, booleans, and null values are rejected with a descriptive error.
- Feature IDs are validated to ensure they are registry references; local paths (starting with `./`, `../`, `/`, or `file://`) are rejected with guidance to use registry references instead.

### Determinism and Ordering
- The `order` array is deterministically computed using topological sort with lexicographic tie-breakers for independent features.
- The `graph` object contains only direct dependencies (union of `installsAfter` and `dependsOn`), deduplicated and sorted lexicographically by feature ID.
- All outputs are stable across multiple runs with the same inputs.

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
  "options": { 
    "stringOpt": "value",
    "boolOpt": true,
    "numberOpt": 300,
    "arrayOpt": ["item1", "item2"],
    "objectOpt": { "nested": "value" },
    "nullOpt": null
  },
  "metadata": { /* FeatureMetadata */ }
}
```

**Note:** As of the option normalization enhancement, `options` supports all JSON types (string, boolean, number, array, object, null). Previously only boolean and string types were supported; other types were silently dropped. All types are now preserved and passed through the pipeline without data loss.

## FeatureMetadata (subset)
```json
{
  "id": "string",
  "version": "string",
  "installsAfter": ["string"],
  "dependsOn": { "string": true }
}
```

