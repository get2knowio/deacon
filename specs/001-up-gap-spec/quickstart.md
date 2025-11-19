# Quickstart: Devcontainer Up Gap Closure

1) **Prep environment**
   - Ensure Docker/Compose available on PATH (or set `--docker-path`/`--docker-compose-path`).
   - Set `RUST_LOG=info` (or `debug/trace`) for stderr logs; stdout must remain JSON-only.

2) **Build and fast checks**
   - `make dev-fast` (fmt-check, clippy, unit/bins/examples, doctests).

3) **Run representative scenarios**
   - Single container: `cargo run -- up --workspace-folder /repo --include-configuration --remote-env FOO=bar`.
   - Prebuild: `cargo run -- up --workspace-folder /repo --prebuild`.
   - Compose: `cargo run -- up --workspace-folder /repo --config .devcontainer/compose/devcontainer.json --mount type=bind,source=/cache,target=/cache --id-label project=demo`.

4) **Inspect outputs**
   - Success: stdout emits one JSON object with containerId and remoteWorkspaceFolder; logs on stderr.
   - Failure: stdout emits error JSON; exit code 1; stderr contains diagnostics without secrets.

5) **Full gate before PR**
   - `cargo build --verbose`
   - `cargo test -- --test-threads=1`
   - `cargo test --doc`
   - `cargo fmt --all && cargo fmt --all -- --check`
   - `cargo clippy --all-targets -- -D warnings`
