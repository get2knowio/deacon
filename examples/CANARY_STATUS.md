# Canary verification status

Cross-session **memory** of which `examples/*/exec.sh` canaries are known to
pass, so they don't have to be re-evaluated from scratch every session.

**Protocol (read this before/after running canaries):**
- **Before** re-running a canary, check its row here. A `✅` row verified at the
  current `main` rarely needs re-running — spend effort on `❓`/`✗`/changed areas.
- **After** running a canary, update its **Status** and **Verified** (date +
  short commit) so the memory stays current.
- A canary's result can only be trusted for the commit it was verified at; treat
  rows older than the last behavioral change in that area as stale (`❓`).
- Run with the release binary: `cargo build --release -p deacon` then
  `DEACON_BIN=/workspaces/deacon/target/release/deacon bash examples/<area>/<name>/exec.sh`.

**Status legend:**
- `✅ pass` — runs green (includes canaries that intentionally assert an error).
- `⚠️ fixture` — does **not** pass as-is; the cause is the example/environment,
  **not** a deacon bug (documented in Notes). Not a regression signal.
- `🚫 deferred` — exercises a deacon capability that isn't implemented yet.
- `❓ unverified` — not evaluated this cycle.

Last broad sweep: **2026-05-29** (against `main` including PRs #129/#131/#132/
#134/#139/#143/#144/#145 and this session's #147/#148/#149/#150/#151). Every
row is currently ✅ — the last `❓`/`⚠️` rows (`down/basic`, the `exec/*` and
`up/*` fixtures) were unblocked by those five PRs. A later pass added Tier 1–3
**coverage** canaries (compose `down`, feature dependency ordering, local /
contributed-option features, lockfile, `outdated`/`upgrade`, `config
substitute`, `doctor`, `runServices`, workspace-trust) — one of which surfaced
the compose-`stopCompose` fix (#153). The four top-level runners (`build/`,
`exec/`, `read-configuration/`, `up/`) just iterate their children and aren't
listed.

| Canary | Status | Verified | Notes |
|---|---|---|---|
| build/basic-dockerfile | ✅ pass | 2026-05-29 | |
| build/buildkit-gated-feature | ✅ pass | 2026-05-29 | needs debian base + `build.dockerfile` (#129) |
| build/compose-missing-service | ✅ pass | 2026-05-29 | asserts error |
| build/compose-service-target | ✅ pass | 2026-05-29 | |
| build/compose-unsupported-flags | ✅ pass | 2026-05-29 | asserts errors (`--push`/`--output`) |
| build/compose-with-features | ✅ pass | 2026-05-29 | compose+features build (#139) |
| build/dockerfile-with-features | ✅ pass | 2026-05-29 | feature layering (#129) |
| build/duplicate-tags | ✅ pass | 2026-05-29 | tag de-dup (#129) |
| build/image-reference | ✅ pass | 2026-05-29 | |
| build/image-reference-with-features | ✅ pass | 2026-05-29 | image-ref+features (#134) |
| build/invalid-config-name | ✅ pass | 2026-05-29 | asserts error |
| build/multi-tags-and-labels | ✅ pass | 2026-05-29 | |
| build/output-archive | ✅ pass | 2026-05-29 | |
| build/platform-and-cache | ✅ pass | 2026-05-29 | |
| build/push | ✅ pass | 2026-05-29 | push denial expected w/o registry (allow-fail) |
| build/push-output-conflict | ✅ pass | 2026-05-29 | asserts error |
| build/secrets-and-ssh | ✅ pass | 2026-05-29 | ssh needs `SSH_AUTH_SOCK` (allow-fail) |
| build/unwritable-output | ✅ pass | 2026-05-29 | asserts error |
| compose/multiple-compose-files | ✅ pass | 2026-05-29 | |
| compose/multiservice-down | ✅ pass | 2026-05-29 `f29a1a3` | compose `down`/`stopCompose` + `--remove`/`--volumes`; needs the `stop_project` fix (#153). `runServices` dropped — unset now brings up ALL services (#156, PR #157) |
| compose/run-services | ✅ pass | 2026-05-29 | `runServices` selectivity (app+worker up, idle down) (new) |
| configuration/extends-chain-cycle | ✅ pass | 2026-05-29 | asserts cycle errors |
| configuration/secrets-declarative | ✅ pass | 2026-05-29 | |
| configuration/substitute | ✅ pass | 2026-05-29 | `config substitute`: localEnv/localWorkspaceFolderBasename, `--dry-run` (new) |
| configuration/workspace-trust | ✅ pass | 2026-05-29 | host `initializeCommand` trust gate: `DEACON_NO_PROMPT` denies, `--trust-workspace` allows (new) |
| doctor/diagnostics | ✅ pass | 2026-05-29 | plain `doctor`, `--json`, `--bundle` (new) |
| doctor/gpu-host-requirements | ✅ pass | 2026-05-29 | |
| doctor/host-requirements-failure | ✅ pass | 2026-05-29 | |
| down/basic | ✅ pass | 2026-05-29 | `--all` now sweeps by `devcontainer.local_folder` + idempotent down on gone container (#147) |
| exec/container-id-targeting | ✅ pass | 2026-05-29 | baked `--mount-workspace-git-root false` into the example's `up` (#149) |
| exec/exit-code-handling | ✅ pass | 2026-05-29 | baked `--mount-workspace-git-root false` (#149) |
| exec/id-label-targeting | ✅ pass | 2026-05-29 | non-spec `containerLabels`→`runArgs --label`; git-root mount flag (#149) |
| exec/interactive-pty | ✅ pass | 2026-05-29 | |
| exec/non-interactive-streaming | ✅ pass | 2026-05-29 | PTY-on-non-tty + JSON stream fixes (#148); `xxd`→`od`, git-root flag (#149) |
| exec/remote-env-variables | ✅ pass | 2026-05-29 | git-root mount flag (#149) |
| exec/remote-user-execution | ✅ pass | 2026-05-29 | git-root mount flag (#149) |
| exec/user-env-probe-modes | ✅ pass | 2026-05-29 | camelCase `--default-user-env-probe` values (#148); git-root flag (#149) |
| exec/workspace-folder-discovery | ✅ pass | 2026-05-29 | git-root mount flag (#149) |
| features/contributed-options | ✅ pass | 2026-05-29 | feature-contributed mount/entrypoint/init/capAdd reach the container (new) |
| features/dependency-ordering | ✅ pass | 2026-05-29 `f29a1a3` | auto install order via `installsAfter`+`dependsOn` (no override); now uses local-path `dependsOn` form `./feature-lib` (#155, PR #158) |
| features/feature-contributed-lifecycle | ✅ pass | 2026-05-29 | |
| features/feature-env-injection | ✅ pass | 2026-05-29 | |
| features/local-feature | ✅ pass | 2026-05-29 | local `./` feature install + option override (new) |
| features/lockfile | ✅ pass | 2026-05-29 | lockfile generate / `--frozen-lockfile` pass + mismatch fail; needs ghcr (new) |
| features/oci-digest-pin | ✅ pass | 2026-05-29 | `name:tag@digest` parsing (#131) |
| features/option-sanitization | ✅ pass | 2026-05-29 | |
| features/override-install-order | ✅ pass | 2026-05-29 | |
| outdated/basic | ✅ pass | 2026-05-29 | `outdated --output json` + `--fail-on-outdated`; needs ghcr (new) |
| read-configuration/basic | ✅ pass | 2026-05-29 | |
| read-configuration/compose | ✅ pass | 2026-05-29 | |
| read-configuration/extends-chain | ✅ pass | 2026-05-29 | |
| read-configuration/features-additional | ✅ pass | 2026-05-29 | |
| read-configuration/features-minimal | ✅ pass | 2026-05-29 | |
| read-configuration/id-labels-and-devcontainerId | ✅ pass | 2026-05-29 | |
| read-configuration/legacy-normalization | ✅ pass | 2026-05-29 | |
| read-configuration/named-config-search | ✅ pass | 2026-05-29 | |
| read-configuration/override-config | ✅ pass | 2026-05-29 | |
| read-configuration/with-variables | ✅ pass | 2026-05-29 | |
| run-user-commands/basic | ✅ pass | 2026-05-29 | prebuild (#130) + feature lifecycle (#140) |
| set-up/basic | ✅ pass | 2026-05-29 | |
| template-management/optional-paths | ✅ pass | 2026-05-29 | |
| up/additional-mounts | ✅ pass | 2026-05-29 | |
| up/basic-image | ✅ pass | 2026-05-29 | |
| up/compose-basic | ✅ pass | 2026-05-29 | |
| up/compose-profiles | ✅ pass | 2026-05-29 | |
| up/configuration-output | ✅ pass | 2026-05-29 | base switched alpine→debian:bookworm-slim (git feature needs bash) (#151) |
| up/container-user-vs-remote-user | ✅ pass | 2026-05-29 | |
| up/dockerfile-build | ✅ pass | 2026-05-29 | |
| up/dotfiles-integration | ✅ pass | 2026-05-29 | repo URL `codespaces/dotfiles` (404)→`holman/dotfiles` (#151); `~` target-path expansion (#150) |
| up/gpu-modes | ✅ pass | 2026-05-29 | GPU `all` failure expected on non-GPU hosts (tolerated) |
| up/id-labels-reconnect | ✅ pass | 2026-05-29 | full-ID on reconnect (#143) |
| up/image-metadata-merge | ✅ pass | 2026-05-29 | |
| up/initialize-command | ✅ pass | 2026-05-29 | |
| up/lifecycle-hooks | ✅ pass | 2026-05-29 | non-existent `devuser`→root (apt needs root); array hooks→argv `["bash","-c",…]` (#151) |
| up/override-command | ✅ pass | 2026-05-29 | |
| up/ports-config | ✅ pass | 2026-05-29 | |
| up/prebuild-mode | ✅ pass | 2026-05-29 | keep-alive PATH fix (#145) |
| up/remote-env-secrets | ✅ pass | 2026-05-29 | |
| up/remove-existing | ✅ pass | 2026-05-29 | full-ID reuse (#143) |
| up/security-options | ✅ pass | 2026-05-29 | |
| up/skip-lifecycle | ✅ pass | 2026-05-29 | |
| up/update-remote-user-uid | ✅ pass | 2026-05-29 | |
| up/user-env-probe-modes | ✅ pass | 2026-05-29 | |
| up/wait-for | ✅ pass | 2026-05-29 | |
| up/with-features | ✅ pass | 2026-05-29 | canary python fix (#144) |
| up/workspace-mount | ✅ pass | 2026-05-29 | |
| upgrade/basic | ✅ pass | 2026-05-29 | `upgrade --dry-run` + lockfile write; needs ghcr (new) |
