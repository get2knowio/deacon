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
  - `features`
    - `test <path> [--json]`
    - `package <path> --output <DIR> [--json]`
    - `publish <path> --registry <URL> [--dry-run] [--json]`
    - `info <mode> <feature>` (not yet implemented)
  - `templates`
    - `apply <template>` (not yet implemented)
    - `publish <path> --registry <URL> [--dry-run]`
    - `metadata <path>`
    - `generate-docs <path> --output <DIR>`
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
  - ✅ **COMPLETE**: Additional build flags: cache-from/to, ssh, secrets, buildkit toggles are all implemented and tested.
  - Output format/verbosity flags — verify naming.
- `exec`
  - TTY and stdin semantics — verify defaults and `--no-tty` naming.
  - Working directory derivation parity — confirm.
- `read-configuration`
  - Options for substitution-only, raw vs merged vs effective — verify.
- `features`
  - `info` modes and output shape — confirm.
  - `publish` auth and registry flags (scope, token) — confirm.
- `templates`
  - `apply` flags (target dir, options, force) — confirm.
  - `publish` flags/auth — confirm.
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
  - Features: `info` (metadata resolution for local/registry features)
  - Templates: `apply` (project scaffolding with options)
  - Run-user-commands (executes configured commands; honor non-blocking semantics)
  - Set-up (convert existing container to dev container)
  - Outdated (report current/available versions)
  - Upgrade (lockfile upgrades)

2) Flags and UX parity
  - Perform a literal diff of help text; add aliases/flags to match TS CLI (no behavioral change) with deprecation messages for non-preferred forms.
  - Standardize output and exit codes where tests expect legacy messages (already partially done for NotFound).
  - Build: accept optional positional `[path]` mapped to workspace folder; preserve existing flags.

3) OCI publishing (features/templates)
  - Implement full push to OCI (auth, tags, digests) in `core::oci` with retries/backoff; wire `features publish` and `templates publish` (non-dry-run).

4) Host requirements
  - Implement real storage checks for workspace path with platform-aware logic; extend doctor output accordingly.

5) Lifecycle and compose polish
  - Honor non-blocking `postStart`/`postAttach` semantics more closely; improve logging/streaming and timeouts.
  - Compose: ensure multi-service port events and `exec` targeting across services are robust.

6) Docker/Build options
  - ✅ **COMPLETE**: Advanced build flags (cache-from/to, ssh, secrets) and BuildKit controls are implemented, tested, and validated.

7) Security and redaction
  - Cryptographic hashing for secret registry; ensure redaction is applied to all outputs (progress, audit, PORT_EVENT) and respects `--no-redact`.

**Next Steps (actionable)**
- Provide `devcontainer --help` output (global + each subcommand) to convert the checklist into a definitive parity matrix and exact patches.
- If preferred, I can draft patches now for: features info, templates apply (scaffold), and real storage checks; then iterate on flags after we review TS help text.
