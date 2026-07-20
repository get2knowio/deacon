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
- `✗ fail` — fails because of a **deacon bug** (documented in Notes). This IS a
  regression signal; it should have a tracking issue and a fix, not a row edit.
- `⚠️ fixture` — does **not** pass as-is; the cause is the example/environment,
  **not** a deacon bug (documented in Notes). Not a regression signal.
- `🚫 deferred` — exercises a deacon capability that isn't implemented yet.
- `❓ unverified` — not evaluated this cycle.

Last broad sweep: **2026-07-20** against `main` @ `de5b045` (post-#318/#319) —
all 91 canaries run with the release binary. **87 pass, 4 do not**:

- `features/oci-digest-pin` — **✗ deacon bug**, a regression of #131. A
  digest-pinned ref round-trips lossily: `FeatureRef::reference()` rejoins
  `name` + `version` with `:`, so a `version` of `sha256:<hex>` yields
  `…/git:sha256:<hex>`, which re-parses as name `git:sha256` + tag `<hex>` and
  requests `/v2/devcontainers/features/git:sha256/manifests/<hex>` → 404.
  `parse_name_and_tag` itself is correct; the defect is `reference()` in
  `crates/core/src/oci/types.rs` (same shape on `TemplateRef`). Needs `@` for a
  digest.
- `up/compose-profiles` — **⚠️ fixture**: `nginx.conf` is referenced by
  `docker-compose.yml` but was never committed, so Docker auto-creates it as a
  *directory* and the bind mount fails ("not a directory"). Add the file.
- `exec/container-id-targeting` — **❓ behavior change, unclassified**: with
  `--container-id` (and no `--workspace-folder`) `exec` no longer applies the
  config's `remoteUser`/`remoteEnv` — it runs as `root` with
  `CONTAINER_ENV_VAR` unset, so the canary's `| grep CONTAINER_` fails under
  `pipefail`. May well be correct (nothing names a config), but it is a change
  from the 2026-05-29 result; decide intended semantics before editing either
  side.
- `up/prebuild-mode` — **❓ unclassified**: `onCreate` `apt-get` fails with
  `Permission denied` (exit 100). Config sets `remoteUser: vscode` on
  `ubuntu:22.04`, which has no `vscode` user — the same latent shape that
  `up/lifecycle-hooks` was fixed for on 2026-05-29 ("non-existent `devuser`→root,
  apt needs root"). Likely fixture, but user resolution changed in #276/#299, so
  confirm rather than assume.

Sweep hygiene note: canaries left 9 stray `*devcontainer-lock.json` files and
(via the missing `nginx.conf`) one root-owned directory in the working tree.
Removed by hand; `exec.sh` cleanup is incomplete for those examples.

Prior sweep: 2026-05-29 (against `main` including PRs #129/#131/#132/#134/#139/
#143/#144/#145 and #147/#148/#149/#150/#151), when every row was ✅. A later
pass added Tier 1–3 **coverage** canaries (compose `down`, feature dependency
ordering, local / contributed-option features, lockfile, `outdated`/`upgrade`,
`config substitute`, `doctor`, `runServices`, workspace-trust) — one of which
surfaced the compose-`stopCompose` fix (#153). The four top-level runners
(`build/`, `exec/`, `read-configuration/`, `up/`) just iterate their children
and aren't listed.

| Canary | Status | Verified | Notes |
|---|---|---|---|
| build/basic-dockerfile | ✅ pass | 2026-07-20 `de5b045` |  |
| build/buildkit-gated-feature | ✅ pass | 2026-07-20 `de5b045` | needs debian base + `build.dockerfile` (#129) |
| build/compose-missing-service | ✅ pass | 2026-07-20 `de5b045` | asserts error |
| build/compose-service-target | ✅ pass | 2026-07-20 `de5b045` |  |
| build/compose-unsupported-flags | ✅ pass | 2026-07-20 `de5b045` | asserts errors (`--push`/`--output`) |
| build/compose-with-features | ✅ pass | 2026-07-20 `de5b045` | compose+features build (#139) |
| build/dockerfile-with-features | ✅ pass | 2026-07-20 `de5b045` | feature layering (#129) |
| build/duplicate-tags | ✅ pass | 2026-07-20 `de5b045` | tag de-dup (#129) |
| build/image-reference | ✅ pass | 2026-07-20 `de5b045` |  |
| build/image-reference-with-features | ✅ pass | 2026-07-20 `de5b045` | image-ref+features (#134) |
| build/invalid-config-name | ✅ pass | 2026-07-20 `de5b045` | asserts error |
| build/multi-tags-and-labels | ✅ pass | 2026-07-20 `de5b045` |  |
| build/output-archive | ✅ pass | 2026-07-20 `de5b045` |  |
| build/platform-and-cache | ✅ pass | 2026-07-20 `de5b045` |  |
| build/push | ✅ pass | 2026-07-20 `de5b045` | push denial expected w/o registry (allow-fail) |
| build/push-output-conflict | ✅ pass | 2026-07-20 `de5b045` | asserts error |
| build/secrets-and-ssh | ✅ pass | 2026-07-20 `de5b045` | ssh needs `SSH_AUTH_SOCK` (allow-fail) |
| build/unwritable-output | ✅ pass | 2026-07-20 `de5b045` | asserts error |
| compose/multiple-compose-files | ✅ pass | 2026-07-20 `de5b045` |  |
| compose/multiservice-down | ✅ pass | 2026-07-20 `de5b045` | compose `down`/`stopCompose` + `--remove`/`--volumes`; needs the `stop_project` fix (#153). `runServices` dropped — unset now brings up ALL services (#156, PR #157) |
| compose/run-services | ✅ pass | 2026-07-20 `de5b045` | `runServices` selectivity (app+worker up, idle down) (new) |
| configuration/extends-chain-cycle | ✅ pass | 2026-07-20 `de5b045` | asserts cycle errors |
| configuration/secrets-declarative | ✅ pass | 2026-07-20 `de5b045` |  |
| configuration/substitute | ✅ pass | 2026-07-20 `de5b045` | `config substitute`: localEnv/localWorkspaceFolderBasename, `--dry-run` (new) |
| configuration/workspace-trust | ✅ pass | 2026-07-20 `de5b045` | host `initializeCommand` trust gate: `DEACON_NO_PROMPT` denies, `--trust-workspace` allows (new) |
| doctor/diagnostics | ✅ pass | 2026-07-20 `de5b045` | plain `doctor`, `--json`, `--bundle` (new) |
| doctor/gpu-host-requirements | ✅ pass | 2026-07-20 `de5b045` |  |
| doctor/host-requirements-failure | ✅ pass | 2026-07-20 `de5b045` |  |
| down/basic | ✅ pass | 2026-07-20 `de5b045` | `--all` now sweeps by `devcontainer.local_folder` + idempotent down on gone container (#147) |
| exec/container-id-targeting | ❓ recheck | 2026-07-20 `de5b045` | `--container-id` no longer applies config `remoteUser`/`remoteEnv` (runs as root, `CONTAINER_ENV_VAR` unset); intended semantics undecided. |
| exec/exit-code-handling | ✅ pass | 2026-07-20 `de5b045` | baked `--mount-workspace-git-root false` (#149) |
| exec/id-label-targeting | ✅ pass | 2026-07-20 `de5b045` | non-spec `containerLabels`→`runArgs --label`; git-root mount flag (#149) |
| exec/interactive-pty | ✅ pass | 2026-07-20 `de5b045` |  |
| exec/non-interactive-streaming | ✅ pass | 2026-07-20 `de5b045` | PTY-on-non-tty + JSON stream fixes (#148); `xxd`→`od`, git-root flag (#149) |
| exec/remote-env-variables | ✅ pass | 2026-07-20 `de5b045` | git-root mount flag (#149) |
| exec/remote-user-execution | ✅ pass | 2026-07-20 `de5b045` | git-root mount flag (#149) |
| exec/user-env-probe-modes | ✅ pass | 2026-07-20 `de5b045` | camelCase `--default-user-env-probe` values (#148); git-root flag (#149) |
| exec/workspace-folder-discovery | ✅ pass | 2026-07-20 `de5b045` | git-root mount flag (#149) |
| features/contributed-options | ✅ pass | 2026-07-20 `de5b045` | feature-contributed mount/entrypoint/init/capAdd reach the container (new) |
| features/dependency-ordering | ✅ pass | 2026-07-20 `de5b045` | auto install order via `installsAfter`+`dependsOn` (no override); now uses local-path `dependsOn` form `./feature-lib` (#155, PR #158) |
| features/feature-contributed-lifecycle | ✅ pass | 2026-07-20 `de5b045` |  |
| features/feature-env-injection | ✅ pass | 2026-07-20 `de5b045` |  |
| features/local-feature | ✅ pass | 2026-07-20 `de5b045` | local `./` feature install + option override (new) |
| features/lockfile | ✅ pass | 2026-07-20 `de5b045` | lockfile generate / `--frozen-lockfile` pass + mismatch fail; needs ghcr (new) |
| features/oci-digest-pin | ✗ fail | 2026-07-20 `de5b045` | **deacon bug** — digest ref round-trip regression of #131; `reference()` rejoins with `:` not `@` → 404. See header. |
| features/option-sanitization | ✅ pass | 2026-07-20 `de5b045` |  |
| features/override-install-order | ✅ pass | 2026-07-20 `de5b045` |  |
| observability/json-logs | ✅ pass | 2026-07-20 `de5b045` | Output Streams Contract: `--log-format json` stdout=1 JSON doc, stderr=JSON log lines, no log leakage to stdout; hermetic (read-configuration, no Docker) (new) |
| outdated/basic | ✅ pass | 2026-07-20 `de5b045` | `outdated --output json` + `--fail-on-outdated`; needs ghcr (new) |
| read-configuration/basic | ✅ pass | 2026-07-20 `de5b045` |  |
| read-configuration/compose | ✅ pass | 2026-07-20 `de5b045` |  |
| read-configuration/extends-chain | ✅ pass | 2026-07-20 `de5b045` |  |
| read-configuration/features-additional | ✅ pass | 2026-07-20 `de5b045` |  |
| read-configuration/features-minimal | ✅ pass | 2026-07-20 `de5b045` |  |
| read-configuration/id-labels-and-devcontainerId | ✅ pass | 2026-07-20 `de5b045` |  |
| read-configuration/legacy-normalization | ✅ pass | 2026-07-20 `de5b045` |  |
| read-configuration/named-config-search | ✅ pass | 2026-07-20 `de5b045` |  |
| read-configuration/override-config | ✅ pass | 2026-07-20 `de5b045` | switched overlay demo to `--merge-config` (#285: `--override-config` now replaces) |
| read-configuration/with-variables | ✅ pass | 2026-07-20 `de5b045` |  |
| run-user-commands/basic | ✅ pass | 2026-07-20 `de5b045` | prebuild (#130) + feature lifecycle (#140) |
| set-up/basic | ✅ pass | 2026-07-20 `de5b045` |  |
| template-management/optional-paths | ✅ pass | 2026-07-20 `de5b045` |  |
| up/additional-mounts | ✅ pass | 2026-07-20 `de5b045` |  |
| up/auto-forward | ✅ pass | 2026-07-20 `de5b045` | `--auto-forward` loopback reach + multi-container collision-free (015) |
| up/basic-image | ✅ pass | 2026-07-20 `de5b045` |  |
| up/compose-basic | ✅ pass | 2026-07-20 `de5b045` |  |
| up/compose-profiles | ⚠️ fixture | 2026-07-20 `de5b045` | `nginx.conf` referenced by docker-compose.yml but never committed → docker makes a dir, bind mount fails. Not a deacon bug. |
| up/configuration-output | ✅ pass | 2026-07-20 `de5b045` | base switched alpine→debian:bookworm-slim (git feature needs bash) (#151) |
| up/container-user-vs-remote-user | ✅ pass | 2026-07-20 `de5b045` |  |
| up/dockerfile-build | ✅ pass | 2026-07-20 `de5b045` |  |
| up/dotfiles-integration | ✅ pass | 2026-07-20 `de5b045` | repo URL `codespaces/dotfiles` (404)→`holman/dotfiles` (#151); `~` target-path expansion (#150) |
| up/gpu-modes | ✅ pass | 2026-07-20 `de5b045` | GPU `all` failure expected on non-GPU hosts (tolerated) |
| up/host-ca | ✅ pass | 2026-07-20 `de5b045` | `--inject-host-ca` explicit bundle; debian-slim → env-var-only fallback (no `ca-certificates`), canonical bundle + CA env vars present (016) |
| up/id-labels-reconnect | ✅ pass | 2026-07-20 `de5b045` | full-ID on reconnect (#143) |
| up/image-metadata-merge | ✅ pass | 2026-07-20 `de5b045` |  |
| up/initialize-command | ✅ pass | 2026-07-20 `de5b045` |  |
| up/lifecycle-hooks | ✅ pass | 2026-07-20 `de5b045` | non-existent `devuser`→root (apt needs root); array hooks→argv `["bash","-c",…]` (#151) |
| up/override-command | ✅ pass | 2026-07-20 `de5b045` |  |
| up/ports-config | ✅ pass | 2026-07-20 `de5b045` |  |
| up/prebuild-mode | ❓ recheck | 2026-07-20 `de5b045` | onCreate `apt-get` Permission denied (exit 100); `remoteUser: vscode` on `ubuntu:22.04` has no such user. Same shape as the lifecycle-hooks fixture fix; confirm vs #276/#299. |
| up/remote-env-secrets | ✅ pass | 2026-07-20 `de5b045` |  |
| up/remove-existing | ✅ pass | 2026-07-20 `de5b045` | full-ID reuse (#143) |
| up/security-options | ✅ pass | 2026-07-20 `de5b045` |  |
| up/skip-lifecycle | ✅ pass | 2026-07-20 `de5b045` |  |
| up/up-exec-down | ✅ pass | 2026-07-20 `de5b045` | compound-flow up→exec→run-user-commands→down by --workspace-folder (#187 configHash fix) |
| up/update-remote-user-uid | ✅ pass | 2026-07-20 `de5b045` |  |
| up/user-env-probe-modes | ✅ pass | 2026-07-20 `de5b045` |  |
| up/wait-for | ✅ pass | 2026-07-20 `de5b045` |  |
| up/with-features | ✅ pass | 2026-07-20 `de5b045` | canary python fix (#144) |
| up/workspace-mount | ✅ pass | 2026-07-20 `de5b045` |  |
| upgrade/basic | ✅ pass | 2026-07-20 `de5b045` | `upgrade --dry-run` + lockfile write; needs ghcr (new) |
