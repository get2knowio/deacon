<!--
Sync Impact Report
- Version change: 1.11.0 → 1.12.0
- Modified principles:
  - III. No Silent Fallbacks — Fail Fast: Added "CLI Argument Validation" subsection requiring
    validation of CLI arguments at ingress with clear error messages for invalid values.
  - IV. Idiomatic, Safe Rust: Added "Pattern Matching Over Unwrap" guideline for nested Options
    and "Documentation-Code Sync" requirement for keeping docstrings accurate after refactoring.
  - VII. Subcommand Consistency & Shared Abstractions: Added "Avoid Redundant Operations" guideline
    to prevent duplicate filesystem/network calls across shared and subcommand-specific code.
- Added sections: None
- Removed sections: None
- Templates requiring updates/alignment:
  - ✅ .specify/templates/plan-template.md (constitution check remains valid)
  - ✅ .specify/templates/spec-template.md (no changes needed)
  - ✅ .specify/templates/tasks-template.md (no changes needed)
  - ✅ CLAUDE.md (no changes needed - existing guidance covers new additions)
- Follow-up TODOs: None
-->

# deacon Constitution
<!-- A Rust DevContainer-like CLI aligned with containers.dev and the repo's CLI spec -->

## Core Principles

### I. Spec‑Parity as Source of Truth
Deacon MUST implement behavior consistent with the authoritative CLI specification in `docs/CLI-SPEC.md` and the
containers.dev ecosystem. Terminology (feature, template, lifecycle, workspace, container environment, variable
substitution) MUST be preserved. Any requested change conflicting with the spec requires explicit clarification and
spec updates before implementation; examples and fixtures MUST be kept in sync.

**Data Model and Algorithm Alignment**: Implementation data structures MUST match spec-defined shapes exactly (e.g.,
`map<string, T>` cannot be substituted with `Vec<T>`); null handling, field presence, and ordering requirements are
binding contracts. When the spec defines an algorithm (e.g., version derivation, configuration resolution with extends,
tag selection logic), the implementation MUST follow it step-by-step; do not invent shortcuts or "close enough"
alternatives that break spec guarantees.

**Configuration Resolution**: Commands that operate on devcontainer configuration MUST use the full resolution path
(includes `extends` chains, overrides, variable substitution) as defined in the spec, not just the top-level file.
Failing to resolve the complete effective configuration violates contract expectations and produces incomplete results.

**Phased Implementation**: Complex spec features MAY be implemented in phases (MVP-first), provided:
1. Core data structures and contracts are established in the first phase
2. Placeholder integration points are explicitly documented (e.g., "Future: docker inspect for labels")
3. Design decisions explaining phased approach are recorded in `research.md` with clear rationale
4. Tests verify the implemented portion functions correctly with the available data
5. Future work to complete full data population is clearly scoped

This pattern is valid when full implementation requires architectural threading (e.g., passing resolved features
through execution flows) that extends beyond MVP scope. The key is explicit documentation—reviewers should find
answers to "why isn't X fully populated?" in research.md decisions.

**Deferral Tracking**: When work is deferred per the phased implementation pattern above, deferrals MUST be:
1. Documented in `research.md` with numbered decisions explaining rationale
2. Added to `tasks.md` under a dedicated "## Deferred Work" section with:
   - Task IDs continuing the numbering sequence (e.g., T050, T051)
   - Clear reference to the research.md decision that created the deferral
   - Specific acceptance criteria for completing the deferred work
3. Tracked until completion—a specification is NOT considered complete while deferred tasks remain

The "Deferred Work" section in tasks.md ensures visibility and prevents deferrals from being forgotten. Reviewers
MUST verify that any research.md deferrals have corresponding tasks.md entries before approving PRs.

### II. Keep the Build Green (Non‑Negotiable)
All code changes MUST keep the build green with an explicit cadence for quick vs. full checks:
- Fast Loop (default during spec‑phase, local only):
  - `cargo fmt --all && cargo fmt --all -- --check`
  - `cargo clippy --all-targets -- -D warnings`
  - Fast tests only: `make test-nextest-fast` (unit/bins/examples + doctests; excludes smoke/parity/docker)
- Full Gate (periodic and before push/PR):
  - `cargo build --verbose`
  - `make test-nextest` (full parallel suite with all integration tests)
  - `cargo test --doc`
  - `cargo fmt --all && cargo fmt --all -- --check`
  - `cargo clippy --all-targets -- -D warnings`

**Testing Command Standard**: Use `make test-nextest-*` targets exclusively for running test suites. These targets
provide optimal parallelization, timing artifacts, and consistent configuration. Available targets:
- `make test-nextest-fast` - Fast parallel subset (excludes smoke/parity/docker)
- `make test-nextest-unit` - Only unit tests (super fast)
- `make test-nextest-docker` - Only docker integration tests
- `make test-nextest-smoke` - Only smoke tests with conservative profile
- `make test-nextest-long-running` - Long-running integration tests
- `make test-nextest` - Full parallel suite (use before PR)
- `make test-nextest-ci` - CI profile with conservative settings

Public behavior changes MUST update tests and examples accordingly. Use targeted test commands during development
when touching relevant areas, and always run `make test-nextest` before PR.

**Fix, Don't Skip**: When tests fail, they MUST be fixed—not disabled, not marked `#[ignore]`, not skipped. If a test
failure cannot be resolved (e.g., reveals fundamental implementation issues, requires capabilities not yet available,
or exposes spec ambiguities), work MUST STOP until the issue is properly addressed. Do not proceed with incomplete or
broken functionality. This is non-negotiable: a failing test indicates broken code, and broken code does not ship.

**Pre-Implementation Validation**: Before writing implementation code for a new subcommand or feature:
1. Read the complete spec section (SPEC.md, data-model.md, contracts/) to understand all requirements
2. Identify all data structures, algorithms, and behavioral contracts defined in the spec
3. Map out which existing infrastructure (e.g., `ConfigLoader::load_with_extends`, error types) must be used
4. Create a checklist of spec-mandated behaviors (flags, exit codes, output formats, ordering) to verify during implementation
5. Do not proceed with implementation until this validation is documented (e.g., in a plan.md or PR description)

This gate prevents "implement first, discover spec mismatch later" cycles that generate technical debt.

### III. No Silent Fallbacks — Fail Fast
Production code MUST NOT silently downgrade, noop, or substitute mock/stub implementations when capabilities (OCI,
registry resolution, container runtime, feature install backend) are unavailable or unimplemented. The program MUST
emit a clear, user‑facing error and abort. Mocks/fakes are permitted ONLY in tests and MUST NOT leak into runtime
code paths.

**Input Validation and Filtering**: When the spec defines which inputs are valid or supported (e.g., only OCI feature
refs, only semver tags), the implementation MUST filter or skip invalid entries as specified—do not pass them through
to downstream logic where they cause confusing errors or bogus output. Explicit parsing and validation at ingress
points prevents cascading failures and keeps error messages clear and actionable.

**CLI Argument Validation**: CLI arguments with constrained value sets (e.g., `--consistency` accepting only "cached",
"consistent", "delegated") MUST be validated at the argument parsing/normalization layer with clear error messages
listing valid options. Do not defer validation to downstream code where invalid values cause opaque runtime failures.
Define valid values as constants and validate against them early:
```rust
const VALID_CONSISTENCY: &[&str] = &["cached", "consistent", "delegated"];
if !VALID_CONSISTENCY.contains(&value.as_str()) {
    return Err(anyhow!("Invalid value '{}'. Valid: {}", value, VALID_CONSISTENCY.join(", ")));
}
```

### IV. Idiomatic, Safe Rust
Code MUST be modern, idiomatic Rust (Edition 2021) with clear module boundaries, no `unsafe` (unless absolutely
required and fully justified with documented safety invariants). Error handling: prefer `thiserror` for domain
errors in core; use `anyhow` only at the binary boundary with meaningful context. Abstractions SHOULD be expressed
via traits (e.g., `ContainerRuntime`, `RegistryClient`) to enable alternate backends; production binds to real
implementations. Introduce async only for IO‑bound work. Logging uses `tracing` with structured fields and spans
aligned to workflows (e.g., `config.resolve`, `feature.install`, `container.create`, `lifecycle.run`). Formatting
and imports are enforced via rustfmt; imports order: std → external crates → local modules.

**Error Propagation**: Use `Result` types consistently; do not swallow errors with unwraps or unchecked `expect`
calls or by returning sentinel values when operations can fail. Provide error context with `anyhow::Context` or
equivalent. Avoid direct `std::process::exit` calls; implement proper `Termination` or error wrappers so cleanup
and testing work correctly. Runtime code MUST be panic-free for expected failure modes—propagate, don’t crash.

**Async Discipline**: Do not block the async runtime with synchronous IO (e.g., `std::process::Command::output()`,
`std::fs::read_to_end`) inside async functions. Use async equivalents (`tokio::process::Command` with streaming,
`tokio::fs`, `tokio::io`). Long-running or CPU-heavy work should be offloaded to blocking tasks with clear bounds.

**Modular Boundaries Over Monoliths**: Large commands and clients MUST be decomposed into focused modules with
clear APIs (e.g., `plan`, `package`, `publish`, `test` for features; `auth`, `client`, `semver`, `install` for OCI;
`args`, `config`, `compose`, `runtime` for `up`). Keep public surface area minimal and reuse shared helpers instead
of duplicating logic in sprawling single files.

**Dependency Hygiene**: Keep dependencies current and avoid deprecated crates. When a dependency is deprecated or
superseded (e.g., `atty` → `is-terminal`), migrate promptly. Prefer minimal, stable dependencies with active
maintenance.

**Pattern Matching Over Unwrap**: When extracting values from nested `Option` types or `Result` types where the value
is logically guaranteed to exist, use pattern matching or `if let` instead of `.unwrap()`. This prevents fragile code
that could panic if invariants change:
```rust
// BAD: Fragile unwrap on nested Option
if let Some(result) = find_root(&path)? {
    return Ok(result.root.unwrap());  // panics if invariant broken
}

// GOOD: Pattern matching enforces the invariant structurally
if let Some(Result { root: Some(path), .. }) = find_root(&path)? {
    return Ok(path);  // type system ensures path exists
}
```

**Documentation-Code Sync**: When refactoring functions to change their behavior, docstrings MUST be updated in the
same commit. Stale documentation that contradicts actual behavior is a bug. Use `cargo doc --open` to verify rendered
documentation matches implementation. Pay particular attention to "Note:" sections that describe what a function does
NOT do—these often become incorrect after capability additions.

**Environment Variable Constants**: Environment variable names used in multiple places MUST be defined as constants
(e.g., `const ENV_FORCE_TTY_IF_JSON: &str = "DEACON_FORCE_TTY_IF_JSON";`) rather than repeated string literals.
This prevents typos, ensures consistency, and makes the codebase easier to search and refactor. Document the
constant with its purpose and valid values.

### V. Observability and Output Contracts
Stdout/stderr separation is a contract:
- JSON modes (`--json`, `--output json`): stdout contains only the single JSON document; all logs go to stderr.
- Text modes: stdout contains human‑readable results; all logs/diagnostics go to stderr via `tracing`.

Log format and level are configurable; structured JSON logs are supported. Secret values MUST be redacted by
default in logs. Span names and fields MUST reflect spec workflows for traceability. Release hygiene follows
Conventional Commits; labels drive release notes; examples and fixtures MUST remain representative and pass parsing.

**Schema and Ordering Compliance**: JSON output MUST conform exactly to the spec-defined schema (key names, field
presence, null handling). When the spec requires declaration order to be preserved (e.g., features in config order),
use ordered data structures (`Vec`, `IndexMap`) during serialization—do not allow implicit alphabetical reordering
via `BTreeMap` or similar. Text output MUST honor the same ordering guarantees when specified.

**Exit Code Contracts**: When the spec defines special exit codes (e.g., exit 2 for "outdated detected"), honor them
in ALL output modes (text, JSON, interactive, non-interactive). Do not gate exit code behavior on output format
unless explicitly specified.

### VI. Testing Completeness
All spec-mandated tests MUST be implemented before a feature is considered complete. When a spec includes a "Testing"
section or lists required test scenarios, treat them as acceptance criteria:
- Unit tests for pure logic (version comparison, parsing, validation)
- Integration tests for workflows crossing process/runtime boundaries (with mocked external dependencies)
- Output format tests (text rendering, JSON schema validation, ordering)
- Exit code tests (success, failure, special codes like --fail-on-outdated)
- Edge case and resilience tests (empty inputs, invalid refs, network failures with mocks)
- Doctests for public APIs and helpers

Tests MUST be deterministic and hermetic (no network); use fixtures and mocked registries. When an integration test
passes but codifies incorrect behavior (e.g., testing for alphabetical order when spec requires declaration order),
it is a bug—fix the implementation and update the test to assert correct behavior.

**Ignored Tests Require Tracking**: When a test is marked `#[ignore]` because it requires specific environment setup
(e.g., PTY allocation failure simulation) or cannot be reliably automated, it MUST include a tracking issue number
or detailed manual testing procedure in the test comment. Ignored tests without documentation or a tracking plan
are constitution violations—either implement the test properly, document why it cannot be automated, or remove it.

**Nextest Configuration Requirements**: ALL new integration tests MUST be configured in `.config/nextest.toml` with
appropriate test groups for resource isolation and parallelization. When adding a new test binary or test suite:
1. Identify resource requirements (Docker exclusive, Docker shared, filesystem heavy, network, long-running)
2. Add override rules to all profiles (default, dev-fast, full, ci) with appropriate test-group assignment
3. Use the most permissive test group that ensures correctness (prefer `docker-shared` over `docker-exclusive` when
   tests can safely share the Docker daemon; prefer parallel execution over serial when no state conflicts exist)
4. Verify tests pass with `make test-nextest` before submitting PR

Test groups available (defined in `.config/nextest.toml`):
- `docker-exclusive` (max-threads=1): Tests requiring exclusive Docker daemon access or shared container state
- `docker-shared` (max-threads=4): Tests using Docker but can share daemon safely
- `fs-heavy` (max-threads=4): Significant filesystem operations
- `long-running` (max-threads=1): Heavy end-to-end or large context builds
- `smoke` (max-threads=1): High-level integration tests
- `parity` (max-threads=1): Behavior comparison with upstream CLI

**Test Parallelization Strategy**: Optimize test execution by segmenting tests into appropriate binaries and
configuring them for maximum safe parallelism. When multiple tests share the same fixture or container configuration:
1. Evaluate if tests can run in parallel safely (no state conflicts, no resource contention)
2. If parallel execution causes race conditions (e.g., same container name), assign to `docker-exclusive` group
3. If tests only read from Docker or use unique container names, assign to `docker-shared` group
4. Consider splitting test binaries by resource type (e.g., `integration_docker_*` vs `integration_fs_*`) for finer
   control over parallelization
5. Document parallelization constraints in test module comments when non-obvious

Goal: Maximize test throughput while maintaining determinism and avoiding flaky tests.

### VII. Subcommand Consistency & Shared Abstractions
All CLI subcommands (existing and new) MUST share canonical helpers for any behavior that appears in
multiple commands. Terminal sizing, configuration/override/secrets resolution, container targeting,
remote environment merging, compose option wiring, and environment probing/user selection MUST NOT be
hand‑implemented per subcommand—these flows belong in shared modules consumed everywhere.

**Shared Helper Requirements**:
- Maintain a living backlog of cross-subcommand alignment tasks (terminal dimensions helper, config loader,
  container selector integration, remote env parsing, compose env-file threading, env probe reconciliation).
  The backlog is binding; resolve items before extending `up`, `exec`, or introducing a new subcommand in
  the same area.
- When overlapping functionality exists (e.g., `--id-label`, `--terminal-columns/rows`, config overrides,
  `--remote-env`, compose env files, env probe defaults), teams MUST either reuse the shared helper or
  justify in writing why divergence is unavoidable. Implementing bespoke parsing or behavior is a
  constitution violation.
- New subcommands or CLI flags MUST evaluate whether they hook into existing helpers. If a helper is
  missing, add it once, record the debt in the backlog, and reuse it everywhere before shipping.

**Drift Remediation**: When inconsistencies are discovered, record them in the shared backlog immediately
and remediate before unrelated feature work. Reordering or deleting backlog items requires the same
approval rigor as modifying this constitution.

**Avoid Redundant Operations**: When shared code performs an expensive operation (e.g., `canonicalize()`, network
fetch, container inspection), subcommand-specific code MUST NOT repeat that operation. Trust the result from the
shared layer. Redundant operations waste resources and introduce race conditions where filesystem/network state
may change between calls. Document in shared code comments which invariants are guaranteed (e.g., "workspace_folder
is already canonicalized").

**Builder Pattern for Configuration Objects**: When constructing complex configuration or output objects
that have optional enrichment (e.g., `EnrichedMergedConfiguration`), use builder-style methods:
- Base construction: `Type::from_base(base_data)`
- Optional enrichment: `.with_feature_metadata(...)`, `.with_labels(...)`, etc.
- This pattern allows callers to add only available data without complex conditional logic

**Design Decision Documentation**: When implementation choices may be questioned by reviewers (e.g., using
`from_config_entry()` instead of `from_resolved()`, placeholder TODO comments), document the rationale in
`specs/{feature}/research.md` with numbered decisions. Reference these decisions in code comments to help
reviewers understand intentional trade-offs versus oversights.

### VIII. Executable & Self‑Verifying Examples
Examples are executable contracts, not documentation suggestions. Every example directory under `examples/`
MUST contain an `exec.sh` that:
- Executes every README-documented path in a single run (no hidden env toggles), with clear echo banners per
  scenario and inline comments mapping to README sections.
- Cleans up all resources it creates (containers, images, volumes, temp files) after each scenario so reruns
  start clean and CI remains deterministic.
- Avoids interactive prompts; pins images/refs; uses non-networked fixtures unless the README explicitly
  documents required connectivity.
- Stays in lockstep with the README; any README change requires the corresponding `exec.sh` change and vice
  versa.

Subcommand-level aggregators (e.g., `examples/up/exec.sh`) MUST invoke each child example script serially with
scenario logging and must fail fast when any child fails. Adding a new example requires updating both the README
table and the aggregator to include the new `exec.sh`.

## Additional Constraints & Security

- Do not execute arbitrary shell from unvalidated input; surface destructive operations (e.g., container removal,
  volume pruning) behind explicit flags.
- Avoid leaking secrets in logs; maintain a redaction layer (if disabled explicitly, warn users).
- Tests MUST be deterministic and hermetic (no network); gate true integration behind CI‑only markers when needed.
- Prefer minimal, pinned dependencies; justify additions and keep the dependency set lean.
- Maintain an authoritative cross-subcommand alignment log. When implementing CLI work that overlaps
  existing functionality, consult and update that log so engineering tasks track the shared-helper
  obligations codified in Principle VII.
- Examples under `examples/` and fixtures under `fixtures/` are living documentation; keep READMEs, `exec.sh`
  scripts, and aggregator scripts in sync with user-facing flags, schemas, and outputs. Each `exec.sh` must
  leave the workspace clean after completion.

## Development Workflow & Quality Gates

- Small, reviewable changes; avoid large refactors unless explicitly requested. Keep the binary crate focused on the
  CLI entrypoint and orchestration; extract shared logic into core crates as needed.
- For each new module: add unit tests (pure logic) and integration tests when crossing process/runtime boundaries.
- Maintain smoke tests under `crates/deacon/tests/` covering: read‑configuration, build, up/exec, doctor.
- Doctests MUST compile and run; add missing trait imports, `Default` impls, or public visibility as needed.
- Examples under `examples/` and fixtures under `fixtures/` are living documentation; update them when user‑facing
  flags, schemas, or outputs change; keep `examples/README.md` curated and aligned with spec terminology.
- Use context7 MCP server for retrieving up-to-date documentation for libraries and packages.
- Use github MCP server for interacting with GitHub repositories, managing issues, pull requests, and code searches.

### Code Search & Refactoring with ast-grep

Use ast-grep (command: `sg`) for searching and rewriting code instead of `find`, `grep`, or regex-based tools.
ast-grep operates on Abstract Syntax Trees (AST), enabling precise pattern matching that respects language syntax.

**When to use ast-grep**:
- Searching for specific code patterns (function calls, struct definitions, trait implementations)
- Refactoring code at scale (renaming, restructuring, migrating APIs)
- Finding usages that regex would miss or over-match (e.g., distinguishing method calls from field access)
- Enforcing code conventions or detecting anti-patterns

**Basic usage**:
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

**Pattern syntax**:
- `$NAME` - Single metavariable (matches one AST node)
- `$$$ARGS` - Variadic metavariable (matches zero or more nodes)
- Patterns match AST structure, not text—whitespace and formatting are irrelevant

**Best practices**:
- Always specify `--lang rust` for Rust codebases
- Test patterns with search before applying rewrites
- Use `--interactive` for selective rewrites
- Prefer ast-grep over regex for any structural code transformation
- For complex refactors, write YAML rules in `sgconfig.yml`
- Observability: prefer structured fields over string concatenation; ensure spans cover multi‑step workflows.

### Pre-Implementation Validation Checklist

Before implementing a new subcommand or major feature, complete this checklist and document answers in your plan or PR:

1. **Spec Review**: Have you read the complete spec section (SPEC.md, data-model.md, contracts/)?
2. **Data Model Alignment**: Do your structs match spec-defined shapes exactly (map vs vec, field names, null handling)?
3. **Algorithm Alignment**: Have you identified all spec-defined algorithms (resolution, derivation, selection) and planned to implement them precisely?
4. **Input Validation**: Have you identified which inputs are valid/supported and where filtering must occur?
5. **Configuration Resolution**: Does your command use the full resolution path (extends, overrides, substitution) if it reads config?
6. **Output Contracts**: Have you verified JSON schema, ordering requirements, and exit code contracts?
7. **Testing Coverage**: Have you listed all spec-mandated tests and planned to implement them?
8. **Infrastructure Reuse**: Have you identified which existing helpers/loaders/traits you must use (vs reimplementing)?
9. **Nextest Configuration**: Have you planned which test group each new integration test will use and verified no conflicts?

This checklist prevents spec drift and reduces rework. Document deviations with explicit justification.

### Agentic Fast Loop Mode (local‑only)

- Use `make test-nextest-fast` for rapid iterations; it avoids Docker‑heavy suites and long‑running integration tests.
- Recommended cadence: run `make test-nextest-fast` every few iterations; run `make test-nextest` before commits/PRs.
- For targeted testing based on change type:
  - Parsing/validation changes → `make test-nextest-unit`
  - Docker lifecycle changes → `make test-nextest-docker` or `make test-nextest-smoke`
  - Long-running integration → `make test-nextest-long-running` (run periodically, not every iteration)
- This preserves the "keep build green" principle while reducing iteration time.

### Fixture and Example Hygiene

- **Reproducibility**: Pin all external images and dependencies to specific versions (e.g., `alpine:3.18` not
  `alpine:latest`). This prevents non-deterministic failures and ensures examples remain stable over time.
- **Schema Currency**: Keep fixture schemas current with spec requirements (e.g., Docker Compose 3.9+ for profiles).
- **Test Realism**: Integration test fixtures MUST include all flags and fields the test name implies (e.g., a test
  named `ignore_host_requirements` must actually set `ignore_host_requirements: true`).

## Governance

- This constitution supersedes other practice docs where conflicts arise for CLI behavior, quality gates, and
  engineering discipline.
- Amendments require a PR with: change rationale, mapping to `docs/CLI-SPEC.md` sections, risk assessment, and a
  version bump per rules below.
- Versioning of this document uses Semantic Versioning:
  - MAJOR: backward‑incompatible governance or principle removals/redefinitions
  - MINOR: new principles/sections or materially expanded guidance
  - PATCH: clarifications, wording, typo fixes, non‑semantic refinements
- Compliance Review: All PRs MUST include a quick constitution compliance check (in PR body or checklist). Reviewers
  SHALL block merges on violations of Principles I–VIII or on missing updates to tests/examples.

**Version**: 1.12.0 | **Ratified**: 2025-10-31 | **Last Amended**: 2025-11-28
