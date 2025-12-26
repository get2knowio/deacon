# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What is Deacon?

Deacon is a Rust implementation of the Development Containers CLI, following the [containers.dev specification](https://containers.dev). It provides DevContainer lifecycle management including configuration resolution, feature installation, template scaffolding, and container orchestration.

## Core Architecture

**Workspace Structure:**
- `crates/deacon/` - CLI binary crate (argument parsing, command orchestration, UI)
- `crates/core/` - Core library with domain logic (config parsing, container runtime, OCI registry, features/templates)
- `docs/subcommand-specs/*/SPEC.md` - Authoritative CLI specifications (source of truth for all behavior)
- `.specify/memory/constitution.md` - Development constitution defining principles and constraints
- `examples/` - Executable examples with `exec.sh` scripts demonstrating features

**Key Abstractions:**
- `ContainerRuntime` trait - Docker/Podman abstraction for container operations
- `HttpClient` trait - OCI registry communication (reqwest-based with HEAD/GET/POST/PUT)
- `ConfigLoader` - DevContainer configuration resolution with extends chains
- `FeatureInstaller` - OCI feature installation and dependency resolution
- `ContainerLifecycle` - Lifecycle command execution (onCreate, postCreate, postStart, etc.)
- Container environment probe with caching - 50%+ latency improvement via `probe_container_environment()`

## Critical Development Principles

**1. Spec-Parity as Source of Truth**
- ALL behavior MUST align with `docs/subcommand-specs/*/SPEC.md`
- Data structures MUST match spec shapes exactly (map vs vec, field ordering, null handling)
- Configuration resolution MUST use full extends chains via `ConfigLoader::load_with_extends`
- Never implement shortcuts that deviate from spec-defined algorithms

**2. Keep the Build Green (Non-Negotiable)**
Run after EVERY code change:
```bash
cargo fmt --all && cargo fmt --all -- --check  # Format immediately
cargo clippy --all-targets -- -D warnings      # Zero tolerance
```

Development loop options:
- Fast loop (default): `make test-nextest-fast` - unit/bins/examples + doctests, excludes docker/smoke
- Targeted: `make test-nextest-unit` (unit only), `make test-nextest-docker` (docker integration)
- Full gate (before PR): `make test-nextest` - complete parallel test suite

**Fix All Failures - Even Unrelated Ones:**
If you encounter build or test failures during CI or local testing, fix them even if they're unrelated to your current work. A broken build blocks everyone. Never defer failures to "fix later" - address them before completing your current task. This includes:
- Pre-existing test failures discovered during your work
- Flaky tests that fail intermittently
- Lint or format issues in files you didn't modify
- Documentation or doctest compilation errors

**3. No Silent Fallbacks - Fail Fast**
- Production code MUST emit clear errors when capabilities are unavailable
- Mocks/fakes are ONLY for tests, never in runtime code paths
- Filter invalid inputs at ingress per spec (e.g., only OCI refs, only semver tags)
- Never swallow errors with unwraps or sentinel values - always propagate with `Result` and `.context()`

**4. Panic-Free, Async-Safe Implementations**
- Runtime code MUST NOT panic on expected failures: replace `unwrap`/unchecked `expect` with fallible paths and
  contextual errors.
- Async code MUST avoid blocking calls (`std::process::Command::output`, blocking file IO). Use `tokio` async
  equivalents with streamed output or offload to bounded blocking tasks.
- Prefer modular boundaries over monoliths: split large commands/clients into focused modules (e.g., `features`
  `{plan,package,publish,test}`, `oci` `{auth,client,semver,install}`, `up` `{args,config,compose,runtime}`) and
  reuse shared helpers.

**4. Subcommand Consistency & Shared Abstractions**
When multiple subcommands share behavior (terminal sizing, config resolution, container targeting, env probing), use shared helpers:
- `resolve_env_and_user()` - Container environment probing with cache support
- `ConfigLoader::load_with_extends()` - Full configuration resolution
- Terminal/remote-env helpers in `commands/shared/`
- See `docs/ARCHITECTURE.md` for cross-cutting patterns (env probe caching, etc.)

## Common Development Tasks

**Build & Run:**
```bash
cargo build --release              # Production build
cargo run -- --help                # Run CLI
cargo run -- up                    # Start a devcontainer
cargo run -- read-configuration    # Parse devcontainer.json
```

**Testing Strategy:**
```bash
# Fast feedback loop (default during development)
make test-nextest-fast            # Unit/bins/examples + doctests (excludes docker/smoke)

# Targeted testing by area
make test-nextest-unit            # Super fast unit tests only
make test-nextest-docker          # Docker integration tests
make test-nextest-smoke           # High-level smoke tests

# Full validation (before PR)
make test-nextest                 # Complete parallel suite with all tests
```

**Test Groups** (configured in `.config/nextest.toml`):
- `docker-exclusive` (serial) - Exclusive Docker daemon access required
- `docker-shared` (parallel-4) - Safe concurrent Docker usage
- `fs-heavy` (parallel-4) - Significant filesystem operations
- `long-running` (serial) - Heavy end-to-end tests
- `smoke` (serial) - High-level integration tests
- `parity` (serial) - Upstream CLI comparison

**When adding new integration tests:**
1. Identify resource requirements (docker exclusive vs shared, filesystem heavy, etc.)
2. Add override rules to ALL profiles in `.config/nextest.toml`
3. Prefer most permissive group that ensures correctness (docker-shared over docker-exclusive when safe)
4. Verify with `make test-nextest` to ensure no race conditions

**Code Quality:**
```bash
cargo fmt --all                   # Format code (run after EVERY change)
cargo clippy --all-targets -- -D warnings  # Lint with zero tolerance
make dev-fast                     # Fast loop: fmt + clippy + fast tests
make release-check                # Full quality gate
```

**Running Single Tests:**
```bash
# With cargo-nextest (faster, parallel)
cargo nextest run test_name
cargo nextest run 'test(integration_)*'

# Traditional cargo test (serial)
cargo test test_name -- --test-threads=1
```

## Code Patterns & Style

**Error Handling:**
- `thiserror` for domain errors in core
- `anyhow` only at binary boundaries with `.context()` for diagnostics
- Never use `unwrap()`/unchecked `expect` in runtime paths; propagate with `Result` and context
- Avoid blocking calls inside async functions; prefer `tokio` async IO or spawn bounded blocking tasks

**Logging:**
- Use `tracing` spans for workflows: `config.resolve`, `feature.install`, `lifecycle.run`
- Structured fields over string concatenation
- Respect `DEACON_LOG` / `RUST_LOG` environment variables
- JSON logging mode: `--log-format json` (all logs to stderr, results to stdout)

**Imports Organization** (rustfmt enforces):
1. Standard library (`use std::...`)
2. External crates (`use serde::...`)
3. Local modules (`use crate::...`, `use super::...`)

**Testing Requirements:**
- ALL spec-mandated tests MUST be implemented (output formats, exit codes, edge cases)
- Unit tests for pure logic, integration tests for runtime boundaries
- Deterministic and hermetic (no network) - use fixtures and mocks
- Doctests MUST compile with proper trait imports and Default implementations

**Examples Hygiene:**
- Every `examples/*/` directory MUST have `exec.sh` that runs all README scenarios
- Scripts MUST clean up all resources (containers, images, volumes)
- Pin images to specific versions (e.g., `alpine:3.18` not `latest`)
- Keep README and `exec.sh` in lockstep

## Code Search & Refactoring with ast-grep

Use ast-grep (command: `sg`) for searching and rewriting code instead of `find`, `grep`, or regex-based tools.
ast-grep operates on Abstract Syntax Trees (AST), enabling precise pattern matching that respects language syntax.

**When to use ast-grep:**
- Searching for specific code patterns (function calls, struct definitions, trait implementations)
- Refactoring code at scale (renaming, restructuring, migrating APIs)
- Finding usages that regex would miss or over-match
- Enforcing code conventions or detecting anti-patterns

**Basic usage:**
```bash
# Search for a pattern in Rust files
sg --pattern 'unwrap()' --lang rust

# Search for function definitions
sg --pattern 'fn $NAME($$$ARGS) -> $RET { $$$BODY }' --lang rust

# Rewrite code (dry-run by default)
sg --pattern 'println!($$$ARGS)' --rewrite 'tracing::info!($$$ARGS)' --lang rust

# Apply rewrites
sg --pattern 'old_fn($ARG)' --rewrite 'new_fn($ARG)' --lang rust --update-all
```

**Pattern syntax:**
- `$NAME` - Single metavariable (matches one AST node)
- `$$$ARGS` - Variadic metavariable (matches zero or more nodes)
- Patterns match AST structure, not text—whitespace and formatting are irrelevant

**Best practices:**
- Always specify `--lang rust` for Rust codebases
- Test patterns with search before applying rewrites
- Use `--interactive` for selective rewrites
- Prefer ast-grep over regex for any structural code transformation
- For complex refactors, write YAML rules in `sgconfig.yml`

## OCI Registry Implementation

**HTTP Client Trait Pattern:**
- Use HEAD requests to check blob existence (not GET - avoids downloading)
- Extract and use Location headers from POST /blobs/uploads/ responses per OCI spec
- When modifying `HttpClient` trait, update ALL implementations: `ReqwestClient`, `MockHttpClient`, `AuthMockHttpClient`, etc.

**Common Pitfalls to Avoid:**
- Don't use GET to check blob existence (wastes bandwidth)
- Don't ignore Location headers from upload initiation
- Don't forget to update test mocks when changing trait methods
- Do test with realistic mock responses (202 for upload start, 201/204 for completion)

## Container Environment Probe Caching

**Architecture Overview:**
All subcommands that execute commands in containers use the shared `resolve_env_and_user()` helper with optional caching:

```rust
// Pass --container-data-folder to enable caching
let env_user = resolve_env_and_user(
    &docker_client,
    &container_id,
    cli_user,
    config_remote_user,
    probe_mode,
    config_remote_env,
    &cli_env_map,
    args.container_data_folder.as_deref(),  // Cache folder
).await?;
```

**Cache behavior:**
- Location: `{cache_folder}/env_probe_{container_id}_{user}.json`
- Performance: 10-50x speedup on cache hit (90-98% latency reduction)
- Invalidation: Automatic on container ID change
- Error handling: Best-effort with graceful fallback

**Implemented in:** `up`, `exec`, `run-user-commands`
**Future subcommands:** Any command executing lifecycle hooks should implement this pattern

See `docs/ARCHITECTURE.md` for implementation checklist and code references.

## Pre-Implementation Checklist

Before implementing any new subcommand or feature:
1. Read complete spec (SPEC.md, data-model.md, contracts/)
2. Verify data structures match spec shapes exactly
3. Identify all spec-defined algorithms to implement precisely
4. Plan input validation and filtering per spec requirements
5. Ensure full config resolution with extends chains if needed
6. Verify JSON schema, ordering, and exit code contracts
7. List all spec-mandated tests to implement
8. Identify existing helpers/loaders to reuse (not reimplement)
9. Plan nextest test groups for new integration tests

Document this checklist in plan.md or PR description to prevent spec drift.

## Deferral Tracking

When implementing complex features in phases (MVP-first approach):

1. **Document deferrals in research.md** with numbered decisions explaining rationale
2. **Add deferred work to tasks.md** under a "## Deferred Work" section:
   - Reference the research.md decision number
   - Include specific acceptance criteria
   - Use `[Deferral]` tag in task description
3. **A spec is NOT complete** while deferred tasks remain unresolved

Example tasks.md entry:
```markdown
## Deferred Work

- [ ] T050 [Deferral] Thread resolved FeatureMetadata through flows per research.md Decision 6
  - **Decision**: Use from_config_entry() for MVP; from_resolved() when metadata available
  - **Rationale**: Requires architectural threading beyond MVP scope
  - **Acceptance**: featureMetadata includes version, name, description from resolved features
```

When reviewing PRs, verify research.md deferrals have corresponding tasks.md entries.

## Common Anti-Patterns to Avoid

- **Data Structure Mismatch**: Using `Vec` when spec defines `map<string, T>`
- **Incomplete Resolution**: Loading top-level config only, ignoring extends chains
- **Silent Fallbacks**: Passing invalid inputs to downstream logic instead of filtering
- **Ordering Violations**: Using `BTreeMap` when spec requires declaration order (use `Vec` or `IndexMap`)
- **Exit Code Gating**: Only honoring special exit codes in one output mode (spec applies to all)
- **Test Gaps**: Implementing features without spec-mandated tests
- **Missing Nextest Config**: Adding integration tests without configuring test groups
- **Suboptimal Test Grouping**: Using docker-exclusive when docker-shared would work
- **Untracked Deferrals**: Documenting deferrals in research.md without corresponding tasks.md entries

## Output Streams Contract

**JSON modes** (`--output json`, `--json`):
- **stdout**: Single JSON document only (newline terminated)
- **stderr**: All logs, diagnostics, progress via tracing

**Text modes** (default):
- **stdout**: Human-readable results only
- **stderr**: All logs, diagnostics, progress via tracing

**Examples:**
```bash
# JSON mode - safe parsing
deacon read-configuration --output json > config.json 2> logs.txt

# Text mode - human readable
deacon doctor > diagnosis.txt 2> logs.txt

# Parse JSON safely
OUTPUT=$(deacon features plan --json 2>/dev/null)
echo "$OUTPUT" | jq '.order'
```

## Makefile Targets Reference

**Fast Development Loop:**
- `make dev-fast` - fmt + clippy + fast tests (recommended during iteration)
- `make test-nextest-fast` - Unit/bins/examples + doctests (excludes docker/smoke)

**Targeted Testing:**
- `make test-nextest-unit` - Unit tests only (super fast)
- `make test-nextest-docker` - Docker integration tests
- `make test-nextest-smoke` - High-level smoke tests

**Full Validation:**
- `make test-nextest` - Complete parallel suite (before PR)
- `make release-check` - Full quality gate (fmt + clippy + test + build)

**Utilities:**
- `make test-nextest-audit` - View test group assignments
- `make fmt` - Format all code
- `make clippy` - Lint with warnings as errors
- `make coverage` - Generate coverage report with llvm-cov

## Important Files & References

**Must-Read Documentation:**
- `.specify/memory/constitution.md` - Development principles and constraints
- `AGENTS.md` - Quick reference for AI assistants
- `docs/subcommand-specs/*/SPEC.md` - Authoritative behavior specifications
- `docs/ARCHITECTURE.md` - Cross-cutting patterns (env probe caching, etc.)
- `.github/copilot-instructions.md` - Detailed development guidelines

**Key Implementation Files:**
- `crates/core/src/config.rs` - Configuration resolution with extends chains
- `crates/core/src/container_env_probe.rs` - Environment probing with caching
- `crates/core/src/feature_installer.rs` - OCI feature installation
- `crates/core/src/container_lifecycle.rs` - Lifecycle command execution
- `crates/deacon/src/commands/shared/` - Shared command helpers

**Configuration Files:**
- `.config/nextest.toml` - Test parallelization and grouping
- `Cargo.toml` - Workspace configuration
- `Makefile` - Common development tasks

## Dependencies & Toolchain

**Active Technologies:**
- Rust 1.70+ (Edition 2021, stable toolchain)
- `clap` - CLI argument parsing
- `serde`/`serde_json` - Configuration and JSON handling
- `tracing`/`tracing-subscriber` - Structured logging
- `thiserror` - Domain errors (core)
- `anyhow` - Error context (binary)
- `tokio` - Async runtime
- `reqwest` - HTTP client (rustls TLS)
- `cargo-nextest` - Parallel test execution

**Container Runtimes:**
- Docker (default, production-ready)
- Podman (in development)

## CI/CD Requirements

GitHub Actions runs on every PR:
- Format check: `cargo fmt --all -- --check` (must pass)
- Lint: `cargo clippy --all-targets -- -D warnings` (zero warnings)
- Tests: `make test-nextest-fast` (Ubuntu), `make test-nextest-ci` (full)
- Smoke tests: `make test-nextest-smoke` (serial with Docker)
- Coverage: Minimum threshold enforced via cargo-llvm-cov

**Common CI Failures:**
- Trailing whitespace anywhere (run `cargo fmt --all`)
- Clippy warnings (run `cargo clippy --all-targets -- -D warnings`)
- Doctest failures (missing trait imports, missing Default implementations)
- Test race conditions (reclassify to more conservative nextest group)

## Debugging Tips

**Enable debug logging:**
```bash
RUST_LOG=debug cargo run -- <command>
DEACON_LOG=deacon=trace,deacon_core=debug cargo run -- <command>
```

**Test a specific nextest group:**
```bash
cargo nextest run --profile full --filter-expr 'test(integration_)*'
```

**View nextest configuration:**
```bash
cargo nextest show-config test-groups
make test-nextest-audit
```

**Verify cache behavior:**
```bash
RUST_LOG=debug cargo run -- up --container-data-folder /tmp/cache
# Check /tmp/cache/env_probe_* files
```
