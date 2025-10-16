# context: logging-and-errors

Overview of logging and error handling in this codebase:

- Logging: Uses `tracing` across CLI and core crates
  - Common spans/fields in `crates/core/src/observability.rs`
  - CLI sets log level via `DEACON_LOG`/`RUST_LOG` in `crates/deacon/src/cli.rs`
  - Commands instrumented with `#[instrument]` and `tracing::{debug,info,warn,error}`
- Error handling:
  - Prefer `thiserror` for domain errors in core
  - Use `anyhow` with context at binary boundaries (CLI)
  - Redact secrets in logs; avoid logging sensitive values

Useful files to inspect:
- crates/core/src/observability.rs
- crates/deacon/src/cli.rs
- crates/deacon/src/commands/**

