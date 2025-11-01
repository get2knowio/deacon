# Quickstart — Close Spec Gap (Features Plan)

This feature closes gaps in the “features plan” behavior: strict input validation, deterministic order, and precise graph semantics.

## Prerequisites
- Rust stable toolchain (see `rust-toolchain.toml`)
- This repo checked out: /workspaces/deacon

## Fast dev loop

```bash
# Format + clippy + unit/examples + doctests
make dev-fast
```

## What will be implemented
- Validate `--additional-features` is a JSON object (fail fast otherwise)
- Reject local feature paths (use registry references only)
- Build deterministic `{ order, graph }` where:
  - `order` is topological with lexicographic tie-breakers
  - `graph` lists direct deps: union(installsAfter, dependsOn), deduped and sorted
- Shallow merge of option maps with CLI precedence
- Fail fast on registry metadata fetch errors (401/403/404/network)
- JSON output contract: plan JSON to stdout; logs to stderr

## Try it (after implementation)

```bash
# Show CLI help
cargo run -p deacon -- features plan --help

# Basic usage
cargo run -p deacon -- features plan --config examples/features/lockfile-demo/devcontainer.json \
  --additional-features '{"ghcr.io/devcontainers/features/git:1": {"version": "latest"}}'
```

Notes
- Tests should cover invalid JSON, local paths, cycle detection, tie-break ordering, and merge behavior.
- Keep build green: fmt, clippy, tests.
