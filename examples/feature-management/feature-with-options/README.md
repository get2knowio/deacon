# Feature With Options Example

Demonstrates a feature with multiple option types, environment, mounts, lifecycle.

## Commands
```sh
deacon features test . --json
OUT=$(mktemp -d)
deacon features package . --output "$OUT" --json
deacon features publish . --registry ghcr.io/example/with-options-feature --dry-run --json
```
