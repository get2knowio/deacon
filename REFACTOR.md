# Refactor To-Do List

- [x] `crates/deacon/src/commands/features.rs`: remove panics when discovering workspace—replace `current_dir().unwrap()` with fallible path resolution that surfaces user-facing context.
- [x] `crates/deacon/src/commands/features.rs`: avoid `serde_json::to_value(...).unwrap_or(...)` when building descriptors; propagate/annotate serialization errors instead of panicking.
- [x] `crates/deacon/src/commands/features.rs`: split the monolith into focused modules (e.g., `plan.rs`, `package.rs`, `publish.rs`, `test.rs`, shared helpers). Limit public API to a few entry points and reuse shared config/merge helpers.
- [x] `crates/deacon/src/commands/up.rs`: stop swallowing `canonicalize` failures (`unwrap_or(ws)`); return structured errors with path/context so bad inputs don't silently proceed.
- [x] `crates/deacon/src/commands/build/mod.rs`: drop `take().unwrap()` on child pipes and handle line-read errors explicitly; make scan output collection fully fallible and context-rich.
- [x] `crates/core/src/oci.rs`: refactor install-script execution to `tokio::process::Command` with streamed stdout/stderr and await status; avoid blocking the async runtime and unbounded buffering.
- [x] `crates/core/src/oci.rs`: decompose into submodules (auth, client, semver utils, install execution, cache). Keep sync paths isolated and reuse retry/logging helpers.
- [x] Cross-cutting: add small tests around the new fallible paths (canonicalization failure, serde failure in packaging, install script non-zero exit) and update `.config/nextest.toml` grouping if new bins are added.
