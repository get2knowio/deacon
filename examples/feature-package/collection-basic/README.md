# Collection Basic Example

This example demonstrates packaging a feature collection (a `src/` folder containing multiple features).

Files:

- `src/feature-a/devcontainer-feature.json`
- `src/feature-b/devcontainer-feature.json`

Usage:

```bash
# Package the collection located at examples/feature-package/collection-basic
deacon features package examples/feature-package/collection-basic --output-folder examples/feature-package/collection-basic/output

# Expected output:
# - Two .tgz artifacts in the `output/` folder (one per feature)
# - A `devcontainer-collection.json` file listing both features
```
