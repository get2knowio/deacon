# Deacon → 1.0 Readiness Report

**Prepared:** 2026-05-25
**Scope:** Consumer-side devcontainer CLI in Rust. Feature/template *authoring* remains permanently out of scope (per Constitution v1.13.0 §2).
**Inputs:** Codebase audit, upstream `@devcontainers/cli` (HEAD ≈ v0.87.0, May 2026), `containers.dev` spec (HEAD `c95ffee`, May 2026), Rust ecosystem scan (May 2026).

---

## TL;DR

Deacon is in much better shape than the issue tracker suggests. The **spec itself has had zero substantive changes since the Aug 2025 pin (`113500f4`)** — everything new is in the reference CLI's surface, the Rust ecosystem, or our own deferred work. The path to 1.0 is bounded and visible:

- **One real spec-parity bug** (issue #1: features installed in running container instead of during image build).
- **Five CLI-surface gaps added upstream since our pin** that consumers will expect.
- **Two unmaintained dependencies** (`serde_yaml`, `json5`) and one critical CVE we need to audit (`async-tar`/`tokio-tar` TARmageddon).
- **Two specced-but-not-implemented subcommands** (`set-up`, `upgrade`) the reference CLI exposes as consumer-side.
- **Distribution story missing**: no `CHANGELOG.md`, no release artifacts (`cargo-dist`), no security gates (`cargo-audit`/`cargo-deny`) in CI.
- **Examples hygiene drift**: 5 of 13 example directories violate Constitution §9 (no `exec.sh`).

**Estimate:** ~6–10 weeks of focused work to a confident 1.0 tag, if no major scope additions land.

---

## 1. Codebase Snapshot

### Implemented subcommands (9)
| Command | Status | Notes |
|---|---|---|
| `up` | Implemented, ~95% per `up/GAP.md` | Lockfile flags and a few experimental options deferred |
| `build` | Implemented | Feature integration TODOs at `build/mod.rs:1120,1285`; metadata label format needs update (§3.B.2) |
| `exec` | Implemented | Recent signal-exit and home-fallback fixes landed |
| `down` | Implemented | Not in upstream CLI — Deacon-original |
| `read-configuration` | Implemented | Container-based reading done (`--container-id`/`--id-label`; reads `devcontainer.metadata` off the container, issue #268) |
| `run-user-commands` | Implemented | Container selection wiring incomplete (`run_user_commands.rs:32,36`, issue #269) |
| `config` (substitute/merge-*) | Implemented | Deacon-original utility surface |
| `templates apply` / `pull` | Implemented | `--omit-paths` missing |
| `outdated` | Implemented | Spec at "Phase 2"; works against feature lockfile concept |
| `doctor` | Not present in reference CLI — Deacon-original | Not yet implemented per audit |

### Not yet implemented
- **`set-up`** — the reference CLI exposes this; it adopts an existing container as a dev container. **Required for parity.**
- **`upgrade`** — the reference CLI graduated `upgrade` from experimental in v0.87.0 ([PR #1212](https://github.com/devcontainers/cli/pull/1212)). **Required for parity.**

### Core library shape
The `crates/core/src/` module set is comprehensive: `config`, `features`, `container_lifecycle`, `oci/*`, `compose`, `dotfiles`, `gpu`, `host_requirements`, `mount`, `secrets`, `redaction`, `variable`, `lockfile`, `state`, `workspace`, `cache/*`, `docker_retry`, `progress`, etc. The major abstractions called out in `CLAUDE.md` (`ContainerRuntime`, `HttpClient`, `ConfigLoader`, `FeatureInstaller`, `ContainerLifecycle`) are present and structured.

### Test coverage
- `crates/deacon/tests/` — 88 integration tests
- `crates/core/tests/` — 43 integration tests
- Gaps: `set-up`, `upgrade`, dedicated `config merge-*` integration coverage.
- `#[ignore]`d tests cluster around feature-gates T020–T029 (compose profiles, reconnect/secrets-file, postAttach/init).

### TODO/FIXME inventory
57 markers total; most cluster in test scaffolding for ungated features. The non-test TODOs worth tracking on the 1.0 punchlist:

| File | Note |
|---|---|
| `container.rs:1036` | Workspace-based container resolution (issue #270) |
| ~~`read_configuration.rs:33`~~ | ~~Container-based config reading (issue #268)~~ — **done** (`--container-id`/`--id-label` + `devcontainer.metadata` read) |
| `read_configuration.rs:467,507` | Feature metadata caching; `metadata.init` field |
| `run_user_commands.rs:32,36,158` | Container selection wiring; collect lifecycle commands from extends/features |
| `build/mod.rs:1120,1285` | Feature integration; `feature_set_digest` placeholder |
| `lifecycle.rs:1058` | Timeout enforcement on lifecycle commands |
| `compose.rs:356` | Feature-derived options in validation |

---

## 2. Spec Compliance (`containers.dev`)

**The headline:** between our pin `113500f4` (2025-08-01 — note `CLAUDE.md` records "October 2025"; the SHA is actually August) and spec HEAD `c95ffee` (May 2026), every merged commit is doc/typo/formatting. **No new fields, no new behaviors, no new contracts.** Our pin is current.

**Action:** Update `CLAUDE.md` line 6 to read `113500f4` (Aug 2025) — or move the pin to spec HEAD now (no behavioral consequence). Either way, fix the date.

**Open proposals to watch (not 1.0 blockers):**
- [spec#22](https://github.com/devcontainers/spec/issues/22) `extends` finalization — we already support it.
- [spec#716](https://github.com/devcontainers/spec/issues/716) versioned, reusable configuration packages.
- [spec#324](https://github.com/devcontainers/spec/pull/324) `build.options` (open since 2024).

---

## 3. Reference CLI Parity Gaps

### A. Missing consumer-side subcommands
Both are already specced; treat as primary 1.0 work.

1. **`set-up`** — adopts an existing container (`--container-id` required) as a dev container, runs lifecycle + dotfiles, emits `{outcome, configuration?, mergedConfiguration?}`. Reference: `devContainersSpecCLI.ts:79` and `:354`.
2. **`upgrade`** — upgrades the lockfile; graduated from experimental in v0.87.0 ([PR #1212](https://github.com/devcontainers/cli/pull/1212)). Reference: `devContainersSpecCLI.ts:85`.

### B. CLI-surface changes upstream made since our Oct 2025 pin
| # | Item | Severity | Source |
|---|---|---|---|
| B.1 | **Lockfile graduation**: `--no-lockfile`, `--frozen-lockfile`, and *default-on* lockfile writes on `up`/`build` (`.devcontainer-lock.json`) | **High** — CI primitive | v0.87.0, [PR #1212](https://github.com/devcontainers/cli/pull/1212) |
| B.2 | **`devcontainer.metadata` label is always a JSON array**, even for single-entry metadata. Writer-side contract change. | **High** — image interop with VS Code/Zed/envbuilder | v0.86.0, [PR #1199](https://github.com/devcontainers/cli/pull/1199) |
| B.3 | `--mount-git-worktree-common-dir` for worktrees with `--relative-paths` | Medium | v0.81.0, [PR #1127](https://github.com/devcontainers/cli/pull/1127) |
| B.4 | Default `--workspace-folder` to CWD when omitted | Medium — UX | v0.82.0, [PR #1104](https://github.com/devcontainers/cli/pull/1104) |
| B.5 | Windows drive-letter lowercase normalization for id-labels (parity with VS Code) | Low — Windows-only | v0.86.0 |
| B.6 | Skip injecting `# syntax=docker/dockerfile:1.4` when Docker Engine ≥ 23.0.0; hidden `--omit-syntax-directive` flag | Low | v0.80.3, [PR #1118](https://github.com/devcontainers/cli/pull/1118) |
| B.7 | `BUILDKIT_INLINE_CACHE=1` on Feature build path for cache reuse | Low — perf | v0.83.0, [PR #1135](https://github.com/devcontainers/cli/pull/1135) |
| B.8 | Buildx platform envvars inlined when resolving base image/user | Low | v0.85.0, [PR #1169](https://github.com/devcontainers/cli/pull/1169) |

### C. Long-standing reference flags worth a parity audit
- `--container-session-data-folder` (distinct from `--container-data-folder`; specifically for userEnvProbe caching). Our cache helper uses `--container-data-folder`; spec parity says we should also accept the session variant.
- `--stop-for-personalization` on `run-user-commands`.
- `--omit-paths` on `templates apply` ([PR #868](https://github.com/devcontainers/cli/pull/868), 2024-08).
- `--secrets-file` (JSON of lifecycle-only secrets, never persisted to metadata) and `--omit-config-remote-env-from-metadata` (hidden).
- `--workspace-mount-consistency {consistent|cached|delegated}` (macOS bind-mount perf).
- `--gpu-availability {all|detect|none}` — Deacon has a flag; verify default is `detect` to match.
- `--update-remote-user-uid-default {never|on|off}` and `--default-user-env-probe {none|loginInteractiveShell|interactiveShell|loginShell}`.

### D. Podman parity
Constitution lists Podman as "in development." Upstream made Podman a first-class peer over the last year:
- `label=disable` SELinux mount option instead of `:z` for rootless ([PR #1045](https://github.com/devcontainers/cli/pull/1045), v0.80.0).
- `--uidmap`/`--gidmap` must not conflict with `--userns` ([PRs #1005](https://github.com/devcontainers/cli/pull/1005), [#1018](https://github.com/devcontainers/cli/pull/1018), v0.77.0).
- Omit `--userns=keep-id` for root user.
- Rootless Docker support landed 2026-03.

Decide for 1.0: **Either** ship Podman as supported (and own the parity items above) **or** mark it explicitly experimental in `--help` and docs. The current "in development" middle ground is the worst of both.

### E. Open bug already filed
- **Issue #1: Features installed in running container instead of during image build.** This is a spec-parity violation (Constitution §1) and a real correctness problem — features installed at runtime won't survive container rebuilds, won't appear in the image metadata label, and won't match VS Code/reference behavior. **Treat as a 1.0 blocker.**

---

## 4. Rust Ecosystem Actions

### Urgent (1.0 blockers)
1. **`serde_yaml` is deprecated** (since March 2024). Used for compose file parsing. Migrate to **`serde_yaml_ng`** (drop-in fork by acatton; same API).
2. **`json5` crate is unmaintained** — RUSTSEC-2025-0120. Used in `deacon-core/Cargo.toml`. If it's parsing devcontainer.json comments, migrate to **`jsonc-parser`** (serde-compatible, maintained).
3. **`async-tar`/`tokio-tar` "TARmageddon"** (CVE-2025-62518, CVSS 8.1) — RCE via TAR boundary parsing. Audit OCI layer extraction path (`oci/`, `crates/core/src/templates.rs`); pull `tar` 0.4 directly if needed and pin a patched version.
4. **MSRV bump**: workspace pins `rust-version = "1.70"`. Several actively-maintained deps now require 1.75+; the MSRV-aware resolver (default since 1.84) silently pins old versions otherwise. Bump to **1.82** (unblocks deps; doesn't force Edition 2024).
5. **Add `cargo-audit` + `cargo-deny` to CI** as gating steps. Non-negotiable for a 1.0.

### Adopt before 1.0
- **`cargo-dist`** (axodotdev) for release artifacts: builds binaries, generates installers (sh/ps1/Homebrew/MSI), wires GitHub Actions. We currently have a `release.yml` but no artifact distribution story.
- **`cargo-zigbuild`** for cross-compilation: 5–10× faster than `cross`, no Docker overhead, clean musl/glibc-version targeting. (Caveat: not compatible with `RUSTFLAGS="-C target-feature=+crt-static"`; use `--target x86_64-unknown-linux-musl` for static.)
- **`wiremock`** for OCI registry test fixtures (if not already in use) — async-first, supersedes `mockito`.
- **`insta`** snapshot tests for JSON output contracts (`read-configuration --output json`, etc.). This aligns with existing tech-debt issue #3.
- **Document the `reqwest = "0.12"` pin** with an inline `Cargo.toml` comment so the next dependabot PR doesn't quietly accept 0.13. (0.13 forces `aws-lc-rs`/CMake/NASM and breaks pure-Rust cross-compile — your prior decision is correct, but the rationale needs to live in code, not memory.)

### Stay the course
- **`reqwest 0.12`** (rustls + `ring`) — correct pin for pure-Rust cross-compile.
- **`tokio` 1.x** — pin to an LTS line (1.47 until Sep 2026, or 1.51 until Mar 2027) for 1.0 stability.
- **`clap 4.x`** — bump 4.5 → 4.6 (no breaking changes); skip alternatives.
- **`serde_json`** — skip `simd-json`; benchmarks show serde_json wins on small files (devcontainer.json is <1KB).

### Watch (post-1.0)
- **Edition 2024 migration** (Rust 1.85+). `cargo fix --edition` handles most of it; defer to a 1.x point release.
- **`oci-client`** (oras-project) — could replace our hand-rolled `HttpClient`/OCI code, but the architectural lock-in risk for 1.0 is too high.
- **`bollard`** for native Docker/Podman API — supersedes shell-out. Already filed as tech-debt issue #2. Significant rewrite; post-1.0.
- **OpenTelemetry export** via `tracing-opentelemetry` — gate behind a `telemetry` cargo feature if requested.
- **Edera's TARmageddon fixes** — follow upstream `tar-rs` and `astral-sh/uv`-style hardenings.

---

## 5. Examples, Docs & Distribution Hygiene

### Examples — Constitution §9 violation
Constitution principle 9 mandates "Executable & Self-Verifying Examples" (every example has `exec.sh` that runs all README scenarios and cleans up). Current state:

| Status | Directories |
|---|---|
| ✓ Both `exec.sh` and `README.md` | `build`, `exec`, `read-configuration`, `up` (4) |
| Missing `exec.sh` | `build-secrets`, `configuration`, `container-lifecycle` (3) |
| Missing `README.md` | `build`, `compose`, `doctor` (3) |
| Missing both | `cli`, `compose`, `doctor`, `features`, `observability`, `registry`, `template-management` (5+) |

**Action:** Either complete each example or delete the directory. Half-finished example dirs in `examples/` violate "no half-finished implementations" (system instructions).

### Missing release artifacts
- **No `CHANGELOG.md`**. For a CLI heading to 1.0, this is table stakes. Adopt `keepachangelog.com` format or `git-cliff` for generation.
- **No release notes**. Tie to `cargo-dist` adoption above.
- **No `SECURITY.md`** in `.github/`. Standard for any tool that fetches and executes container images.
- **No `CONTRIBUTING.md`** at top level (constitution + `AGENTS.md` cover internals but not external contributor onboarding).

### CI gaps
- No `cargo-audit` step.
- No `cargo-deny` (license + advisory checks).
- No artifact build/upload step (will come with `cargo-dist`).
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

1. **Fix issue #1**: features must be installed during image build, not in the running container. Spec-parity bug.
2. **Implement `set-up`** subcommand (per the reference CLI's behavior).
3. **Implement `upgrade`** subcommand (per the reference CLI's behavior).
4. **Lockfile graduation** (CLI parity §B.1): `--no-lockfile`, `--frozen-lockfile`, default-on writes on `up`/`build`. Touches `up`, `build`, and the existing `lockfile.rs`.
5. **`devcontainer.metadata` label format** (CLI parity §B.2): always emit JSON array on `build`.
6. **Migrate off `serde_yaml`** (deprecated) → `serde_yaml_ng`.
7. **Audit `async-tar`/`tokio-tar` paths** for CVE-2025-62518; patch or replace.
8. **Migrate off `json5`** (RUSTSEC-2025-0120) → `jsonc-parser` if used for devcontainer.json comments.
9. **MSRV bump** to 1.82; update `rust-version` in workspace and document in CI.
10. **Add `cargo-audit` + `cargo-deny` to CI**.
11. **Resolve `run-user-commands` container-selection wiring** (`run_user_commands.rs:32,36`, issue #269).
12. ~~**Resolve `read-configuration` from-running-container path** (issue #268).~~ **Done** — `read-configuration` accepts `--container-id`/`--id-label`, resolves the container, and reads the `devcontainer.metadata` label (`getImageMetadataFromContainer` parity).
13. **Pick a Podman story**: either ship supported (own the items in §3.D) or explicitly mark experimental in help text + docs.
14. **Add `CHANGELOG.md`** and start filling it (1.0.0-rc.1, 1.0.0-rc.2, … entries).

### Tier 2 — Strongly recommended

15. CLI parity §B.3–B.5: `--mount-git-worktree-common-dir`, default `--workspace-folder = CWD`, Windows drive-letter normalization.
16. `--container-session-data-folder` flag (parity with reference probe-cache plumbing).
17. `--omit-paths` on `templates apply`.
18. `--stop-for-personalization` on `run-user-commands`.
19. Adopt `cargo-dist` for release artifacts (Linux x86_64/aarch64 musl, macOS arm64/x86_64, Windows x86_64); switch cross-compile to `cargo-zigbuild`.
20. Add snapshot tests (`insta`) for JSON output contracts (tech-debt issue #3).
21. Document `reqwest 0.12` pin rationale inline in `Cargo.toml`.
22. Examples cleanup per Constitution §9: either complete or delete the 9 incomplete example directories.
23. **Done (2026-05-26):** historical roadmap/parity docs and the `repomix-output-devcontainers-cli.xml` snapshot purged; `.gitignore` updated to keep `repomix-output*` artifacts out of VCS.
24. **Done:** `SECURITY.md` added in PR #47 (2026-05-26); `CONTRIBUTING.md` refreshed in the same PR.

### Tier 3 — Nice-to-have for 1.0; otherwise post-1.0

25. CLI parity §B.6–B.8 (Dockerfile syntax injection, BuildKit cache hints, buildx platform envvars).
26. `--secrets-file` and `--omit-config-remote-env-from-metadata`.
27. `--workspace-mount-consistency` on macOS.
28. Lifecycle command timeout enforcement (`lifecycle.rs:1058`).
29. Workspace-based container resolution (issue #270, `container.rs:1036`).
30. Address tech-debt issues #4–#9.

### Explicitly post-1.0
- Edition 2024 migration.
- `oci-client` adoption (replace hand-rolled `HttpClient`).
- `bollard` adoption (replace Docker shell-out; tech-debt issue #2).
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

---

## 8. Risks & Open Questions

- **Lockfile semantics** (Tier 1 #4): the upstream graduation in v0.87.0 changed default behavior — existing Deacon users may be surprised by automatic `.devcontainer-lock.json` writes. Consider a one-time deprecation notice or opt-in flag for the 1.0-rc cycle.
- **Podman commitment**: if we ship 1.0 with Podman marked supported, we own the bug surface (rootless, `userns`, SELinux). If we mark experimental, we leave a chunk of the dev-on-Linux market underserved. This is a product call, not an engineering one.
- **`doctor` subcommand**: not in upstream CLI. We should decide whether this is a stable contract in 1.0 or whether we mark it `--unstable`. If stable, document its contract in the CLI `--help` and README (it is a deacon-specific extension, so the official spec doesn't cover it).
- **`down` subcommand**: same — Deacon-original, no upstream contract to follow. Document the schema clearly so it's stable.
- **Issue #1 root cause**: needs investigation before we can scope a fix. May touch `up/compose` install-into-build-shape work (already merged for compose; verify Dockerfile path).

---

## Appendix A — Sources

- Reference CLI: `devcontainers/cli` HEAD ≈ v0.87.0 (May 2026). Subcommand surface in `src/spec-node/devContainersSpecCLI.ts`.
- Spec pin: `devcontainers/spec@113500f4` (Aug 2025). Current HEAD `c95ffee` (May 2026) — doc-only deltas.
- Rust ecosystem scan (May 2026): see embedded references for `serde_yaml` deprecation, RUSTSEC-2025-0120 (`json5`), CVE-2025-62518 (TARmageddon), Rust 2024 edition (1.85), `cargo-dist` v0.31, `cargo-zigbuild`, `oci-client`, `bollard`, `tokio` LTS lines.
- Local audit: `crates/deacon/src/commands/*`, `crates/core/src/*`, `.github/`, `examples/`.
