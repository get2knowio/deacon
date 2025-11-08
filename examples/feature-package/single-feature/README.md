# Single Feature Example

This example demonstrates packaging a single feature directory using the `devcontainer features package` command.

Files:

- `devcontainer-feature.json` - The single feature manifest used as input.

Usage:

```bash
# From this repository root, package the single feature (target is the feature directory):
deacon features package examples/feature-package/single-feature --output-folder examples/feature-package/single-feature/output

# Expected output:
# - A .tgz artifact in the `output/` folder
# - A `devcontainer-collection.json` file in the `output/` folder describing the packaged feature
```
