# Quickstart: Devcontainer Up Gap Closure

1) **Prep environment**
   - Ensure Docker/Compose available on PATH (or set `--docker-path`/`--docker-compose-path`).
   - Set `RUST_LOG=info` (or `debug/trace`) for stderr logs; stdout must remain JSON-only.

2) **Build and fast checks**
   - `make dev-fast` (fmt-check, clippy, unit/bins/examples, doctests; skips slow integration/smoke tests).
   - **Fast loop reference**: During active development, use `make dev-fast` for rapid iteration (completes in seconds).
   - **Alternative fast tests**: `make test-fast` (unit+bins+examples+doctests only, no fmt/clippy).
   - **Parallel fast tests**: `make test-nextest-fast` (uses cargo-nextest with dev-fast profile for parallel execution).

3) **Run representative scenarios**
   - Single container: `cargo run -- up --workspace-folder /repo --include-configuration --remote-env FOO=bar`.
   - Prebuild: `cargo run -- up --workspace-folder /repo --prebuild`.
   - Compose: `cargo run -- up --workspace-folder /repo --config .devcontainer/compose/devcontainer.json --mount type=bind,source=/cache,target=/cache --id-label project=demo`.

4) **Inspect outputs**
   - Success: stdout emits one JSON object with containerId and remoteWorkspaceFolder; logs on stderr.
   - Failure: stdout emits error JSON; exit code 1; stderr contains diagnostics without secrets.

5) **Full gate before PR**
   - **One command**: `make release-check` (runs fmt, clippy, full test suite, and release build).
   - **Manual steps** (if preferred):
     - `cargo build --verbose`
     - `cargo test -- --test-threads=1`
     - `cargo test --doc`
     - `cargo fmt --all && cargo fmt --all -- --check`
     - `cargo clippy --all-targets -- -D warnings`
   - **Parallel full tests**: `make test-nextest` (uses cargo-nextest with full profile; faster than serial `cargo test`).
