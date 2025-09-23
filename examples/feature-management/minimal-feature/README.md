# Minimal Feature Example

A minimal feature manifest with only an `id`.

## Commands
```sh
deacon features test . --json
OUT=$(mktemp -d)
deacon features package . --output "$OUT" --json
```
