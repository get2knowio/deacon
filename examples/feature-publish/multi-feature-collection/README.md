# multi-feature-collection

What this demonstrates
- A small collection containing two features.
- `deacon features package` produces a `devcontainer-collection.json` and feature packages.
- `deacon features publish` (dry-run) will compute tags for each feature and publish collection metadata as `:collection`.

Files
- `devcontainer-collection.json` — collection metadata referencing the two features
- `features/alpha/devcontainer-feature.json`
- `features/beta/devcontainer-feature.json`

Commands
```sh
OUT=$(mktemp -d)
# Package the collection
deacon features package . --output "$OUT" --progress json

# Dry-run publish the collection
deacon features publish . --namespace exampleorg/multi-collection --registry ghcr.io --dry-run --progress json
```

Notes
- The collection file `devcontainer-collection.json` is included to exercise collection publish code path.
