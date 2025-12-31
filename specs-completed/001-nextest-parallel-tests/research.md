# Research Log

- **Decision**: Manage parallelism through a checked-in `.nextest.toml` with explicit test groups (`docker-exclusive`, `docker-shared`, `fs-heavy`, `unit-default`, `smoke`, `parity`) and named profiles (`dev-fast`, `full`, `ci`).
  - **Rationale**: Central configuration keeps grouping discoverable, supports reuse across local and CI runs, and lets us enforce serialization for smoke/parity suites while raising concurrency for unit/integration tests.
  - **Alternatives considered**: Passing ad-hoc `cargo nextest run --run-group ...` flags (harder to standardize, error-prone); relying on nightly-only `-Z` options (unstable, conflicts with stable toolchain policy).

- **Decision**: Expose new Make targets (`test-nextest-fast`, `test-nextest`, `test-nextest-ci`) that wrap cargo-nextest invocations and perform a preflight check for the binary.
  - **Rationale**: Developers already rely on the Makefile for workflows; wrapping ensures consistent flags, environment setup, and failure messaging if cargo-nextest is absent.
  - **Alternatives considered**: Documenting raw commands without Make targets (increases drift); adding shell aliases (not shareable across environments).

- **Decision**: Install cargo-nextest in CI via the maintained `taiki-e/install-action@v2` GitHub Action and cache the toolchain using the existing Rust cache key.
  - **Rationale**: The action pins nextest versions, supports checksums, and avoids manual `cargo install` time; it integrates cleanly with existing GitHub Actions workflows.
  - **Alternatives considered**: Running `cargo install cargo-nextest` each job (slow, introduces version drift); prebuilding a custom CI image (higher maintenance).

- **Decision**: Fail fast when cargo-nextest is missing by checking `command -v cargo-nextest` before invoking and printing install guidance that links to https://nexte.st/book/getting-started.html.
  - **Rationale**: Aligns with the constitution's "No Silent Fallbacks" principle and gives developers a clear remediation path.
  - **Alternatives considered**: Falling back silently to `cargo test` (violates principle III); assuming developers install it manually (poor UX).

- **Decision**: Capture timing deltas by running `cargo nextest run --profile <profile> --final-status-reporter json` and writing aggregated durations to `artifacts/nextest/<profile>-timing.json` during CI, then summarizing them in job output.
  - **Rationale**: Provides structured data for SC-007 without polluting stdout; artifacts enable comparison across runs and manual inspection.
  - **Alternatives considered**: Parsing human-readable summaries (fragile); adding a bespoke timing harness (duplicate effort).

- **Decision**: Document test classification guidance in `docs/CLI_PARITY.md` (testing section) and a new `docs/testing/nextest.md`, referencing `cargo nextest list --status` to audit group assignments.
  - **Rationale**: Keeps maintainer workflow explicit and colocated with existing testing docs; the list command makes verification reproducible.
  - **Alternatives considered**: Relying on comments inside `.nextest.toml` only (harder to discover); keeping guidance solely in the spec (less accessible during development).
