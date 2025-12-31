# Force Clean Example

This example demonstrates the `--force-clean-output-folder` (`-f`) behavior.

Files:

- `devcontainer-feature.json` - The feature to package.
- `output/OLD_ARTIFACT.tgz` - A pre-existing file to demonstrate that `-f` removes previous output.

Usage:

```bash
# Without force-clean: existing files in output/ may remain
deacon features package examples/feature-package/force-clean --output-folder examples/feature-package/force-clean/output

# With force-clean: the `output/` folder will be cleaned before packaging
deacon features package examples/feature-package/force-clean -f --output-folder examples/feature-package/force-clean/output

# Expected behavior with -f:
# - `OLD_ARTIFACT.tgz` is removed by the CLI and only the new artifacts (and devcontainer-collection.json) remain
```
