# Non-ASCII Paths Example

This example places a feature in a path containing non-ASCII characters (`ümlaut-feature`) to exercise Unicode path handling.

Files:

- `src/ümlaut-feature/devcontainer-feature.json`

Usage:

```bash
# Package the collection containing the non-ASCII feature
deacon features package examples/feature-package/non-ascii --output-folder examples/feature-package/non-ascii/output

# Expected output:
# - A .tgz artifact for `ümlaut-feature` and a `devcontainer-collection.json` in the `output/` folder
```
