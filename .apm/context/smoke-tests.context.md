# context: smoke-tests

CLI smoke tests and quick checks:

- Location: `crates/deacon/tests/` (e.g., `integration_cli.rs`)
- Philosophy: deterministic and hermetic; no network
- Useful invocations:
  - `cargo test -p deacon -- --test-threads=1`
  - Run a single test: `cargo test -p deacon <name_substring>`
- Validate doctests: `cargo test --doc`

When updating behavior, add or adjust smoke tests to reflect the expected CLI UX and error messages.

