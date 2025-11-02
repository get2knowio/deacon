# single-feature-basic

What this demonstrates
- A simple feature with a valid semantic version `1.2.3`.
- Packaging then dry-run publishing which computes semantic tags: `1`, `1.2`, `1.2.3`, and `latest`.

Files
- `devcontainer-feature.json` — feature metadata

Commands
```sh
# From this directory
# Package the feature (output to temporary dir)
OUT=$(mktemp -d)
deacon features package . --output "$OUT" --progress json

# Dry-run publish (no network push) and show JSON output
deacon features publish . --namespace exampleorg/example-features --registry ghcr.io --dry-run --progress json
```

Notes
- In dry-run mode the command prints the JSON summary to stdout and logs to stderr.
- Replace `--registry` and `--namespace` with your registry/namespace as needed for integration testing.
