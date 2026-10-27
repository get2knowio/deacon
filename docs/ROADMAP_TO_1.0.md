# Deacon → 1.0 Readiness Report

**Prepared:** 2026-05-25
**Reconciled against the code:** 2026-10-27 (`main` at `8e3895c`). Items that have since landed are struck through or marked **done** in place; the rest of the body is the May 2026 analysis. For behavior-level status the authority is [`parity/SPEC_STATUS.md`](../parity/SPEC_STATUS.md), not this document.
**Scope:** Consumer-side devcontainer CLI in Rust. Feature/template *authoring* remains permanently out of scope (per Constitution v1.13.0 §2).
**Inputs:** Codebase audit, upstream `@devcontainers/cli` (HEAD ≈ v0.87.0, May 2026), `containers.dev` spec (HEAD `c95ffee`, May 2026), Rust ecosystem scan (May 2026).

---

## TL;DR

Deacon is in much better shape than the issue tracker suggests. The **spec itself has had zero substantive changes since the Aug 2025 pin (`113500f4`)** — everything new is in the reference CLI's surface, the Rust ecosystem, or our own deferred work. The path to 1.0 is bounded and visible:

- ~~**One real spec-parity bug** (issue #1: features installed in running container instead of during image build).~~ **Done**: issue #1 is closed, and Features install during the image build (see §3.E).
- **CLI-surface gaps added upstream since our pin** that consumers will expect. Most have landed since (§3.B). Still open: syntax-directive injection / `--omit-syntax-directive` (#732), BuildKit inline-cache hints, and buildx platform envvars.
- ~~**Two unmaintained dependencies** (`serde_yaml`, `json5`) and one critical CVE we need to audit (`async-tar`/`tokio-tar` TARmageddon).~~ **Done**: the dependency graph has no YAML crate and no `json5`, and `jsonc-parser` parses `devcontainer.json` (see §4).
- ~~**Two specced-but-not-implemented subcommands** (`set-up`, `upgrade`) the reference CLI exposes as consumer-side.~~ **Done**: both ship (`commands/set_up.rs`, `commands/upgrade.rs`) with parity scenarios (`parity/cases/set-up.json`, `parity/cases/upgrade.json`); see §3.A.
- ~~**Distribution story missing**: no `CHANGELOG.md`, no release artifacts (`cargo-dist`), no security gates (`cargo-audit`/`cargo-deny`) in CI.~~ **Done**: `CHANGELOG.md` exists. The tag-triggered `release.yml` publishes 8 targets with SHA256s, SPDX SBOMs and build provenance, built with `cross` rather than `cargo-dist`. CI runs `cargo deny check`, which includes advisories.
- **Examples hygiene drift**: 24 of 115 example directories (`examples/<area>/<name>/`) still have no `exec.sh` (Constitution §9); see §5.

**Estimate:** ~6–10 weeks of focused work to a confident 1.0 tag, if no major scope additions land.

---

## 1. Codebase Snapshot

### Implemented subcommands (12)
| Command | Status | Notes |
|---|---|---|
| `up` | Implemented | Lockfile flags (`--no-lockfile`/`--frozen-lockfile`) have landed. Per-behavior status is in `parity/SPEC_STATUS.md`; the `up/GAP.md` this row used to cite no longer exists |
| `build` | Implemented | Features install for Dockerfile, image-reference and Compose configs. The `devcontainer.metadata` label carries one entry per Feature plus the config pick (§3.B.2). Compose configs reject `--push`/`--output`/`--cache-to`/`--platform`. `feature_set_digest` is still a placeholder (`build/mod.rs:1630`) |
| `exec` | Implemented | Recent signal-exit and home-fallback fixes landed |
| `down` | Implemented | Not in upstream CLI — Deacon-original |
| `read-configuration` | Implemented | Container-based reading done (`--container-id`/`--id-label`; reads `devcontainer.metadata` off the container, issue #268) |
| `run-user-commands` | Implemented | Container selection is wired: `--container-id` / `--id-label`, with the configuration read from the container's `devcontainer.metadata` label when no `--config` is given |
| `config substitute` | Implemented | Deacon-original utility surface. `substitute` is the only `config` subcommand, and there are no `config merge-*` subcommands: merging is done with the `--override-config` / `--merge-config` flags (#286) |
| `templates apply` / `pull` | Implemented | `--omit-paths` missing |
| `outdated` | Implemented | Spec at "Phase 2"; works against feature lockfile concept |
| `doctor` | Implemented | Not present in reference CLI — Deacon-original |
| `set-up` | Implemented | Adopts an existing container (`--container-id`) as a dev container. 6 parity cases in `parity/cases/set-up.json`; rows under `set-up` in `SPEC_STATUS.md` |
| `upgrade` | Implemented | Regenerates the lockfile (`--dry-run` prints it instead). 11 parity cases in `parity/cases/upgrade.json`; rows under `upgrade` in `SPEC_STATUS.md` |

### Not yet implemented
- Nothing on the consumer surface the reference CLI exposes. `set-up` and `upgrade`, which this section listed in May 2026, have both shipped.

### Core library shape
The `crates/core/src/` module set is comprehensive: `config`, `features`, `container_lifecycle`, `oci/*`, `compose`, `dotfiles`, `gpu`, `host_requirements`, `mount`, `secrets`, `redaction`, `variable`, `lockfile`, `state`, `workspace`, `cache/*`, `docker_retry`, `progress`, etc. The major abstractions called out in `CLAUDE.md` (`ContainerRuntime`, `HttpClient`, `ConfigLoader`, `FeatureInstaller`, `ContainerLifecycle`) are present and structured.

### Test coverage
- `crates/deacon/tests/` — 114 integration test files
- `crates/core/tests/` — 44 integration test files
- `set-up` is covered by in-module unit tests, Docker-gated tests (`integration_container_id_foreign.rs`, `integration_feature_lifecycle.rs`) and parity cases. `upgrade` is covered by in-module unit tests and parity cases.
- Gaps: `config substitute` has no CLI-level integration test and no parity scenario, only unit tests in `commands/config.rs` and the `examples/configuration/substitute` canary. The two span tests in `crates/core/tests/integration_spans.rs` that drive it are `#[ignore]`d with the stale reason "Uses non-existent 'config substitute' subcommand". `upgrade` has no dedicated `crates/deacon/tests/` file; its CLI-level coverage is the parity cases.
- `#[ignore]`d tests cluster around feature-gates T020–T029 (compose profiles, reconnect/secrets-file, postAttach/init).

### TODO/FIXME inventory
As of 2026-10-27, 19 markers remain in `crates/*/src` (57 in May 2026). Most are `#[allow(dead_code)]` scaffolding notes in `up/args.rs` and `up/result.rs`. The non-test TODOs worth tracking on the 1.0 punchlist:

| File | Note |
|---|---|
| `core/src/container.rs:1669` | Workspace-based container resolution (`TODO(#270)`) |
| ~~`read_configuration.rs:33`~~ | ~~Container-based config reading (issue #268)~~ — **done** (`--container-id`/`--id-label` + `devcontainer.metadata` read) |
| ~~`read_configuration.rs:467,507`~~ | ~~Feature metadata caching; `metadata.init` field~~ — markers gone |
| ~~`run_user_commands.rs:32,36,158`~~ | ~~Container selection wiring; collect lifecycle commands from extends/features~~ — **done**, markers gone |
| `build/mod.rs:1630` | `feature_set_digest` placeholder. The Feature-integration TODO is gone, since Features now install in `build` |
| `core/src/lifecycle.rs:1199` | Timeout enforcement on lifecycle commands |
| `core/src/compose.rs:451` | Feature-derived options in validation |

---

## 2. Spec Compliance (`containers.dev`)

**The headline:** between our pin `113500f4` (2025-08-01 — note `CLAUDE.md` records "October 2025"; the SHA is actually August) and spec HEAD `c95ffee` (May 2026), every merged commit is doc/typo/formatting. **No new fields, no new behaviors, no new contracts.** Our pin is current.

**Action:** ~~Fix the pin date or move the pin to spec HEAD.~~ Done: the pin moved to `c95ffeed` (2026-03-20); CLAUDE.md and the constitution carry the corrected date.

**Open proposals to watch (not 1.0 blockers):**
- [spec#22](https://github.com/devcontainers/spec/issues/22) `extends` finalization — we already support it.
- [spec#716](https://github.com/devcontainers/spec/issues/716) versioned, reusable configuration packages.
- [spec#324](https://github.com/devcontainers/spec/pull/324) `build.options` (open since 2024).

---

## 3. Reference CLI Parity Gaps

### A. Missing consumer-side subcommands
**Done.** Both have shipped (`crates/deacon/src/commands/set_up.rs`, `upgrade.rs`), and their conformance is recorded under the `set-up` and `upgrade` areas of `parity/SPEC_STATUS.md`.

1. ~~**`set-up`**~~ **done** — adopts an existing container (`--container-id` required) as a dev container, runs lifecycle + dotfiles, emits `{outcome, configuration?, mergedConfiguration?}`. Reference: `devContainersSpecCLI.ts:79` and `:354`.
2. ~~**`upgrade`**~~ **done** — upgrades the lockfile; graduated from experimental in v0.87.0 ([PR #1212](https://github.com/devcontainers/cli/pull/1212)). Reference: `devContainersSpecCLI.ts:85`.

### B. CLI-surface changes upstream made since our Oct 2025 pin
| # | Item | Severity | Source |
|---|---|---|---|
| B.1 | ~~**Lockfile graduation**: `--no-lockfile`, `--frozen-lockfile`, and *default-on* lockfile writes on `up`/`build` (`.devcontainer-lock.json`)~~ — **done** on `up` and `build` (lockfile rows under `build` and `up` in `SPEC_STATUS.md`; #556, #569, #571) | **High** — CI primitive | v0.87.0, [PR #1212](https://github.com/devcontainers/cli/pull/1212) |
| B.2 | ~~**`devcontainer.metadata` label is always a JSON array**, even for single-entry metadata. Writer-side contract change.~~ — **done**: on every shape `build` writes the same entry array `up` does, one entry per Feature plus the config pick (#436, #440) | **High** — image interop with VS Code/Zed/envbuilder | v0.86.0, [PR #1199](https://github.com/devcontainers/cli/pull/1199) |
| B.3 | ~~`--mount-git-worktree-common-dir` for worktrees with `--relative-paths`~~ — **done** (#664) | Medium | v0.81.0, [PR #1127](https://github.com/devcontainers/cli/pull/1127) |
| B.4 | ~~Default `--workspace-folder` to CWD when omitted~~ — **done** on every subcommand: `up` by issue #610 and `read-configuration` by issue #615, the last one out of step; `exec`, `build`, `down` and `run-user-commands` already did. On `read-configuration` the default applies only when no other selector is given — `--config` alone still anchors at the config file's directory and `--container-id`/`--id-label` alone still mean container-only mode | Medium — UX | v0.82.0, [PR #1104](https://github.com/devcontainers/cli/pull/1104) |
| B.5 | ~~Windows drive-letter lowercase normalization for id-labels (parity with VS Code)~~ — **done** (`label_path`, #682) | Low — Windows-only | v0.86.0 |
| B.6 | Skip injecting `# syntax=docker/dockerfile:1.4` when Docker Engine ≥ 23.0.0; hidden `--omit-syntax-directive` flag. Still open: deacon never injects the directive, which is recorded as an open nonconformance (#732) | Low | v0.80.3, [PR #1118](https://github.com/devcontainers/cli/pull/1118) |
| B.7 | `BUILDKIT_INLINE_CACHE=1` on Feature build path for cache reuse | Low — perf | v0.83.0, [PR #1135](https://github.com/devcontainers/cli/pull/1135) |
| B.8 | Buildx platform envvars inlined when resolving base image/user. Still open: `dockerfile_utils` accepts them, but no caller supplies them yet | Low | v0.85.0, [PR #1169](https://github.com/devcontainers/cli/pull/1169) |

### C. Long-standing reference flags worth a parity audit
As of 2026-10-27, `--help` lists every flag below except `--omit-paths`:
- ~~`--container-session-data-folder` (distinct from `--container-data-folder`; specifically for userEnvProbe caching)~~ — accepted (global flag).
- ~~`--stop-for-personalization` on `run-user-commands`~~ — accepted; `SPEC_STATUS.md` records it as matching the reference.
- `--omit-paths` on `templates apply` ([PR #868](https://github.com/devcontainers/cli/pull/868), 2024-08) — **still missing**.
- ~~`--secrets-file` (JSON of lifecycle-only secrets, never persisted to metadata) and `--omit-config-remote-env-from-metadata` (hidden)~~ — both accepted: `--secrets-file` globally, the other on `up`.
- ~~`--workspace-mount-consistency {consistent|cached|delegated}` (macOS bind-mount perf)~~ — accepted on `up`.
- ~~`--gpu-availability {all|detect|none}` — Deacon has a flag; verify default is `detect` to match~~ — verified: the default is `detect`.
- ~~`--update-remote-user-uid-default {never|on|off}` and `--default-user-env-probe {none|loginInteractiveShell|interactiveShell|loginShell}`~~ — both accepted; `--default-user-env-probe` defaults to `loginInteractiveShell`.

### D. Podman parity
**Decided: supported.** A required CI lane (`ci.yml`) runs the integration suite against rootless Podman, and deacon handles the items below behind `CliRuntime::is_podman()`. The remaining gaps are GPU passthrough and an unverified Windows + Podman combination (#239), tracked on #30; see the README's Known limitations. The May 2026 analysis follows.

The constitution listed Podman as "in development." Upstream made Podman a first-class peer over the last year:
- `label=disable` SELinux mount option instead of `:z` for rootless ([PR #1045](https://github.com/devcontainers/cli/pull/1045), v0.80.0).
- `--uidmap`/`--gidmap` must not conflict with `--userns` ([PRs #1005](https://github.com/devcontainers/cli/pull/1005), [#1018](https://github.com/devcontainers/cli/pull/1018), v0.77.0).
- Omit `--userns=keep-id` for root user.
- Rootless Docker support landed 2026-03.

Decide for 1.0: **Either** ship Podman as supported (and own the parity items above) **or** mark it explicitly experimental in `--help` and docs. The current "in development" middle ground is the worst of both.

### E. Open bug already filed
**Closed.** Issue #1 is resolved: Features install during the image build for Dockerfile, image-reference and Compose configurations, on both `up` and `build`. The original note follows.
- **Issue #1: Features installed in running container instead of during image build.** This is a spec-parity violation (Constitution §1) and a real correctness problem — features installed at runtime won't survive container rebuilds, won't appear in the image metadata label, and won't match VS Code/reference behavior. **Treat as a 1.0 blocker.**

---

## 4. Rust Ecosystem Actions

### Urgent (1.0 blockers)
**Status 2026-10-27: items 1–5 are done.** No YAML crate and no `json5` remain in `Cargo.lock`, and `jsonc-parser` 0.34 parses configuration. Archive handling uses `tar` 0.4.46; the only `tokio-tar` in the graph is `astral-tokio-tar` 0.6.2, pulled in through the dev-only `testcontainers`. The MSRV is 1.95 on Edition 2024. CI's `cargo deny check` job covers advisories, licenses and sources.

1. **`serde_yaml` is deprecated** (since March 2024). Used for compose file parsing. Migrate to **`serde_yaml_ng`** (drop-in fork by acatton; same API).
2. **`json5` crate is unmaintained** — RUSTSEC-2025-0120. Used in `deacon-core/Cargo.toml`. If it's parsing devcontainer.json comments, migrate to **`jsonc-parser`** (serde-compatible, maintained).
3. **`async-tar`/`tokio-tar` "TARmageddon"** (CVE-2025-62518, CVSS 8.1) — RCE via TAR boundary parsing. Audit OCI layer extraction path (`oci/`, `crates/core/src/templates.rs`); pull `tar` 0.4 directly if needed and pin a patched version.
4. **MSRV bump**: workspace pins `rust-version = "1.70"`. Several actively-maintained deps now require 1.75+; the MSRV-aware resolver (default since 1.84) silently pins old versions otherwise. Bump to **1.82** (unblocks deps; doesn't force Edition 2024).
5. **Add `cargo-audit` + `cargo-deny` to CI** as gating steps. Non-negotiable for a 1.0.

### Adopt before 1.0
- **`cargo-dist`** (axodotdev) for release artifacts: builds binaries, generates installers (sh/ps1/Homebrew/MSI), wires GitHub Actions. *Superseded:* `release.yml` now ships 8 targets with SHA256s, SBOMs, provenance and an install script, without `cargo-dist`.
- **`cargo-zigbuild`** for cross-compilation: 5–10× faster than `cross`, no Docker overhead, clean musl/glibc-version targeting. (Caveat: not compatible with `RUSTFLAGS="-C target-feature=+crt-static"`; use `--target x86_64-unknown-linux-musl` for static.)
- ~~**`wiremock`** for OCI registry test fixtures (if not already in use) — async-first, supersedes `mockito`.~~ In use (`wiremock` 0.6, `deacon-core` dev-dependency).
- **`insta`** snapshot tests for JSON output contracts (`read-configuration --output json`, etc.). This aligns with existing tech-debt issue #3. *Declined:* #3 was closed as not planned, and `insta` is not a dependency.
- **Document the `reqwest = "0.12"` pin** with an inline `Cargo.toml` comment so the next dependabot PR doesn't quietly accept 0.13. (0.13 forces `aws-lc-rs`/CMake/NASM and breaks pure-Rust cross-compile — your prior decision is correct, but the rationale needs to live in code, not memory.)

### Stay the course
- **`reqwest 0.12`** (rustls + `ring`) — correct pin for pure-Rust cross-compile.
- **`tokio` 1.x** — pin to an LTS line (1.47 until Sep 2026, or 1.51 until Mar 2027) for 1.0 stability.
- **`clap 4.x`** — bump 4.5 → 4.6 (no breaking changes); skip alternatives.
- **`serde_json`** — skip `simd-json`; benchmarks show serde_json wins on small files (devcontainer.json is <1KB).

### Watch (post-1.0)
- ~~**Edition 2024 migration** (Rust 1.85+). `cargo fix --edition` handles most of it; defer to a 1.x point release.~~ **Done**: the workspace is on Edition 2024.
- **`oci-client`** (oras-project) — could replace our hand-rolled `HttpClient`/OCI code, but the architectural lock-in risk for 1.0 is too high.
- **`bollard`** for native Docker/Podman API — supersedes shell-out. Already filed as tech-debt issue #2 (since closed as not planned). Significant rewrite; post-1.0.
- **OpenTelemetry export** via `tracing-opentelemetry` — gate behind a `telemetry` cargo feature if requested.
- **Edera's TARmageddon fixes** — follow upstream `tar-rs` and `astral-sh/uv`-style hardenings.

---

## 5. Examples, Docs & Distribution Hygiene

### Examples — Constitution §9 violation
Constitution principle 9 mandates "Executable & Self-Verifying Examples" (every example has `exec.sh` that runs all README scenarios and cleans up). The May 2026 table counted top-level area directories. Examples live one level down, in `examples/<area>/<name>/`, and as of 2026-10-27 91 of those 115 directories have an `exec.sh`. The 24 without one:

| Area | Directories missing `exec.sh` |
|---|---|
| `cli` | `custom-container-name`, `port-forwarding` |
| `compose` | `multiservice-basic`, `port-events` |
| `configuration` | `basic`, `extends-chain`, `nested-variables`, `with-variables` |
| `container-lifecycle` | `advanced`, `basic`, `execution-order`, `non-blocking-and-skip`, `progress-events`, `redaction`, `with-variables` |
| `doctor` | `host-requirements` |
| `features` | `cache-reuse-hint`, `dependencies-and-ordering`, `lockfile-demo`, `parallel-install-demo` |
| `registry` | `authentication` |
| `template-management` | `minimal-template`, `template-with-options`, `templates-apply` |

Per-canary run status is in `examples/CANARY_STATUS.md`.

**Action:** Either complete each example or delete the directory. Half-finished example dirs in `examples/` violate "no half-finished implementations" (system instructions).

### Missing release artifacts
All four have been addressed since May 2026:
- ~~**No `CHANGELOG.md`**~~ — `CHANGELOG.md` exists (keepachangelog layout, `[Unreleased]` section).
- ~~**No release notes**~~ — tagged GitHub releases exist (`v0.2.0` through `v0.4.0`), published by `release.yml`.
- ~~**No `SECURITY.md`**~~ — `SECURITY.md` exists at the repository root.
- ~~**No `CONTRIBUTING.md`** at top level~~ — `CONTRIBUTING.md` exists at the repository root.

### CI gaps
- ~~No `cargo-audit` step.~~ / ~~No `cargo-deny` (license + advisory checks).~~ — `ci.yml` runs `cargo deny check` (`deny.toml` configures `[advisories]`, licenses, bans and sources), which subsumes `cargo-audit`.
- ~~No artifact build/upload step (will come with `cargo-dist`).~~ — `release.yml` builds and uploads 8 targets.
- Dependabot is configured (recent commit `bb4abf6`); good.

### Docs that should be reconciled or pruned

**Done (2026-05-26 docs purge):** the four overlapping roadmap/parity docs
(`MVP-ROADMAP.md`, `CLI_PARITY.md`, `PARITY_APPROACH.md`,
`PARITY_PROMPT.md`) and the 1.4 MB `repomix-output-devcontainers-cli.xml`
upstream-codebase snapshot have all been deleted, along with the
per-subcommand pre-implementation artifacts that used to live under
`docs/subcommand-specs/`. The deacon-authored spec docs were subsequently
removed entirely: the only sources of truth are now the official
[containers.dev specification](https://containers.dev) and the reference
implementation (`@devcontainers/cli`).

---

## 6. Prioritized 1.0 Punchlist

### Tier 1 — Blockers (must ship)

**Status 2026-10-27: every Tier 1 item is done.**

1. ~~**Fix issue #1**: features must be installed during image build, not in the running container. Spec-parity bug.~~ **Done**: issue #1 closed (§3.E).
2. ~~**Implement `set-up`** subcommand (per the reference CLI's behavior).~~ **Done**: `commands/set_up.rs`; `set-up` rows in `SPEC_STATUS.md`.
3. ~~**Implement `upgrade`** subcommand (per the reference CLI's behavior).~~ **Done**: `commands/upgrade.rs`; `upgrade` rows in `SPEC_STATUS.md`.
4. ~~**Lockfile graduation** (CLI parity §B.1): `--no-lockfile`, `--frozen-lockfile`, default-on writes on `up`/`build`.~~ **Done** (§3.B.1).
5. ~~**`devcontainer.metadata` label format** (CLI parity §B.2): always emit JSON array on `build`.~~ **Done** (§3.B.2).
6. ~~**Migrate off `serde_yaml`** (deprecated) → `serde_yaml_ng`.~~ **Done**: no YAML crate remains in the graph.
7. ~~**Audit `async-tar`/`tokio-tar` paths** for CVE-2025-62518; patch or replace.~~ **Done**: product code uses `tar` 0.4.46; the only `tokio-tar` is `astral-tokio-tar` 0.6.2 (dev-only, via `testcontainers`).
8. ~~**Migrate off `json5`** (RUSTSEC-2025-0120) → `jsonc-parser` if used for devcontainer.json comments.~~ **Done**: `jsonc-parser` 0.34 (#52).
9. ~~**MSRV bump** to 1.82; update `rust-version` in workspace and document in CI.~~ **Done**: MSRV 1.95, pinned in the CI `MSRV (cargo check)` job.
10. ~~**Add `cargo-audit` + `cargo-deny` to CI**.~~ **Done**: `cargo deny check` in `ci.yml`.
11. ~~**Resolve `run-user-commands` container-selection wiring** (`run_user_commands.rs:32,36`, issue #269).~~ **Done**: `--container-id` / `--id-label`, with the configuration read from the container's metadata label.
12. ~~**Resolve `read-configuration` from-running-container path** (issue #268).~~ **Done** — `read-configuration` accepts `--container-id`/`--id-label`, resolves the container, and reads the `devcontainer.metadata` label (`getImageMetadataFromContainer` parity).
13. ~~**Pick a Podman story**: either ship supported (own the items in §3.D) or explicitly mark experimental in help text + docs.~~ **Done**: supported, with a required CI lane (§3.D). Follow-ups are on #30.
14. ~~**Add `CHANGELOG.md`** and start filling it (1.0.0-rc.1, 1.0.0-rc.2, … entries).~~ **Done**: `CHANGELOG.md` exists.

### Tier 2 — Strongly recommended

15. ~~CLI parity §B.3–B.5: `--mount-git-worktree-common-dir`, default `--workspace-folder = CWD` (issues #610 and #615), Windows drive-letter normalization.~~ **Done** (#664, #610/#615, #682).
16. ~~`--container-session-data-folder` flag (parity with reference probe-cache plumbing).~~ **Done**: global flag.
17. `--omit-paths` on `templates apply`. **Still open.**
18. ~~`--stop-for-personalization` on `run-user-commands`.~~ **Done**.
19. ~~Adopt `cargo-dist` for release artifacts (Linux x86_64/aarch64 musl, macOS arm64/x86_64, Windows x86_64); switch cross-compile to `cargo-zigbuild`.~~ **Superseded**: `release.yml` builds 8 targets (Linux gnu/musl × x86_64/aarch64, macOS × 2, Windows × 2) with `cross` for the musl targets, and publishes SHA256s, SBOMs and provenance.
20. ~~Add snapshot tests (`insta`) for JSON output contracts (tech-debt issue #3).~~ **Declined**: #3 was closed as not planned.
21. Document `reqwest 0.12` pin rationale inline in `Cargo.toml`. **Partly done**: `crates/core/Cargo.toml` explains the rustls choice next to the pin, but not why 0.13 is avoided. That reason appears only in a later comment, as a pointer to a "memory note".
22. Examples cleanup per Constitution §9: either complete or delete the 24 example directories still missing `exec.sh` (§5).
23. **Done (2026-05-26):** historical roadmap/parity docs and the `repomix-output-devcontainers-cli.xml` snapshot purged; `.gitignore` updated to keep `repomix-output*` artifacts out of VCS.
24. **Done:** `SECURITY.md` added in PR #47 (2026-05-26); `CONTRIBUTING.md` refreshed in the same PR.

### Tier 3 — Nice-to-have for 1.0; otherwise post-1.0

25. CLI parity §B.6–B.8 (Dockerfile syntax injection, BuildKit cache hints, buildx platform envvars). **Still open** (B.6 is #732).
26. ~~`--secrets-file` and `--omit-config-remote-env-from-metadata`.~~ **Done**: both accepted.
27. ~~`--workspace-mount-consistency` on macOS.~~ **Done**: accepted on `up`.
28. Lifecycle command timeout enforcement (`core/src/lifecycle.rs:1199`). **Still open.**
29. Workspace-based container resolution (`TODO(#270)`, `core/src/container.rs:1669`). **Still open.**
30. ~~Address tech-debt issues #4–#9.~~ **Done**: all six closed (#9 completed, #4–#8 not planned).

### Explicitly post-1.0
- ~~Edition 2024 migration.~~ **Done** ahead of 1.0.
- `oci-client` adoption (replace hand-rolled `HttpClient`).
- `bollard` adoption (replace Docker shell-out; tech-debt issue #2, closed as not planned).
- OpenTelemetry export feature.
- `--include-features-configuration` deeper integration (already partial).
- Compose feature-gates T020–T029 (ignored tests; profiles, reconnect/secrets-file, postAttach/init).

---

## 7. Suggested Release Trains

If the punchlist sequences well, a possible cadence:

| Tag | Contents |
|---|---|
| **`1.0.0-rc.1`** | Tier 1 items 1–10 (correctness, security, deprecations) |
| **`1.0.0-rc.2`** | Tier 1 items 11–14 + Tier 2 items 15–17 (parity completeness) |
| **`1.0.0-rc.3`** | Tier 2 items 18–24 (distribution + docs) |
| **`1.0.0`** | All Tier 1 + Tier 2 green; Tier 3 deferred to 1.1+ |
| **`1.1.0`** | Tier 3 items 25–30 |
| **`1.2.0`** | Edition 2024, `oci-client`, `bollard` evaluation |

*Status 2026-10-27:* releases so far are `0.x` (latest tag `v0.4.0`; the crate version on `main` is `0.5.0`), not the `1.0.0-rc.N` train above. Tier 1 is complete. In Tier 2, items 17 (`--omit-paths`), 21 (partly) and 22 remain open.

---

## 8. Risks & Open Questions

- **Lockfile semantics** (Tier 1 #4): the upstream graduation in v0.87.0 changed default behavior — existing Deacon users may be surprised by automatic `.devcontainer-lock.json` writes. Consider a one-time deprecation notice or opt-in flag for the 1.0-rc cycle.
- **Podman commitment** (*resolved: supported*, §3.D): if we ship 1.0 with Podman marked supported, we own the bug surface (rootless, `userns`, SELinux). If we mark experimental, we leave a chunk of the dev-on-Linux market underserved. This is a product call, not an engineering one.
- **`doctor` subcommand**: not in upstream CLI. We should decide whether this is a stable contract in 1.0 or whether we mark it `--unstable`. If stable, document its contract in the CLI `--help` and README (it is a deacon-specific extension, so the official spec doesn't cover it).
- **`down` subcommand**: same — Deacon-original, no upstream contract to follow. Document the schema clearly so it's stable.
- ~~**Issue #1 root cause**~~ (*resolved*: issue #1 closed): needs investigation before we can scope a fix. May touch `up/compose` install-into-build-shape work (already merged for compose; verify Dockerfile path).

---

## Appendix A — Sources

- Reference CLI: `devcontainers/cli` HEAD ≈ v0.87.0 (May 2026). Subcommand surface in `src/spec-node/devContainersSpecCLI.ts`.
- Spec pin at the time: `devcontainers/spec@113500f4` (Aug 2025). Current HEAD `c95ffee` (May 2026) — doc-only deltas. The pin has since moved to `c95ffeed` (§2), and the pinned reference oracle is `@devcontainers/cli` 0.89.0 (`parity/oracle.json`).
- Reconciliation (2026-10-27): `deacon --help` and per-subcommand `--help`, `parity/SPEC_STATUS.md`, `parity/cases/*.json`, `Cargo.toml` / `Cargo.lock`, `.github/workflows/{ci,release}.yml`, `deny.toml`, `examples/`, and the GitHub issue states cited above.
- Rust ecosystem scan (May 2026): see embedded references for `serde_yaml` deprecation, RUSTSEC-2025-0120 (`json5`), CVE-2025-62518 (TARmageddon), Rust 2024 edition (1.85), `cargo-dist` v0.31, `cargo-zigbuild`, `oci-client`, `bollard`, `tokio` LTS lines.
- Local audit: `crates/deacon/src/commands/*`, `crates/core/src/*`, `.github/`, `examples/`.
