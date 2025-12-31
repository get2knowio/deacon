# Invalid Feature Example

This example intentionally contains an invalid `devcontainer-feature.json` to demonstrate error handling during packaging.

Files:

- `devcontainer-feature.json` - Malformed JSON.

Usage:

```bash
# Attempting to package should fail with an error about invalid feature metadata or JSON parsing
deacon features package examples/feature-package/invalid-feature --output-folder examples/feature-package/invalid-feature/output
```
