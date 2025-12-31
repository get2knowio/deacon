# GitHub Copilot Instructions for `deacon`

These instructions guide AI assistance (e.g. GitHub Copilot / Chat) when proposing code, architecture, or documentation changes for this repository.

## Prime Directives
1. Respect the authoritative CLI specification found at `docs/CLI-SPEC.md`. Treat it as a source-of-truth for architecture, workflow semantics, data shapes, and command behavior. If user requests conflict with the spec, explicitly call out the discrepancy and request clarification before generating code.
2. Prefer incremental, small, reviewable changes. Avoid large refactors unless explicitly requested.
3. Maintain idiomatic, modern Rust (Edition 2021) with clear module boundaries and test coverage for new logic.
4. Avoid introducing unsafe code. If absolutely necessary, justify with a comment explaining safety invariants.
5. **CRITICAL: Keep build green** - ALL code changes MUST pass the complete CI pipeline locally before submission. **Run checks after EVERY code change, not just before committing**:
   - `cargo build --quiet` (must compile successfully)
   - `cargo test --quiet -- --test-threads=1` (all tests must pass)
   - `cargo fmt --all --quiet` (format code immediately after changes)
   - `cargo fmt --all --quiet -- --check` (verify no formatting changes needed)
   - `cargo clippy --all-targets -- -D warnings` (no clippy warnings allowed)
6. **No Silent Fallbacks / Stubbed Behavior**: Production (non-test) code MUST NOT transparently downgrade, noop, or silently substitute mock/stub implementations when a capability (e.g., OCI / registry resolution, container runtime, feature install backend) is unavailable or unimplemented. Either:
  - Provide a fully working implementation, OR
  - Emit a clear, user-facing error (e.g., `Not implemented yet: OCI resolution`) and abort the workflow.
  Mocks and fakes are permitted ONLY inside tests. Do not wire mocks into runtime code paths or auto-detect-and-skip behavior; explicit failure is required to prevent hidden divergence from the spec.
7. **Ignore Legacy Task IDs**: Any 3-digit task IDs referenced in source code comments or documentation are from a legacy task list and are no longer valid. Do not attempt to resolve, update, or reference these IDs in any generated code, documentation, or analysis.

## Scope & Architecture Alignment
The long-term goal is a Rust implementation of a DevContainer-like CLI. Align concepts with the spec's domains (configuration resolution, feature system, template system, Docker/OCI integration, lifecycle execution). For any new major subsystem:
- Reference relevant section headers from `CLI-SPEC.md` (e.g. "Feature Installation Workflow", "Configuration Resolution Workflow").
- Preserve terminology: *feature*, *template*, *lifecycle command*, *workspace*, *container environment*, *variable substitution*.
- Introduce abstractions behind traits to enable future alternate backends (e.g., Docker vs. Podman).

## Code Style & Patterns
- **Formatting & Quality**: Code MUST be properly formatted and pass all checks:
  - **CRITICAL**: Run `cargo fmt --all` after EVERY code change, not just before committing
  - **CRITICAL**: Always verify formatting with `cargo fmt --all -- --check` before any commit
  - Remove ALL trailing whitespace from source files (check with `cargo fmt --all -- --check`)
  - **Common formatting failures that cause CI to fail:**
    - Trailing spaces in struct field definitions (e.g., `field: value, ` vs `field: value,`)
    - Missing trailing commas in multi-line struct literals
    - Inconsistent spacing in struct initialization blocks
    - Manual line breaking that conflicts with rustfmt preferences
  - Follow standard Rust import ordering:
    1. Standard library (`use std::...`) 
    2. External crates (`use serde::...`)
    3. Local modules (`use crate::...`, `use super::...`)
  - Address all clippy warnings immediately - zero tolerance policy
- Error Handling: Prefer `thiserror` for domain error enums; use `anyhow` only at the binary boundary or for prototyping. Provide context with `.with_context(...)` where it aids diagnosis.
- Logging: Use `tracing` spans for multi-step workflows (configuration load, feature install, container build) and structured fields instead of string concatenation.
- Configuration Parsing: Plan for layered merges (defaults -> base -> extends -> overrides -> runtime). Keep parsing pure & testable.
- Dependency Injection: Pass traits (e.g., `ContainerRuntime`, `RegistryClient`) rather than concrete types to enable test doubles.
  - Production code MUST bind only to real implementations; test doubles (mocks/fakes) stay confined to test modules. If a real implementation is not yet ready, fail fast with a clear `Not implemented yet` error rather than silently substituting a placeholder.
- Asynchronicity: Introduce async only when IO-bound (network, filesystem, process execution). Keep pure logic synchronous.
- Testing: For each new module add: (a) unit tests for pure logic, (b) integration test if it crosses process/container boundary stubs.
- Benchmarks: Add Criterion benchmarks only for performance-critical paths (parsing, resolution) and guard with `#[cfg(feature = "bench")]` if noise becomes an issue.

## File & Module Conventions
- Keep binary crate (`crates/deacon`) focused on CLI entrypoint, argument parsing, high-level orchestration.
- Extract future shared logic (parsing, model types, runtime abstraction) into planned `crates/core` (to be created) before it grows unwieldy.
- Group commands under a `commands` module with one file per top-level subcommand.
- Store integration tests under `crates/deacon/tests/` using descriptive filenames (`integration_<feature>.rs`).

## Development Tools
- Use ast-grep tool (command 'sg') for searching or rewriting code instead of find or grep.
- Use context7 MCP server for retrieving up-to-date documentation for libraries and packages.
- Use github MCP server for interacting with GitHub repositories, managing issues, pull requests, and code searches.

## Pull Request Guidance (AI Generated or Assisted)
When proposing a change:
1. Brief summary (1–2 sentences) of intent referencing spec section(s).
2. List of modified files and rationale.
3. Risk assessment: breaking changes, API shifts, perf impact.
4. **Verification: MANDATORY CI validation** - ALL commands must pass locally after EVERY change:
   - `cargo build --quiet` ✅
   - `cargo test --quiet -- --test-threads=1` ✅ 
   - `cargo fmt --all --quiet` (format immediately) ✅
   - `cargo fmt --all --quiet -- --check` (verify formatting) ✅
   - `cargo clippy --all-targets -- -D warnings` ✅
5. Follow-ups / deferred work (explicit list) if any.

### Smoke Tests Maintenance
- Keep the smoke tests under `crates/deacon/tests/smoke_basic.rs` up to date when changing CLI behavior for:
  - read-configuration
  - build
  - up (traditional) and exec
  - doctor
- Ensure tests are resilient in environments without Docker by accepting well-defined Docker-unavailable errors.
- When adding a new user-facing subcommand or changing flags/output, extend the smoke tests accordingly.

Add to Pre-submission Checklist:
- `cargo test --quiet` passes including smoke tests ✅
- If behavior changed, corresponding assertions in `smoke_basic.rs` updated ✅

## Dependency Management
- Use `cargo add <crate> --workspace` for shared dependencies; otherwise target the specific crate manifest.
- Keep dependency set lean; justify additions (functionality vs. complexity). Avoid duplicating features already available in standard library or existing deps.
- For upgrades prefer `cargo update` (lock refresh) first; use `cargo upgrade --workspace` only when intentional about semver changes.

## CI/CD Requirements & Preventing Build Failures
**CRITICAL**: The CI pipeline runs on every PR and ALL checks must pass. Common failure causes and prevention:

### Formatting Failures (`cargo fmt --all -- --check`)
- **Always run `cargo fmt --all` after EVERY code change, not just before committing**
- **Common formatting issues that cause failures:**
  - Incorrect line breaking in complex `if` conditions - let rustfmt handle this automatically
  - Manual import formatting - always let rustfmt organize imports
  - Trailing whitespace in any source file (Rust, TOML, markdown, etc.)
  - Inconsistent indentation (spaces vs tabs)
- **Prevention strategy:**
  - Run `cargo fmt --all` immediately after writing any code
  - Check with `cargo fmt --all -- --check` before any commit
  - Use editor plugins that auto-format on save and show trailing whitespace
- Follow standard Rust import ordering (rustfmt will enforce this):
  1. Standard library (`use std::...`)
  2. External crates (`use serde::...`) 
  3. Local modules (`use crate::...`, `use super::...`)

### Clippy Failures (`cargo clippy --all-targets -- -D warnings`)
- Address ALL clippy warnings - zero tolerance policy
- Common issues: unused variables, unnecessary clones, style violations
- Run with `-D warnings` flag to treat warnings as errors locally
- Use `#[allow(clippy::...)]` sparingly and only with justification

### Test Failures (`cargo test --quiet`)
- Ensure all tests pass locally before submission
- Update tests when changing functionality
- Write deterministic tests that don't depend on external state

### Build Failures (`cargo build --quiet`)
- Code must compile cleanly on stable Rust
- Check for missing imports, type errors, unused dependencies
- Verify workspace configuration is correct

### Doctest Failures (`cargo test --doc`)
- **All documentation examples must compile and run successfully**
- **Common doctest issues that cause CI failures:**
  - Missing trait imports in doctest scope (e.g., `clap::Parser` for CLI parsing)
  - Struct/enum missing `Default` implementations when used in examples
  - Incorrect path references in doctests (`crate::` vs proper module paths)
  - Function visibility issues (referencing private functions in public doctests)
- **Prevention strategies:**
  - Add required trait imports at the top of doctest examples
  - Implement `Default` trait for structs used in doctests when appropriate
  - Use proper module paths that work from external crate perspective
  - Avoid referencing internal/private functions in public API doctests
  - Test doctests locally with `cargo test --doc -p <crate>` before submitting
- **Common fixes:**
  ```rust
  /// # Examples  
  /// ```
  /// use clap::Parser;  // Add missing trait import
  /// use your_crate::SomeStruct;  // Use proper external path
  /// let example = SomeStruct::default();  // Ensure Default is implemented
  /// ```
  ```

**Pre-submission Checklist**:
```bash
# Run this exact sequence after EVERY code change AND before every commit:
cargo build --quiet
cargo test --quiet -- --test-threads=1
cargo test --doc  # Verify all doctests pass
cargo fmt --all --quiet
cargo fmt --all --quiet -- --check  # Must show "no changes required"
cargo clippy --all-targets -- -D warnings
```

### Agentic Fast Loop Mode (local-only)
- Use `make dev-fast` for rapid iterations; it avoids Docker-heavy suites and long-running integration tests.
- Recommended cadence: run `make test-non-smoke` every few iterations if you touched parsing/validation; run `make test-smoke` when touching Docker lifecycle; run `make release-check` before commits/PRs.
- This preserves the “keep build green” principle while reducing iteration time.

**Iterative Development Workflow**:
1. Make a small code change (add function, modify logic, etc.)
2. **Immediately** run `cargo fmt --all`
3. Pick your test cadence:
  - Fast Loop (default during spec-phase): `make dev-fast` (fmt-check + clippy + unit/bins/examples + doctests; skips slow integration/smoke)
  - Full Loop (periodic/at milestones): `make test` (all tests, serial) or `make release-check` (full gate)
4. If using Fast Loop, still run a Full Loop when you change CLI behavior or runtime paths, add/modify integration tests, or before pushing a branch/PR.
5. Only proceed to next change if your chosen loop passes.
6. Before final commit, run the complete checklist above.

> **CRITICAL**: Never make multiple changes before validating each one. Always fix formatting and clippy issues immediately after each small change. Do not accumulate technical debt or "fix it later" - the CI will fail and block the PR.

If any command fails, fix the issues before submitting. The CI will run these exact same checks.

## Formatting Best Practices & Common Pitfalls

### Critical Formatting Rules
- **NEVER manually format code** - always let `cargo fmt` handle formatting
- **Run `cargo fmt --all` immediately after any code change**
- **Always verify with `cargo fmt --all -- --check` before committing**

### Common Formatting Issues That Cause CI Failures
1. **Complex conditional statements**: Don't manually break lines in `if` conditions
   ```rust
   // ❌ BAD - manual line breaking
   if let Some(cycle) = Self::dfs_find_cycle(dep, graph, visited, rec_stack, path)
   {
       return Some(cycle);
   }
   
   // ✅ GOOD - let rustfmt handle it
   if let Some(cycle) = Self::dfs_find_cycle(dep, graph, visited, rec_stack, path) {
       return Some(cycle);
   }
   ```

2. **Import statements**: Don't manually format imports
   ```rust
   // ❌ BAD - manual line breaking
   use deacon_core::features::{
       FeatureDependencyResolver, FeatureMetadata, ResolvedFeature,
   };
   
   // ✅ GOOD - let rustfmt handle it
   use deacon_core::features::{FeatureDependencyResolver, FeatureMetadata, ResolvedFeature};
   ```

3. **Trailing whitespace**: Check ALL file types (not just .rs files)
   - Rust source files (.rs)
   - TOML files (Cargo.toml, etc.)
   - Markdown files (.md) 
   - YAML files (.yml)

4. **Struct literal formatting**: Common CI failure patterns
   ```rust
   // ❌ BAD - trailing spaces and inconsistent formatting
   Self { 
       field1: value1, 
       field2: value2, 
       field3: value3 
   }
   
   // ✅ GOOD - consistent formatting with trailing comma
   Self {
       field1: value1,
       field2: value2,
       field3: value3,
   }
   ```

### Prevention Workflow - MANDATORY for Every Code Change
1. Write code naturally, don't worry about formatting while coding
2. **Immediately** run `cargo fmt --all` after any change
3. **ALWAYS** verify with `cargo fmt --all -- --check`  
4. If it shows changes needed, run `cargo fmt --all` again
5. Only proceed when `--check` shows "no changes required"
6. **CRITICAL**: Never commit code that fails `cargo fmt --all -- --check`

### Emergency Formatting Fix Process
If CI formatting checks are failing:
1. Run `cargo fmt --all` to fix formatting
2. Verify with `cargo fmt --all -- --check` 
3. Commit the formatting fix immediately
4. Update instruction file if new formatting patterns emerge

## Logging & Observability
- Provide consistent span names aligned with spec workflows: `config.resolve`, `container.create`, `feature.install`, `template.apply`, `lifecycle.run`.
- Include identifiers (workspace root hash, feature id, template id) as span fields for trace correlation.

## Error Taxonomy (Planned)
Structure domain errors mirroring spec categories:
- Configuration
- Docker / Runtime
- Feature
- Template
- Network
- Validation
- Authentication

Each error enum variant should carry minimal, actionable context. Prefer converting lower-level errors with `#[from]` where appropriate.

## Security & Safety Considerations
- Never execute arbitrary shell supplied by unvalidated user input without sanitization.
- Surface potentially destructive operations (container removal, volume pruning) behind explicit flags.
- Avoid leaking secrets in logs; plan future redaction layer (Issue #41) — placeholder utilities should be clearly marked.

## Performance Practices
- Defer optimization until profiling indicates hotspots.
- Use iterators and slices; avoid unnecessary allocations (`String` ↔ `&str` conversions).
- Cache parsed configuration objects if re-used across steps; invalidate on mtime/hash change.

## Documentation Expectations
- Public functions in core modules require concise rustdoc summarizing purpose, inputs, and failure modes.
- Keep README & CONTRIBUTING authoritative for dev workflow; avoid duplicating extended rationale (link to spec sections instead).
- When adding a feature touching spec semantics, include a short `docs/` note referencing the relevant workflow diagram.

## Release Notes Automation & Labeling Policy
We use GitHub's auto-generated release notes with category configuration in `.github/release.yml`. To ensure high-quality release notes, every AI-assisted PR MUST:

- Use Conventional Commits style for the PR title (also acceptable as the squash-merge commit title):
  - `feat: …`, `fix: …`, `perf: …`, `docs: …`, `refactor: …`, `ci: …`, `build: …`, `chore: …`.
  - Include `BREAKING CHANGE:` in the PR description footer for any backward-incompatible change.

- Apply at least one of these labels (choose the most user-impacting primary label):
  - Breaking changes: `breaking-change` (plus the appropriate type label below)
  - Features: `feature`, `feat`, or `enhancement`
  - Fixes: `fix`, `bug`, or `bugfix`
  - Performance: `perf`
  - Documentation: `docs`
  - Refactors: `refactor`
  - CI/CD & Build: `ci`, `build`
  - Dependencies: `deps`, `dependencies`
  - Chore & Maintenance: `chore`, `maintenance`

- Exclude from changelog when appropriate by adding one of:
  - `skip-changelog`, `no-release-notes`

Notes
- These labels map directly to `.github/release.yml` categories used by the Release workflow (`release.yml`).
- If multiple labels apply, prefer ONE primary label; add a secondary label only if it significantly improves categorization.
- For breaking changes, always add `breaking-change` and describe the migration in the PR body under a "Migration" subsection.

Definition of Done for PRs (in addition to quality gates above)
- PR title follows Conventional Commits and describes user-visible impact.
- Appropriate labels applied from the list above.
- Docs updated for user-visible changes (README/examples/spec references as needed).
- Tests updated/added for behavior changes.

## Examples Maintenance
The `examples/` directory and configuration/feature/template fixtures under `examples/` and `fixtures/` are living documentation and MUST reflect current CLI behavior defined in `docs/CLI-SPEC.md`.

Guidelines:
- Prefer minimal, focused examples that each illustrate one primary concept: configuration resolution, variable substitution, feature options, template options, lifecycle commands, etc.
- When introducing or modifying a user‑visible flag, subcommand, feature schema field, or template option, update (or add) an example demonstrating the change in `examples/` and, where parsing/validation is exercised, a corresponding lightweight fixture in `fixtures/` for tests.
- Keep `examples/README.md` curated: add a short entry (one sentence + relative path) for each new example so discoverability stays high.
- Align example names and inline comments with spec terminology ("feature", "template", "lifecycle command", "workspace"). Avoid ad‑hoc synonyms.
- If an example becomes redundant due to broader one covering the same concept, remove it in the same PR (explicitly call this out in the PR body) to avoid drift and noise.
- Ensure examples are lint‑clean: they should compile / pass parsing when invoked via the CLI (add or extend a smoke/integration test if newly cover behavior not previously tested).
- For variable usage examples, prefer deterministic placeholders over secrets (e.g. `MY_TOKEN` with a comment rather than a fabricated value).
- Keep example JSON/TOML/YAML consistently formatted (run through `rustfmt` for Rust snippets; rely on editor/formatter for structured files) and free of trailing whitespace.

Validation Flow (recommended when adding/modifying examples):
1. Add or update the example content.
2. Run the CLI against the example (`cargo run -- read-configuration --path examples/...`) to confirm parsing success.
3. If it highlights new behavior, add/adjust an integration test referencing the matching fixture.
4. Update `examples/README.md` index section.
5. Re-run full checklist (build, tests, fmt, clippy).

PR Body Expectations (when examples change):
- Brief rationale for new/updated/removed examples.
- Mapping to spec section(s) demonstrating relevance.
- Note any added/adjusted tests ensuring coverage.

Failure to update examples when altering user‑facing behavior increases drift risk; treat missing updates as a review blocker.

## Testing Strategy
- Favor deterministic tests; isolate environment-dependent logic behind trait abstractions with mock implementations.
- Use `assert_cmd` for end-to-end CLI invocation tests.
- Avoid network in unit tests; gate true integration (e.g., Docker) tests behind CI-only markers or environment variables if needed.
- Never rely on production fallback logic to skip unimplemented features; tests should assert explicit error variants/messages for not-yet-implemented functionality. Mocks/fakes are tools for isolation in tests only and must not leak into shipped execution paths.

## OCI Registry & HTTP Client Implementation Guidelines

### HTTP Client Trait Pattern
When modifying `HttpClient` trait or implementing OCI registry operations:

1. **HEAD Requests for Blob Existence**: Always use HEAD requests (not GET) to check if a blob exists before uploading:
   ```rust
   // ✓ CORRECT - Uses HEAD which doesn't download the body
   match self.client.head(&blob_url, HashMap::new()).await {
       Ok(status) if status == 200 => return Ok(()), // Blob exists
       Ok(status) if status == 404 => { /* proceed with upload */ }
       _ => { /* handle error */ }
   }
   
   // ✗ WRONG - GET downloads entire blob unnecessarily
   match self.client.get(&blob_url).await {
       Ok(_) => return Ok(()),
       ...
   }
   ```

2. **Location Header Handling**: OCI Distribution Spec v2 requires using the Location header from POST responses:
   ```rust
   // ✓ CORRECT - Extract and use Location header
   let response = self.client.post_with_headers(&upload_url, ...).await?;
   let location = response.headers.get("location")
       .ok_or_else(|| "Missing Location header")?;
   let upload_location = format!("{}?digest={}", location, digest);
   
   // ✗ WRONG - Hardcoding upload URL construction
   let upload_location = format!("{}?digest={}", upload_url, digest);
   ```

3. **POST Response Structure**: When changing `post_with_headers` to return headers:
   - Update trait signature to return `HttpResponse` struct instead of `Bytes`
   - Update ALL implementations including test mocks (MockHttpClient, AuthMockHttpClient, etc.)
   - `HttpResponse` struct must include: status, headers (HashMap), body (Bytes)

4. **Trait Method Additions Checklist**:
   When adding new methods to `HttpClient` trait (or any public trait):
   - [ ] Update trait definition with `#[async_trait::async_trait]` attribute
   - [ ] Update production implementation (`ReqwestClient`)
   - [ ] Update all test mock implementations:
     - [ ] `MockHttpClient` in `crates/core/src/oci.rs`
     - [ ] `AuthMockHttpClient` in `crates/core/tests/integration_oci_auth.rs`
     - [ ] `MockAuthReqwestClient` in `crates/core/tests/integration_oci_auth.rs`
     - [ ] Any test-specific mocks (e.g., `FailingMockClient`, `AlwaysFailingClient`)
   - [ ] Run `cargo clippy` to catch missing implementations early
   - [ ] Update integration tests to exercise new methods

5. **Mock Response Setup for OCI Uploads**:
   Tests should simulate realistic OCI upload flow with proper Location headers:
   ```rust
   // Set up POST response with Location header
   let upload_uuid = "550e8400-e29b-41d4-a716-446655440000";
   let location = format!("/v2/{}/blobs/uploads/{}", repo, upload_uuid);
   let mut headers = HashMap::new();
   headers.insert("location".to_string(), location.clone());
   
   fake_registry.mock_client.add_response_with_headers(
       upload_init_url,
       HttpResponse { status: 202, headers, body: Bytes::from("") }
   ).await;
   
   // Mock PUT at the Location + digest
   let upload_complete_url = format!("{}?digest={}", location, layer_digest);
   fake_registry.mock_client.add_response(upload_complete_url, ...).await;
   ```

### Common Pitfalls to Avoid
- **Don't** use GET requests to check blob existence (wastes bandwidth)
- **Don't** ignore Location headers from POST /blobs/uploads/ responses
- **Don't** forget to update test mocks when changing trait methods
- **Don't** hardcode upload URLs instead of using Location header
- **Do** test with realistic mock responses including proper status codes (202 for upload initiation, 201/204 for completion)
- **Do** ensure error handling distinguishes 404 (not found), 401/403 (auth failure), and 5xx (server error)

## Adherence to `CLI-SPEC.md`
> IMPORTANT: All generated code, designs, and refactors MUST remain consistent with `docs/CLI-SPEC.md`. If a requested change deviates (e.g., new command semantics, altered lifecycle order, renamed workflow), respond with a clarification prompt and do not implement until resolved.

Checklist before submitting AI-generated PR suggestions:
- [ ] **All CI checks pass locally after EVERY code change** (build, test, fmt, clippy)
- [ ] **Code properly formatted with `cargo fmt --all` after each change**
- [ ] **No trailing whitespace or formatting inconsistencies anywhere**
- [ ] **Verified with `cargo fmt --all -- --check` (shows "no changes required")**
- [ ] Referenced relevant spec sections? (list in PR body)
- [ ] No stray `dbg!` / commented-out code
- [ ] No new `unsafe` blocks
- [ ] Errors mapped to domain taxonomy (or TODO noted)
- [ ] Added or updated tests for new logic paths
- [ ] Documentation (rustdoc / README / docs/) updated
 - [ ] Examples & fixtures updated (or explicitly confirmed not needed) and `examples/README.md` index maintained

## Future Refactors (Do Not Prematurely Implement)
- Introduce `crates/core` with domain models & runtime abstraction.
- Swap out direct Docker CLI calls for an interface trait.
- Implement redaction middleware for logs & error chains.
- Add structured JSON logging mode toggle.

Until explicitly scheduled, treat these as roadmap items; do not create scaffolding that adds unused code.

---
If you are an AI assistant operating on this repository: remain concise, cite spec section names when relevant, and prefer patches over prose when user intent is clear.

## Active Technologies
- Rust (stable channel per rust-toolchain.toml) + clap (CLI args), serde/serde_json (JSON), tracing (logs), thiserror (errors) (001-read-config-parity)
- Rust (stable toolchain per `rust-toolchain.toml`, Edition 2021) + clap (CLI), serde/serde_json (parsing/JSON), thiserror (errors), tracing (logs)
- Rust (stable toolchain, Edition 2021) + `tar` (archiving), `flate2` (gzip), `serde`/`serde_json` (metadata), `clap` (CLI), `tracing` (logs), `thiserror` (errors) (002-features-package-collection)
- Local filesystem (read sources, write artifacts) (002-features-package-collection)
- Rust (Edition 2021; stable per `rust-toolchain.toml`) + `clap` (CLI), `serde`/`serde_json` (I/O), `tracing` (logs), `thiserror` (errors), HTTP client (reqwest-based), internal `RegistryClient`/`HttpClient` traits (core) (003-features-publish-compliance)
- N/A (OCI registry as remote store) (003-features-publish-compliance)
- Rust (Edition 2021), workspace rust-version 1.70, stable toolchain (rustfmt, clippy) + clap (CLI), serde/serde_json (I/O), tracing (logs), thiserror (errors), tokio (async), reqwest (HTTP, rustls TLS), semver (versioning) (003-features-publish-compliance)
- Rust (stable, Edition 2021) + clap (CLI), tracing (logs), serde/serde_json (JSON), thiserror (errors), reqwest (HTTP with rustls), tokio (async) (004-close-features-info-gap)
- N/A (read‑only network/file operations) (004-close-features-info-gap)
- Rust (stable; per `rust-toolchain.toml`) + `clap` (CLI args), `serde`/`serde_json` (JSON), `tracing` (logs), `thiserror` (errors); Docker/engine integration via existing runtime helpers in the repo (005-features-test-gap)
- N/A (ephemeral workspaces only) (005-features-test-gap)
- Rust 1.70 (workspace toolchain), cargo-nextest 0.9.x + cargo-nextest, GNU Make, GitHub Actions, existing deacon/deacon-core crates (001-nextest-parallel-tests)
- Rust 1.70 (workspace toolchain per `rust-toolchain.toml`) + `clap` for CLI parsing, `serde`/`serde_json` for metadata and output, Docker CLI/buildx via process invocation, internal traits/tools for features and configuration resolution (006-build-subcommand)
- N/A (temporary filesystem artifacts only) (006-build-subcommand)

## Recent Changes
- 001-read-config-parity: Added Rust (stable channel per rust-toolchain.toml) + clap (CLI args), serde/serde_json (JSON), tracing (logs), thiserror (errors)
