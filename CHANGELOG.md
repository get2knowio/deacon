# Changelog

All notable changes to **Deacon** are recorded here. Format based on
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/); this project follows
[Semantic Versioning](https://semver.org/spec/v2.0.0.html).

The 1.0 release is being assembled across a series of PRs tracked in
[`docs/ROADMAP_TO_1.0.md`](docs/ROADMAP_TO_1.0.md).

## [Unreleased]

### Removed

- Removed out-of-scope feature-authoring (OCI publish/upload) code from
  `deacon-core`. Per the consumer-only constitution (§II), the publish/upload
  implementation was unreachable from any binary (every caller was a test) and
  has been deleted: `publish_feature`, `publish_template`,
  `publish_feature_multi_tag`, `publish_collection_metadata`, the
  `upload_blob`/`upload_manifest` helpers, the `PublishResult` type, the
  `OciPublish*` progress events, the `registry.publish` span, and the
  publish-only `HttpClient::put_with_headers` / `post_with_headers` trait
  methods (with their impls and mocks). The consumer-side fetch/pull/install
  path is unchanged.

### Added
- `cargo-deny` security gate in CI (`.github/workflows/ci.yml` `security` job),
  running on PR/push plus a daily 07:00 UTC schedule. Policy defined in
  `deny.toml`.
- `CHANGELOG.md` (this file) and `docs/ROADMAP_TO_1.0.md` (1.0 readiness
  report and roadmap).
- One-time WARN when `--runtime podman` (or `DEACON_RUNTIME=podman`) is
  selected, surfacing Podman's experimental status without spamming.
- Smoke tests covering `run-user-commands --container-id` and
  `run-user-commands --id-label` paths.
- **Lockfile graduation (PR-4):** `--no-lockfile`, `--frozen-lockfile`, and
  legacy `--experimental-lockfile <PATH>` / `--experimental-frozen-lockfile`
  (hidden aliases, WARN-on-use). After `up` resolves features the writer
  emits a sorted, trailing-newline-terminated `devcontainer-lock.json`
  byte-identical to upstream `devcontainers/cli`'s `writeLockfile`.
  `--frozen-lockfile` byte-compares the freshly-built lockfile against the
  on-disk file and fails with the upstream-aligned messages
  `"Lockfile does not exist."` / `"Lockfile does not match."`. Read-only
  workspaces (EROFS/EACCES) downgrade to a WARN. Schema parity fix: the
  `dependsOn` field now serializes as camelCase.
- **`set-up` subcommand** (`deacon set-up --container-id <id>`): convert an
  already-running container into a DevContainer by applying configuration +
  image metadata, executing lifecycle hooks (`onCreate` → `updateContent` →
  `postCreate` → `postStart` → `postAttach`), optionally installing
  dotfiles, and emitting a single-line JSON result on stdout per
  [`docs/subcommand-specs/set-up/SPEC.md`](docs/subcommand-specs/set-up/SPEC.md).
  Flags: `--config`, `--skip-post-create`, `--skip-non-blocking-commands`,
  `--remote-env`, `--dotfiles-repository` / `--dotfiles-install-command` /
  `--dotfiles-target-path`, `--include-configuration`,
  `--include-merged-configuration`, `--container-data-folder`,
  `--container-system-data-folder`.
- **`set-up` /etc patches:** marker-guarded, append-only patches to
  `/etc/environment` (PAM-escaped `KEY="VALUE"` block) and `/etc/profile`
  (re-exports PATH from `/etc/environment` for login shells), wrapped in
  `# >>> deacon set-up >>>` ... `# <<< deacon set-up <<<` delimiters. Marker
  files at `{container_system_data_folder}/.patchEtcEnvironmentMarker` and
  `.patchEtcProfileMarker` (default `/var/devcontainer/`). Best-effort:
  failures emit a WARN and proceed (spec §9).
- **`upgrade` subcommand** (`deacon upgrade`): regenerates
  `devcontainer-lock.json` from the currently resolved Feature set per
  [`docs/subcommand-specs/upgrade/SPEC.md`](docs/subcommand-specs/upgrade/SPEC.md).
  Resolves every Feature against the OCI registry, builds a fresh
  `Lockfile`, and either writes it via `write_lockfile(force_init = true)`
  or prints the canonical JSON to stdout with `--dry-run`. Flags:
  `--dry-run`, `--docker-path`, `--docker-compose-path`. Hidden Dependabot
  flags `-f/--feature` + `-v/--target-version` rewrite the matching feature
  key in `devcontainer.json` in place (text-level surgical edit preserving
  comments/whitespace) before the regenerate phase. Argument validation
  mirrors upstream exactly (`"The '--target-version' and '--feature' flag
  must be used together."`, `"Invalid version 'X'.  Must be in the form
  of 'x', 'x.y', or 'x.y.z'"`).
- **Features-during-`build`** for Dockerfile-based configs: `deacon build`
  no longer fails fast when features are declared. After the user's
  `docker build`, a second BuildKit pass layers features on top of the
  resulting image (reuses `up`'s `build_image_with_features` helper) and
  writes the lockfile next to `devcontainer.json` via
  `write_lockfile(force_init = true)`. Read-only workspaces (EROFS/EACCES)
  downgrade to a WARN. Compose and image-reference builds still fail fast
  with features — their integration patterns differ enough to warrant
  separate follow-ups. Cache invalidation: when features are present the
  cache check is skipped (the current `config_hash` does not yet fold in
  feature digests).

### Changed
- **Spec parity:** `devcontainer.metadata` image label is now always emitted as
  a JSON array of partial-config entries, matching
  [devcontainers/cli#1199](https://github.com/devcontainers/cli/pull/1199)
  (v0.86.0). Readers tolerate both the new array form and the legacy single-
  object form for backwards compatibility with images built by older Deacon
  versions.
- `run-user-commands` now honors `--container-id` and `--id-label` (precedence:
  `--container-id` > `--id-label` > workspace-based discovery). Previously both
  flags were accepted but silently ignored (#269).
- Podman runtime status documented as **experimental in 1.0** (trait-level
  support is complete; rootless-Podman parity items and dedicated test coverage
  targeted for 1.1, tracked in
  [#30](https://github.com/get2knowio/deacon/issues/30)). README, `CLAUDE.md`,
  and `--help` text updated.
- Minimum Supported Rust Version (MSRV) bumped from 1.70 to 1.82 (no
  language-feature gates introduced; unblocks modern dependencies under the
  MSRV-aware resolver default).

### Fixed
- `read-configuration` now correctly handles `devcontainer.metadata` labels
  emitted in upstream array form (previously the reader expected a single
  object and would mis-merge).
- `read-configuration` cleanup: removed stale `#[allow(dead_code)]`
  annotations and obsolete `TODO(#268)` comments on the already-implemented
  `--container-id` / `--id-label` fields.
- `docker_retry` unit tests use `/usr/bin/true` instead of `/bin/true` so the
  `Test (MVP fast) (macos)` CI job passes (`/bin/true` is absent on the macOS
  GHA runner images).

### Notes
The 1.0 audit reclassified several roadmap items as non-issues after
exploration. Recording here as an audit trail:

- **`serde_yaml` migration** — not applicable. The workspace does not depend
  on `serde_yaml` (verified via `cargo tree -i serde_yaml`).
- **`async-tar` / `tokio-tar` "TARmageddon" CVE-2025-62518** — not vulnerable.
  Deacon uses the synchronous `tar` crate (0.4.x), which is not affected.
- **`json5` migration to `jsonc-parser`** — deferred. `json5 1.3.x` is the
  maintained fork (the unmaintained crate is the 0.4 line); `cargo deny check
  advisories` confirms RUSTSEC-2025-0120 does not apply. `jsonc-parser` is not
  serde-compatible and would add ~100 lines of AST translation for no
  functional gain.
- **Issue #1** ("features installed in running container instead of during
  image build") — features are already installed at build time via Dockerfile
  composition and BuildKit `RUN --mount=type=bind` (see
  `crates/core/src/dockerfile_generator.rs` and the bead 14a/14b commits
  `f4997b9` / `65af5bb`). Issue can be verified-and-closed; no implementation
  work needed.

## [Pre-history]

Earlier releases were not formally tracked here. Notable inflection points:

- 2026-05: bead 14b — features installed during build for Compose `build:`
  service shapes (commit `65af5bb`).
- 2026-05: feat — retry transient Docker build failures (commit `f094fdb`).
- 2026-04: feat(exec) — three-pass variable substitution with `containerEnv`
  (commit `0461268`).
- 2025-12 → 2026-05: progressive build-out of the consumer-side CLI surface
  (`up`, `down`, `exec`, `build`, `read-configuration`, `run-user-commands`,
  `templates`, `outdated`, `config`) per `.specify/memory/constitution.md`.

[Unreleased]: https://github.com/get2knowio/deacon/compare/v1.0.0-rc.1...HEAD
