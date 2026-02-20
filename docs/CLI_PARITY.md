**Goal**
- Provide a clear, verifiable map of the current Rust CLI surface to the TypeScript devcontainers/cli, plus a concrete, prioritized roadmap to close gaps. This document is derived from the current source (`crates/deacon/src/cli.rs`) and internal spec docs. Where exact TS CLI flags differ, items are marked “verify against TS CLI help”.

**Current CLI Surface (from cli.rs)**
- Global
  - `--log-format {text|json}`
  - `--log-level {error|warn|info|debug|trace}`
  - `--workspace-folder <PATH>`
  - `--config <PATH>`
  - `--override-config <PATH>`
  - `--secrets-file <PATH>` (repeatable)
  - `--no-redact`
  - `--progress {auto|json|none}`
  - `--progress-file <PATH>`
  - `--plugin <NAME>` (cfg feature)
  - `--force-tty-if-json` (forces PTY allocation for lifecycle exec when JSON logging is active; env: `DEACON_FORCE_TTY_IF_JSON`)

- Subcommands
  - `up`
    - `--remove-existing-container`
    - `--skip-post-create`
    - `--skip-non-blocking-commands`
    - `--ports-events`
    - `--shutdown`
    - `--additional-features <JSON>`
    - `--prefer-cli-features`
    - `--feature-install-order <CSV>`
    - `--ignore-host-requirements`
  - `build`
    - `--no-cache`
    - `--platform <STRING>`
    - `--build-arg <KEY=VALUE>` (repeatable)
    - `--force`
    - `--output-format {text|json}`
    - `--cache-from <CACHE_FROM>` (repeatable)
    - `--cache-to <CACHE_TO>` (repeatable)
    - `--buildkit {auto|never}`
    - `--secret <SECRET>` (repeatable)
    - `--ssh <SSH>` (repeatable)
    - `--scan-image`
    - `--fail-on-scan`
    - `--additional-features <JSON>`
    - `--prefer-cli-features`
    - `--feature-install-order <CSV>`
    - `--ignore-host-requirements`
  - `exec`
    - `--user <USER>`
    - `--no-tty`
    - `--env <KEY=VALUE>` (repeatable)
    - positional: `command ...`
  - `read-configuration`
    - `--include-merged-configuration`
  - `templates`
    - `apply <template>` (not yet implemented)
    - `pull <template>` (OCI template pull)
  - `run-user-commands <commands...>` (not yet implemented)
  - `down`
    - `--remove`
  - `doctor`
    - `--json`
    - `--bundle <PATH>`

**Parity Checklist (needs verification against TS CLI help)**
- Global
  - Aliases and short flags (e.g., `-v`, `-q`, `-o`) — verify.
  - Progress options and default behaviors — verify “auto” semantics match.
  - Secrets and redaction controls — verify naming and precedence.
  - Plugins flag availability — verify parity if TS CLI exposes plugins.
- `up`
  - Identity/label flags (e.g., id-label) — verify.
  - Dotfiles flags (repo, install command, target path) — verify TS flags.
  - Shutdown behaviors and defaults — confirm parity.
  - Port event behavior and prefixes — confirm exact output contract.
- `build`
  - ✅ **COMPLETE**: Build subcommand parity closure (spec 006-build-subcommand)
    - Multi-tag support: `--image-name` can be specified multiple times
    - Custom labels: `--label` can be specified multiple times
    - Registry push: `--push` with BuildKit support
    - Artifact export: `--output` for OCI archives (mutually exclusive with `--push`)
    - BuildKit gating: Clear error messages when BuildKit-only flags are used without BuildKit
    - Compose mode: Targets configured service with validation for unsupported flags
    - Image reference mode: Builds from base image with feature application and metadata
    - JSON output contract: Success payloads with `imageName`, `pushed`, `exportPath` fields
    - Error contract: Structured errors with `message` and `description` fields
  - Output format/verbosity flags — verify naming.
- `exec`
  - TTY and stdin semantics — verify defaults and `--no-tty` naming.
  - Working directory derivation parity — confirm.
- `read-configuration`
  - Options for substitution-only, raw vs merged vs effective — verify.
- `templates`
  - `apply` flags (target dir, options, force) — confirm.
- `down`
  - Additional flags (e.g., compose-only, timeouts) — verify.
- `doctor`
  - Output modes and bundle content control — verify.

Note: This section is refined using the provided top‑level `devcontainer --help` output below. For full parity we still need per‑subcommand help texts.

Provided `devcontainer --help` summary:
- Commands:
  - up
  - set-up
  - build [path]
  - run-user-commands
  - read-configuration
  - outdated
  - upgrade
  - features
  - templates
  - exec <cmd> [args..]
- Global options (top-level): `--help`, `--version`

Observed differences vs. current Rust CLI:
- Missing subcommands in Rust CLI:
  - `set-up` (TS: “Set up an existing container as a dev container”)
  - `outdated` (TS: “Show current and available versions”)
  - `upgrade` (TS: “Upgrade lockfile”)
- Build positional path:
  - TS shows `build [path]`; Rust CLI takes config/workspace via `--config`/`--workspace-folder`. Action: accept an optional positional path for `build` that maps to workspace folder (or config discovery root) to match UX.
- Exec syntax:
  - TS shows `exec <cmd> [args..]`; Rust already accepts `command: Vec<String>` which matches. Ensure help text mirrors TS form.
- Top-level options:
  - TS top-level help only shows `--help`/`--version`; TS typically exposes many flags under subcommands rather than globally. Our Rust CLI has global flags for log/progress/secrets. Keep these (useful), but add aliases/compatibility only if TS exposes them globally; otherwise document the divergence.

Immediate parity tasks (no behavioral risk):
- Add stub subcommands returning NotImplemented with exit code 2:
  - `set-up`
  - `outdated`
  - `upgrade`
- Add optional positional `[path]` to `build` (fallback to discovery when omitted).
- Update `exec` help to display `<cmd> [args..]` form.

**Concrete Roadmap (prioritized)**
1) Subcommands to implement
  - Templates: `apply` (project scaffolding with options)
  - Run-user-commands (executes configured commands; honor non-blocking semantics)
  - Set-up (convert existing container to dev container)
  - Outdated (report current/available versions)
  - Upgrade (lockfile upgrades)

2) Flags and UX parity
  - Perform a literal diff of help text; add aliases/flags to match TS CLI (no behavioral change) with deprecation messages for non-preferred forms.
  - Standardize output and exit codes where tests expect legacy messages (already partially done for NotFound).
  - Build: accept optional positional `[path]` mapped to workspace folder; preserve existing flags.

3) Host requirements
  - Implement real storage checks for workspace path with platform-aware logic; extend doctor output accordingly.

4) Lifecycle and compose polish
  - Honor non-blocking `postStart`/`postAttach` semantics more closely; improve logging/streaming and timeouts.
  - Compose: ensure multi-service port events and `exec` targeting across services are robust.

5) Docker/Build options
  - ✅ **COMPLETE**: Build subcommand parity fully closed (spec 006-build-subcommand)
    - All missing flags implemented: `--image-name` (repeatable), `--label` (repeatable), `--push`, `--output`
    - BuildKit gating logic enforces prerequisites for BuildKit-only flags
    - Compose mode validates and rejects unsupported flags (`--platform`, `--push`, `--output`, `--cache-to`)
    - Image reference builds work with feature application, tagging, and metadata injection
    - JSON output contracts implemented for success and error payloads
    - Multi-tag support with array output when multiple `--image-name` flags provided
    - Push and export workflows fully functional with appropriate status reporting
    - All acceptance tests passing; examples and documentation updated
    - Delivered: Phases 1-5 complete (Setup, Foundational, US1-US3); Phase 6 (Polish) complete

6) Security and redaction
  - Cryptographic hashing for secret registry; ensure redaction is applied to all outputs (progress, audit, PORT_EVENT) and respects `--no-redact`.

**Release Notes: Build Subcommand Parity Closure (spec 006-build-subcommand)**

Completed: 2025-11-15

### Summary
Closed all remaining gaps in the `deacon build` subcommand to achieve full parity with the reference TypeScript CLI. The implementation delivers:

### New Features
- **Multi-tag support**: Build and apply multiple tags to a single image using repeatable `--image-name` flags
- **Custom labels**: Add metadata labels to images via repeatable `--label` flags
- **Registry push**: Push built images directly to registries with `--push` (requires BuildKit)
- **Artifact export**: Export images to OCI archives or other formats with `--output` (requires BuildKit, mutually exclusive with `--push`)
- **Compose service targeting**: Build specific services from Docker Compose configurations
- **Image reference builds**: Build from base images specified in `devcontainer.json` (image property)

### Quality & Validation
- **BuildKit gating**: Clear error messages when BuildKit-only flags are used without BuildKit availability
- **Compose restrictions**: Validation prevents use of unsupported flags in Compose mode (`--platform`, `--push`, `--output`, `--cache-to`)
- **JSON output contract**: Success payloads include `imageName` (string or array), optional `pushed` and `exportPath` fields
- **Error contract**: Structured errors with `message` and optional `description` fields

### Testing & Documentation
- **Comprehensive test coverage**: All user stories validated through integration tests and smoke tests
- **Examples**: Added `compose-service-target` and `image-reference` examples with READMEs
- **Documentation**: Updated `docs/subcommand-specs/build/SPEC.md` with implementation status and output schemas
- **Examples guide**: Enhanced `examples/README.md` with new build scenarios

### Implementation Phases
All planned phases completed:
- Phase 1: Setup (documentation alignment)
- Phase 2: Foundational (domain types and validation helpers)
- Phase 3: User Story 1 - Tagged builds with metadata and labels (P1 MVP)
- Phase 4: User Story 2 - Registry push and artifact export (P2)
- Phase 5: User Story 3 - Multi-source configuration coverage (P3)
- Phase 6: Polish and documentation

### Breaking Changes
None - all changes are additive.

### Migration Notes
None required - new flags are optional and backward compatible.

---

**Next Steps (actionable)**
- Provide `devcontainer --help` output (global + each subcommand) to convert the checklist into a definitive parity matrix and exact patches.
- If preferred, I can draft patches now for: templates apply (scaffold) and real storage checks; then iterate on flags after we review TS help text.
