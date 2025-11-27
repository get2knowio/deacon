# Quickstart: Workspace Mount Consistency and Git-Root Handling

1) Read the spec `specs/006-align-workspace-mounts/spec.md`, research `research.md`, and data model `data-model.md` to confirm workspace discovery, consistency propagation, and git-root handling expectations.
2) Update workspace discovery and mount rendering in `crates/core` / `crates/deacon` to:
   - Apply the provided consistency to every default workspace mount (Docker + Compose).
   - Use git root when the git-root flag is set; fall back to workspace root with an explicit note otherwise.
   - Keep Docker and Compose mount generation in parity for host path and consistency.
3) Add/adjust tests:
   - Unit tests for path selection (workspace vs git root) and consistency propagation.
   - Integration/CLI-render tests if Docker vs Compose formatting paths differ.
   - Configure any new integration binaries in `.config/nextest.toml` with correct test groups.
4) Validation cadence:
   - `cargo fmt --all && cargo fmt --all -- --check`
   - `cargo clippy --all-targets -- -D warnings`
   - `make test-nextest-unit` (logic) and `make test-nextest-fast` (broader)
   - `make test-nextest` before PR
5) Ensure fallback messaging is surfaced without silent divergence and that stdout/stderr contracts remain intact.
