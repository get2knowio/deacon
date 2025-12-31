# Quickstart — features package

Package a single feature or an entire collection into `.tgz` archives and emit collection metadata.

## Prerequisites
- Rust toolchain installed in dev container (provided)
- This repository checked out; run from repo root

## Single Feature
```
deacon features package ./examples/features/minimal-feature -o ./output
```
Expected:
- One `.tgz` produced in `./output`
- `devcontainer-collection.json` describing the packaged feature

## Collection (src/*)
```
deacon features package ./examples/features/parallel-install-demo -o ./output
```
Expected:
- One `.tgz` per valid `src/<featureId>`
- `devcontainer-collection.json` enumerating all packaged features

## Force Clean Output Folder
```
deacon features package . -o ./output --force-clean-output-folder
```
Expected:
- `./output` is emptied before new artifacts are written

## Notes
- Omit the positional `target` to default to `.`
- This subcommand is text-only; no structured JSON output is produced
- Use global log level if needed: `deacon --log-level debug features package ...`
