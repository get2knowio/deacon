# GitHub Copilot Instructions for `deacon`

These instructions guide AI assistance (e.g. GitHub Copilot / Chat) when proposing code, architecture, or documentation changes for this repository.

## Prime Directives
1. Respect the authoritative CLI specification found at `docs/CLI-SPEC.md`. Treat it as a source-of-truth for architecture, workflow semantics, data shapes, and command behavior. If user requests conflict with the spec, explicitly call out the discrepancy and request clarification before generating code.
2. Prefer incremental, small, reviewable changes. Avoid large refactors unless explicitly requested.
3. Maintain idiomatic, modern Rust (Edition 2021) with clear module boundaries and test coverage for new logic.
4. Avoid introducing unsafe code. If absolutely necessary, justify with a comment explaining safety invariants.
5. **CRITICAL: Keep build green** - ALL code changes MUST pass the complete CI pipeline locally before submission. **Run checks after EVERY code change, not just before committing**:
   - `cargo build --verbose` (must compile successfully)
   - `cargo test --verbose -- --test-threads=1` (all tests must pass)
   - `cargo fmt --all` (format code immediately after changes)
   - `cargo fmt --all -- --check` (verify no formatting changes needed)
   - `cargo clippy --all-targets -- -D warnings` (no clippy warnings allowed)

## Scope & Architecture Alignment
The long-term goal is a Rust implementation of a DevContainer-like CLI. Align concepts with the spec's domains (configuration resolution, feature system, template system, Docker/OCI integration, lifecycle execution). For any new major subsystem:
- Reference relevant section headers from `CLI-SPEC.md` (e.g. "Feature Installation Workflow", "Configuration Resolution Workflow").
- Preserve terminology: *feature*, *template*, *lifecycle command*, *workspace*, *container environment*, *variable substitution*.
- Introduce abstractions behind traits to enable future alternate backends (e.g., Docker vs. Podman).

## Code Style & Patterns
- **Formatting & Quality**: Code MUST be properly formatted and pass all checks:
  - **CRITICAL**: Run `cargo fmt --all` after EVERY code change, not just before committing
  - Remove ALL trailing whitespace from source files (check with `cargo fmt --all -- --check`)
  - Follow standard Rust import ordering:
    1. Standard library (`use std::...`) 
    2. External crates (`use serde::...`)
    3. Local modules (`use crate::...`, `use super::...`)
  - Address all clippy warnings immediately - zero tolerance policy
- Error Handling: Prefer `thiserror` for domain error enums; use `anyhow` only at the binary boundary or for prototyping. Provide context with `.with_context(...)` where it aids diagnosis.
- Logging: Use `tracing` spans for multi-step workflows (configuration load, feature install, container build) and structured fields instead of string concatenation.
- Configuration Parsing: Plan for layered merges (defaults -> base -> extends -> overrides -> runtime). Keep parsing pure & testable.
- Dependency Injection: Pass traits (e.g., `ContainerRuntime`, `RegistryClient`) rather than concrete types to enable test doubles.
- Asynchronicity: Introduce async only when IO-bound (network, filesystem, process execution). Keep pure logic synchronous.
- Testing: For each new module add: (a) unit tests for pure logic, (b) integration test if it crosses process/container boundary stubs.
- Benchmarks: Add Criterion benchmarks only for performance-critical paths (parsing, resolution) and guard with `#[cfg(feature = "bench")]` if noise becomes an issue.

## File & Module Conventions
- Keep binary crate (`crates/deacon`) focused on CLI entrypoint, argument parsing, high-level orchestration.
- Extract future shared logic (parsing, model types, runtime abstraction) into planned `crates/core` (to be created) before it grows unwieldy.
- Group commands under a `commands` module with one file per top-level subcommand.
- Store integration tests under `crates/deacon/tests/` using descriptive filenames (`integration_<feature>.rs`).

## Pull Request Guidance (AI Generated or Assisted)
When proposing a change:
1. Brief summary (1–2 sentences) of intent referencing spec section(s).
2. List of modified files and rationale.
3. Risk assessment: breaking changes, API shifts, perf impact.
4. **Verification: MANDATORY CI validation** - ALL commands must pass locally after EVERY change:
   - `cargo build --verbose` ✅
   - `cargo test --verbose -- --test-threads=1` ✅ 
   - `cargo fmt --all` (format immediately) ✅
   - `cargo fmt --all -- --check` (verify formatting) ✅
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
- `cargo test --verbose` passes including smoke tests ✅
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

### Test Failures (`cargo test --verbose`)
- Ensure all tests pass locally before submission
- Update tests when changing functionality
- Write deterministic tests that don't depend on external state

### Build Failures (`cargo build --verbose`)
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
cargo build --verbose
cargo test --verbose -- --test-threads=1
cargo test --doc  # Verify all doctests pass
cargo fmt --all
cargo fmt --all -- --check  # Must show "no changes required"
cargo clippy --all-targets -- -D warnings
```

**Iterative Development Workflow**:
1. Make a small code change (add function, modify logic, etc.)
2. **Immediately** run `cargo fmt --all` 
3. Run `cargo build --verbose` to check compilation
4. Run relevant tests with `cargo test --verbose -- --test-threads=1`
5. Run `cargo clippy --all-targets -- -D warnings` 
6. Only proceed to next change if ALL steps pass
7. Before final commit, run the complete checklist above

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

### Prevention Workflow
1. Write code naturally, don't worry about formatting while coding
2. **Immediately** run `cargo fmt --all` after any change
3. Verify with `cargo fmt --all -- --check`  
4. If it shows changes needed, run `cargo fmt --all` again
5. Only proceed when `--check` shows "no changes required"

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

## Testing Strategy
- Favor deterministic tests; isolate environment-dependent logic behind trait abstractions with mock implementations.
- Use `assert_cmd` for end-to-end CLI invocation tests.
- Avoid network in unit tests; gate true integration (e.g., Docker) tests behind feature flags or CI-only markers later.

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

## Future Refactors (Do Not Prematurely Implement)
- Introduce `crates/core` with domain models & runtime abstraction.
- Swap out direct Docker CLI calls for an interface trait.
- Implement redaction middleware for logs & error chains.
- Add structured JSON logging mode toggle.

Until explicitly scheduled, treat these as roadmap items; do not create scaffolding that adds unused code.

---
If you are an AI assistant operating on this repository: remain concise, cite spec section names when relevant, and prefer patches over prose when user intent is clear.
