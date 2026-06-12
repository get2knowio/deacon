# Parity corpus — findings

Oracle: `@devcontainers/cli` v0.87.0. deacon: this branch. 23 corpus configs.

Source of truth: the official containers.dev spec and the reference CLI's
behavior — not any deacon-authored spec doc.

## Fixed in this PR (thirteen real bugs)

### 1. `hostRequirements` hard-failed `up`/`build` (spec violation)

deacon **refused to start** when the host didn't meet `hostRequirements`
(e.g. `storage: "32gb"` on a smaller disk): `Host requirements not met: Storage:
… required, … available`. A realistic VS Code config commonly declares
`hostRequirements`, so this is a complete bomb before any image is even pulled.

- The containers.dev spec is explicit these are **advisory**: *"you will be
  presented with a **warning** if the requirements are not met"* — not a refusal.
- The reference CLI only **parses/merges** `hostRequirements` into output
  metadata (the `NV()` reducer); it has **no enforcement** anywhere. VS Code is
  the same.

**Fix:** unmet requirements now **warn and proceed** (advisory) in both `up` and
`build`. `--ignore-host-requirements` is retained as a back-compat
warning-suppressor (downgrades the warning to a debug log). Verified end-to-end:
deacon now proceeds through image pull + feature install exactly like the
reference. (`crates/core/src/host_requirements.rs`, `up`/`build` call sites.)

### 2. `remoteEnv` baked into `docker create` broke container startup

`remoteEnv` was applied as a container-**creation** `--env`. For the canonical
PATH-append idiom `remoteEnv.PATH = "${containerEnv:PATH}:/custom/bin"`, the
`${containerEnv:PATH}` reference is unresolvable at creation, so the **literal
template** became the container's `PATH`. That dropped `/usr/local/bin`, so the
image entrypoint couldn't be found and the container **failed to start**:

```
exec: "docker-entrypoint.sh": executable file not found in $PATH
```

Per spec, `remoteEnv` defines the *remote environment* for commands run inside
the container (lifecycle/`exec`), not container-creation env — and deacon already
applies it at exec time via `build_effective_env`. The create-time loop was both
redundant and harmful.

**Fix:** removed the `remoteEnv` loop from `create_container`; only
`containerEnv` is baked at creation. Regression test
`test_remote_env_not_baked_into_create_args`. Verified end-to-end: the realistic
`node-ts` config now does a full clean `up` (container runs, correct image PATH,
postCreate succeeds). (`crates/core/src/docker.rs`.)

### 3. `build.args` (and the rest of `build`) were never variable-substituted

A config with `build.args.FROM_LOCAL_ENV = "${localEnv:USER}"` passed the
**literal template** straight to `docker build` — the build received
`${localEnv:USER}` instead of the resolved value, breaking arg-driven builds.
`apply_variable_substitution` covered `image`, `mounts`, `containerEnv`, etc. but
never touched the `build` object.

**Fix:** recursively substitute the whole `build` JSON value (args / dockerfile /
context / target / cacheFrom) in both `apply_variable_substitution` and the
`_advanced` variant. Unit test `test_substitution_covers_build_args`; verified
end-to-end (a build arg `${localEnv:USER}` now reaches the image as `vscode`).
(`crates/core/src/config.rs`.)

### 4. `${containerWorkspaceFolder}` left literal when `workspaceFolder` is set

`containerEnv.APP_DIR = "${containerWorkspaceFolder}"` with `workspaceFolder:
/srv/app` left the literal template; the reference resolves it to `/srv/app`.

**Fix:** when the config sets an explicit (literal) `workspaceFolder`, seed the
substitution context's `container_workspace_folder` from it (that *is* the
container workspace folder, and it's correct for `up` too). When unset, we still
defer to the container-aware pass during `up` so we never bake a wrong default.
Unit test `test_container_workspace_folder_seeded_from_explicit_workspace_folder`.
(`crates/core/src/config.rs`.)

### 5. `remoteEnv` `${containerEnv:VAR}` left literal at exec time

A remoteEnv value like `"${containerEnv:PATH}:/custom/bin"` (the canonical
PATH-append) is unresolvable at config-load (no container) and survived as a
literal; `build_effective_env` then exported the literal template into the
remote environment. **Fix:** resolve `${containerEnv:VAR}` against the probed
container env when building the effective exec/lifecycle environment. Unit test
`test_build_effective_env_resolves_container_env_refs`.
(`crates/core/src/container_env_probe.rs`.)

### 6. `forwardPorts` static-published + `appPort` bound to `0.0.0.0`

The reference publishes **only `appPort`** via `docker -p`, binding numeric
ports to **`127.0.0.1`** (loopback); `forwardPorts` are forwarding *hints* it
never binds. deacon was statically publishing **`forwardPorts` too** (so a real
config declaring common ports — 3000/8080/9229… — would **bomb `up` on a
host-port conflict** the reference shrugs off) **and** binding bare ports to
**`0.0.0.0`** (exposed on all interfaces, not loopback).

This was the "exception" wrongly excused via a deacon-authored SPEC; re-judged
against the reference it is a real divergence (and an over-exposure). **Fix:**
publish only `appPort`; numeric → `127.0.0.1:N:N`, string → verbatim;
`forwardPorts` are never statically bound (they are still forwarded by the
`--auto-forward` daemon). Tests `test_port_publish_args_excludes_forward_ports`,
`test_port_spec_to_publish_arg_variants`. (`crates/core/src/docker.rs`.)

### 7. Feature `containerEnv` with `${PATH}` clobbered the container PATH (`sh` 127)

The reported real-world break: a **standard Ruby devcontainer + Node.js 22 as a
feature**. The node feature's `containerEnv.PATH =
"/usr/local/share/nvm/current/bin:${PATH}"` was baked into `docker create -e
PATH=…${PATH}` **unexpanded** — Docker doesn't expand `${PATH}` there, so the
literal became the container PATH, dropping `/usr/local/bin`, `/bin`, … Then
**every** exec failed: `exec: "sh": executable file not found in $PATH` (exit
127) — env probe, user mapping, and `postCreate` all bombed.

The feature/base image ENV already carries the correctly-expanded PATH (Docker
expanded `${PATH}` at image-build time). **Fix:** at container creation, skip any
`containerEnv` value still containing an unexpanded `${...}` shell reference and
let the image's ENV stand (catches the feature `combined_env` re-bake and any
metadata-label re-introduction). Regression test
`test_container_env_with_shell_ref_not_baked_into_create_args`. Verified
end-to-end: the Ruby + Node-22 config now does a full clean `up` (`ruby 3.4.4`,
`v22.22.3`). (`crates/core/src/docker.rs`, fixture `ruby-node-feature/`.)

### 8. Lifecycle bash ran non-interactive → feature tools (node) not on PATH

`bare debian:bookworm-slim + node:22 feature` (and any base that doesn't already
put the tool on PATH): `postCreate` failed `node: command not found` (exit 127).
The node feature hooks nvm into the **interactive** bash startup
(`/etc/bash.bashrc`, which early-returns when `$PS1` is unset), but
`get_shell_command_for_lifecycle` ran bash as `-lc` (login, **non-interactive**)
— so nvm was never sourced. zsh already used `-l -i -c`; bash didn't. Proven on
the reference's own container: `bash -lc 'node'` → not found, `bash -lic 'node'`
→ `v22.22.3`. The reference runs lifecycle in an interactive-login shell.

**Fix:** bash lifecycle commands now use `-l -i -c` (login + interactive), like
zsh and the reference. Test `test_get_shell_command_bash_login_interactive`.
Verified end-to-end: bare-base + node-22 now does a clean `up` (`v22.22.3`).
(`crates/core/src/container_env_probe.rs`, fixture `bare-base-node-feature/`.)

Note on the dependency question: the node feature declares
`installsAfter: [common-utils]` (a *soft ordering* hint), **not** `dependsOn`.
Neither deacon nor the reference auto-installs `installsAfter` targets —
verified the reference's bare-base container has no common-utils (only the
`node` user the node feature itself created). So this was a shell-mode bug, not
a missing transitive dependency.

### 9. Transitive `dependsOn` (hard deps) not auto-installed

The reference auto-fetches and installs a feature's `dependsOn` targets even when
undeclared; deacon instead returned a hard error if a `dependsOn` key wasn't
already in the declared set. So a config declaring only a feature that
hard-`dependsOn` another failed under deacon while succeeding under the
reference. (Distinct from `installsAfter`, a soft ordering hint that is
correctly *not* auto-installed.)

**Fix:** in `resolve_and_stage_features`, compute the transitive `dependsOn`
closure after downloading the declared features — fetch each missing dependency
(OCI or local), apply the options from its `dependsOn` entry, and add it to the
feature set before resolving the install order. A user's own declaration of a
dependency still wins (its options are kept), and the closure terminates on
cycles (which the resolver also detects). Test
`parse_feature_options_handles_object_and_non_object`; fixture
`dependson-autoinstall/` (a local feature that `dependsOn` node:22). Verified
end-to-end: declaring only the local feature auto-installs node (`v22.22.3`).
(`crates/deacon/src/commands/up/features_build.rs`.)

### 10. `dependsOn` auto-install across `run-user-commands` + `read-configuration`

Fix #9 covered the `up`/`build` install path. The shared `resolve_features_ordered`
(`run-user-commands`) and read-configuration's `--include-features-configuration`
path still hard-errored on an undeclared `dependsOn`. **Fix:** the transitive
`dependsOn` closure is now applied in the shared resolver (via an extracted
`resolve_one_feature`) and reused by read-configuration, so all three feature
paths behave identically and match the reference. Unit test
`auto_installs_transitive_depends_on`.
(`crates/deacon/src/commands/shared/feature_resolver.rs`,
`crates/deacon/src/commands/read_configuration.rs`.)

### 11. read-configuration mis-anchored local features under auto-discovery

`read-configuration --workspace-folder <ws> --include-features-configuration`
(no `--config`) anchored local feature paths (`./feat`) to the **workspace
folder** instead of the **discovered** `.devcontainer/` config dir, failing with
`Local feature path './feat' … is not accessible` for any config under
`.devcontainer/` — while `up` (which threads the resolved config path) worked.
**Fix:** anchor to the resolved/discovered config path's directory, falling back
to the workspace folder only when neither a `--config` arg nor a discovered path
is available. Integration test `test_local_feature_anchors_to_discovered_config_dir`.
(`crates/deacon/src/commands/read_configuration.rs`.)

### 12. `featuresConfiguration` output grouped by registry instead of install order

`read-configuration --include-features-configuration` discarded the install
plan and grouped features by registry (alphabetical), so the `featureSets` order
diverged from the reference, which emits **one set per feature in install order**
(a feature's dependencies first). For the `dependson-autoinstall` fixture the
reference gives `[node, needs-node]`; deacon gave `[needs-node, node]`.

**Fix:** drive `featureSets` from the resolved installation plan — one set per
feature, in topological install order. Now matches the reference exactly
(`[node, needs-node]`, 2 sets). Integration test
`test_features_configuration_emitted_in_install_order`.
(`crates/deacon/src/commands/read_configuration.rs`.)

### 13. `featuresConfiguration.sourceInformation` was minimal

deacon emitted `{ type: "oci", registry }` per set; the reference carries full
per-feature source info. **Fix:** `sourceInformation` now matches the reference
byte-for-byte — for OCI features `{ type: "oci", manifest, manifestDigest,
featureRef: { id, owner, namespace, registry, resource, path, version, tag },
userFeatureId, userFeatureIdWithoutVersion }` (the **manifest is fetched and
emitted in full**, config + layers + the embedded `dev.containers.metadata`
annotation, with the raw-body `sha256:` digest), and for local features
`{ type: "file-path", resolvedFilePath, userFeatureId }`. Verified field-by-field
against the reference (incl. the byte-equal manifest). Integration test asserts
the `file-path` shape. (`crates/deacon/src/commands/read_configuration.rs`.)

### 14. `up` reported a bare `remoteWorkspaceFolder: "/workspaces"`

For an image config without an explicit `workspaceFolder`, `up` reported the
bare `/workspaces` instead of the spec default
`/workspaces/${localWorkspaceFolderBasename}`. Verified against the reference
(`devcontainer up` on a TempDir): it reports `/workspaces/<basename>`. **Fix:**
a shared `default_remote_workspace_folder` helper now mirrors the actual
bind-mount target built in `Docker::create_container` (basename of the
mount source, i.e. the git root under `--mount-workspace-git-root`), used by
both the traditional (`container.rs`) and compose (`compose.rs`, both reconnect
and fresh paths) flows. Unit tests cover the basename / explicit / no-basename
cases; verified end-to-end (`deacon up` now reports `/workspaces/<basename>`,
matching the reference). (`crates/deacon/src/commands/up/{helpers,container,compose}.rs`.)

### 15. `${containerWorkspaceFolder}` leaked literal in read-configuration (no `workspaceFolder`, no container)

Fix #4 resolved `${containerWorkspaceFolder}` only when the config set an
explicit `workspaceFolder`. The residual case — `read-configuration` with no
container and no `workspaceFolder` (`universal-jsonc`) — still emitted the
literal `${containerWorkspaceFolder}`. The shared loader leaves it unresolved on
purpose so `up`'s container-aware pass can fill the real mount target, so the
fix is a **read-config-only seam**: in the no-container path, re-substitute the
output config with `containerWorkspaceFolder` seeded to the host workspace path
(the same value as `${localWorkspaceFolder}`), matching the reference. Verified
against the reference (identical `containerEnv`) and via the Tier-1 differ
(`universal-jsonc` now ✅ identical). Integration test asserts
`containerWorkspaceFolder == localWorkspaceFolder` with no explicit
`workspaceFolder`. (`crates/deacon/src/commands/read_configuration.rs`.)

### 16. read-configuration eagerly merged `extends` (raw-vs-merged output shape)

The reference CLI's read-configuration `configuration` is the **raw** entry
config with `extends` preserved (a single target as a string) — it defers the
extends merge to `up`/`mergedConfiguration`. deacon eagerly merged the extends
chain via the shared loader (correct for `up`, but it leaked base values into
the output, dropped `extends`, and merged `forwardPorts`). **Fix:** when the
entry file declares `extends`, `read-configuration` loads and substitutes the
single entry file on its own for the output `configuration` field, leaving the
merged `config` (used for `featuresConfiguration`/`mergedConfiguration`)
untouched. Also added a custom `extends` serializer so a single target emits as
a bare string (`"./base.json"`) and `extends` is omitted (never `null`) when
unset, matching the reference. Verified via the Tier-1 differ (`extends-child`
now ✅ identical; **all 23 configs identical, 0 divergences**). Tests cover the
raw-output shape and the string/array/skip serialization.
(`crates/core/src/config.rs`, `crates/deacon/src/commands/read_configuration.rs`.)

All Tier-1 read-configuration divergences against the reference CLI are now
resolved.

## Round 2 — `mergedConfiguration` sweep (`--include-merged-configuration`)

A second differ compares the `mergedConfiguration` block (deacon vs reference)
across the corpus, after the same null/empty normalization. Three real,
Docker-free divergences fixed:

### 17. `mergedConfiguration` leaked raw `hostRequirements` + `null` init/privileged

The reference's merged shape normalizes `hostRequirements.memory`/`.storage` to
a **byte-count string** (binary units): `"8gb"` → `"8589934592"` (`cpus` stays
numeric; the top-level `configuration` block keeps the raw authored string).
It also always materializes `init`/`privileged` as booleans (default `false`,
per upstream `imageMetadata.some(e => e.init)`). deacon leaked the raw `"8gb"`
string and emitted `null` for the booleans. **Fix:** a `mergedConfiguration`
finalization pass (`normalize_merged_configuration_shape`) reusing the core
`ResourceSpec::parse_bytes` binary-unit semantics. Verified byte-identical on
`node-ts` (hostRequirements) and across all configs (init/privileged).
(`crates/deacon/src/commands/read_configuration.rs`.)

### 18. Feature `containerEnv` over-merged into `mergedConfiguration`

deacon folded a feature's `containerEnv` (e.g. `GOROOT`/`GOPATH`, `NVM_*`,
`DOTNET_*`) into `mergedConfiguration.containerEnv`. The reference does **not**:
per upstream `imageMetadata.ts`, a feature's image-metadata entry omits env —
feature env is realized by the feature's own install step (baked into the
image), not surfaced via the `devcontainer.metadata` merge. So the merged shape
carries only the base config's (and image-label) env. **Fix:** stop folding
feature `containerEnv`; feature `mounts`/`customizations`/`init`/`privileged`/
`capAdd`/`securityOpt` are still folded (those *are* in the upstream metadata
entry). Base config env survives unchanged. All feature configs (go/python/
ruby/dotnet) now show `0` deacon-only merged env. Hermetic local-feature
regression test. (`crates/deacon/src/commands/read_configuration.rs`.)

### 19. Image-metadata `customizations` emitted in reversed order

The merged `customizations.<tool>` array prepended each image
`devcontainer.metadata` entry with `insert(0, …)` in a forward loop, which
**reversed** the image entries — scrambling the array relative to the
reference's `[...image, ...features, config]` (image entries in label order).
**Fix:** a pure `ordered_customizations_entries` helper concatenating the
per-contributor buckets in forward order, used by both image-metadata merge
branches. With the `typescript-node` image present, `node-ts`
`mergedConfiguration.customizations.vscode` is byte-identical (all 7 contributor
entries in matching order). Unit tests cover the ordering invariant.
(`crates/deacon/src/commands/read_configuration.rs`.)

## Round 3 — Tier-2 (Docker) `up` sweep

A Docker-level differential: copy each corpus config to a TempDir **outside** the
repo, run `deacon up` and `devcontainer up`, compare exit/outcome/remoteUser and
container-running state, then tear both down (deacon's container is removed
**before** the reference runs — otherwise the two collide on `appPort`/published
host ports, a false positive). Lifecycle ordering, object-form parallel commands
(`lint`/`deps`), array/string forms, feature install, mounts, user-mapping,
init/privileged, and `appPort` bindings (`ports-mixed`: deacon and reference emit
byte-identical `-p` host bindings) all reach parity. Three real bugs:

### 20. `up` Dockerfile-build + features: bare `sha256:` digest as `FROM` (BuildKit 404)

For a config with `build.dockerfile` (no `image`) **and** `features`, `up` first
builds the user Dockerfile via `docker build -q`, which yields a bare
`sha256:<digest>` image ID, and stored that verbatim as `config.image`. The
feature-layering stage then emitted `ARG _DEV_CONTAINERS_BASE_IMAGE=sha256:<digest>`
+ `FROM ${_DEV_CONTAINERS_BASE_IMAGE}` — and BuildKit resolves a **bare digest**
as a remote `docker.io/library/sha256:...` repository → `pull access denied` /
404. So **every** Dockerfile+features `up` failed at feature build, while the
reference (which tags its build) succeeded. **Fix:** `build_image_from_config`
tags the freshly-built image with a deterministic real `repo:tag`
(`deacon-build:<digest12>`, mirroring `deacon build`'s base tag) and returns the
tag, so the downstream `FROM` resolves to the local image. With no features the
bare digest also worked for `docker create`, so only the features path was
affected. Verified end-to-end: deacon now builds + layers features and the
container carries both the Dockerfile-stage markers and the feature marker.
Unit tests cover the digest→tag derivation; a hermetic docker integration test
(`integration_up_dockerfile_features`) guards the full path (reverting the fix
makes it fail with the 404). Also corrected the `dockerfile-build` fixture, which
was invalid two ways (`remoteUser: node` + `node --version` though node is never
installed — both CLIs failed identically on it); it now uses `vscode` +
`git --version` (the git feature it installs), reaching parity-success.
(`crates/deacon/src/commands/up/image_build.rs`,
`crates/deacon/tests/integration_up_dockerfile_features.rs`.)

### 21. Compose: `dockerComposeFile` resolved against the workspace folder, not the config dir

deacon resolved `dockerComposeFile` paths relative to the **workspace folder**,
so the standard `.devcontainer/docker-compose.yml` layout failed at `up` with
`open <workspace>/docker-compose.yml: no such file or directory`. The spec
("relative to the devcontainer.json file") and the reference both resolve them
relative to the **directory containing devcontainer.json** (`.devcontainer/` for
the nested layout) — confirmed by the reference's
`-f <ws>/.devcontainer/docker-compose.yml`. **Fix:** thread the config dir into
`ComposeManager::create_project` and resolve relative compose files against it,
while keeping the project name + working dir on the workspace folder. Volume
`..:/workspace` still resolves to the workspace root (compose's project dir
defaults to the first `-f` file's dir = `.devcontainer/`), so the bind mount
matches the reference byte-for-byte (`<ws> -> /workspace`). Threaded through `up`,
`exec`, `run-user-commands`, and `build` compose paths. Absolute compose paths are
left un-rebased. Unit tests cover config-dir resolution + absolute passthrough.
(`crates/core/src/compose.rs` + the four command call sites.)

### 22. Compose: `runServices` listing the primary service produced duplicate YAML keys

The feature/keep-alive injection override emits a full block for the primary
service, then a labels-only block for **every** `runServices` entry. When
`runServices` lists the primary service alongside the others (e.g.
`["app","db"]` — the common form), `app:` was emitted twice, so
`docker compose` rejected the piped override with `mapping key "app" already
defined`. Every compose config with features (or a non-default
`overrideCommand`) plus a primary-inclusive `runServices` failed. **Fix:** skip
the primary service in the `runServices` label loop (it already has a full
block). Verified end-to-end: `compose-postgres` (compose + `github-cli` feature +
`runServices:[app,db]`) now brings up both services with `gh` installed, matching
the reference. Unit test asserts the primary + secondary services each appear
exactly once. (`crates/core/src/compose.rs`.)

### 23. Compose project name scheme differed from the reference

For the same workspace folder deacon derived the compose `--project-name` as a
sanitized `<folder>` (invalid chars → `-`, no suffix) while the reference uses
`<folder>_devcontainer`. The two also sanitized differently: the reference
**strips** every character outside `[a-z0-9_-]` then lowercases (`Foo.Bar-1` →
`foobar-1`, `my proj` → `myproj`), whereas deacon replaced them with `-`
(`my-proj`). Both isolate correctly and deacon's `up`/`exec`/`down` were
self-consistent, but the project differed from VS Code / reference-built projects
for the same folder. **Fix:** `derive_project_name` now mirrors the reference —
strip `[^a-z0-9_-]`, lowercase, append `_devcontainer` — keeping the explicit
`.env` `COMPOSE_PROJECT_NAME` override verbatim (no suffix). For degenerate
folders where the reference emits an invalid name and docker compose then fails
(leading `-`/`_`, or empty after stripping), deacon trims leading separators /
falls back to a `deacon` stem so it stays robust. Verified end-to-end: for
workspace `My.App-1` both deacon and the reference now produce
`myapp-1_devcontainer`. Unit tests cover the strip/lowercase/suffix rules and the
degenerate fallbacks. (`crates/core/src/compose.rs`.)

### Tier-2 divergences that are NOT deacon bugs

- **`bare-base-node-feature`: deacon succeeds, reference fails.** Config is
  `debian:bookworm-slim` + `ghcr.io/devcontainers/features/node` + `remoteUser:
  node` (no `common-utils`). deacon's node-feature install **creates** the `node`
  user (`node:x:1000:1000`, node v22 on PATH) and `postCreate` runs as `node`.
  The reference installs the same feature but its image has **no** `node` user
  (`getent passwd node` → none; node binary present at
  `/usr/local/share/nvm/...`), so its `up` dies with `unable to find user node`.
  deacon is **more permissive** here (materializes the declared `remoteUser` via
  the feature), not silently falling back — verified the user genuinely exists in
  deacon's container. Not a deacon defect.
- **`extends-child`: deacon resolves `extends` at `up`/merged; reference errors.**
  Raw `read-configuration` is parity (both emit the raw child with `extends`
  preserved — fix #16). But `deacon up` and `deacon read-configuration
  --include-merged-configuration` fully resolve the chain (`image` =
  `base:bookworm`, `containerEnv` = `{CHILD, BASE}`, `forwardPorts` =
  `[3000,4000]`), whereas the reference CLI v0.87.0 **errors** (`up`: "missing one
  of image/dockerFile/dockerComposeFile"; merged read-config: exit 1, empty
  stdout). `extends` is a deliberate deacon capability beyond the reference, not a
  regression — do not "fix" by dropping support.

## Round 4 — feature install ordering

A Docker differential on feature install order using local order-recording
features (each `install.sh` appends its id to a shared file, read back from the
container) plus the official-feature `feature-order` corpus config.

### 24. Partial `overrideFeatureInstallOrder` was ignored

`apply_override_order` only honored `overrideFeatureInstallOrder` when it listed
**every** resolved feature (`override_set == sorted_set`); for a **partial**
override it fell back to a pure topological sort, dropping the override entirely.
So `feature-order` (override `[common-utils, node, python]`, with
`docker-in-docker` unlisted) installed `common-utils, docker-in-docker, node,
python` (alphabetical), while the reference installs `common-utils, node, python,
docker-in-docker` — listed features in override order first, the unlisted one
last. Confirmed with isolated local features: partial override `[charlie]` over
`{alpha,bravo,charlie}` gave deacon `alpha,bravo,charlie` vs reference
`charlie,alpha,bravo`.

**Fix:** replaced the topological-sort + `apply_override_order` pair with a single
priority-aware Kahn's sort. Among the ready set each round (nodes whose
`installsAfter` deps are all emitted), the next feature is chosen by
`(override position, lexicographic id)` — listed features by their override
position, then unlisted features alphabetically. `installsAfter`/`dependsOn`
dependencies are always respected (the override is a tie-break among independent
features, never a way to violate a dependency). The no-override path already
matched the reference (parallel-level flattening = the same lexicographic
rounds), so it is unchanged. Verified end-to-end against the reference for:
no-override tie-break (alphabetical), partial override, and `installsAfter` +
partial override (`w` first, then `y, x` per override, then unlisted `z`). Unit
tests cover the partial-override and dependency-interaction cases.
(`crates/core/src/features.rs`.)

## Round 5 — `userEnvProbe` modes

Docker differential: a Dockerfile sets `PROFILE_VAR` in `/etc/profile.d` (login
shells) and `BASHRC_VAR` in `~/.bashrc` (interactive shells); `postCreateCommand`
records which the lifecycle env captured, per `userEnvProbe` mode.

### 25. Lifecycle commands ignored `userEnvProbe` (always login+interactive)

deacon ran every lifecycle command through a **login + interactive** shell
(`bash -l -i -c`) regardless of `userEnvProbe`, so `none`/`loginShell`/
`interactiveShell` all behaved like `loginInteractiveShell` — sourcing both
`/etc/profile` and `~/.bashrc`. The reference varies the shell by mode. Full
matrix (before fix deacon was `P=[fromprofile] B=[frombashrc]` for **all** four):

| mode | reference | deacon (before) |
|------|-----------|-----------------|
| `none` | `P=[] B=[]` | `P=[…] B=[…]` ✗ |
| `loginShell` | `P=[fromprofile] B=[]` | `P=[…] B=[…]` ✗ |
| `interactiveShell` | `P=[] B=[frombashrc]` | `P=[…] B=[…]` ✗ |
| `loginInteractiveShell` | `P=[fromprofile] B=[frombashrc]` | ✓ |

**Fix:** `get_shell_command_for_lifecycle` now takes the `userEnvProbe` mode and
maps it to the shell's login/interactive flags — `none` → plain `sh -c`,
`loginShell` → `-l -c`, `interactiveShell` → `-i -c`, `loginInteractiveShell` →
`-l -i -c`. The default (`loginInteractiveShell`) is unchanged, so the
feature-installed-tools-on-PATH behavior (nvm/node via interactive rc) is
preserved — confirmed by the docker `integration_feature_lifecycle` suite. After
the fix all four modes match the reference byte-for-byte. Unit tests cover each
mode's flags. (`crates/core/src/container_env_probe.rs`,
`crates/core/src/container_lifecycle.rs`.)

## Round 6 — `--secrets-file`

### 26. `--secrets-file` secrets leaked in plaintext in lifecycle output

With a secret injected and `postCreateCommand: echo "the secret is $MY_SECRET"`,
the reference prints `the secret is ********` (redacted) while deacon printed
`the secret is topsecret123` (**plaintext leak**). deacon redacted lifecycle
*command strings* (for progress events) but streamed the command *output* through
`CliDocker::exec` via a raw `tokio::io::copy` with no redaction — and the loaded
secrets were registered only in a local `SecretsCollection::redaction_registry`,
never the **global** registry the lifecycle redaction consults. **Fix:** `up`
now registers the loaded secret values in the global registry (gated on
redaction being enabled, mirroring the `build` path), and the `stdout_to_stderr`
lifecycle stream is redacted line-by-line through `redact_if_enabled`. With no
secrets registered (or `--no-redact`) it's a no-op, so normal output is
unchanged. Verified: the secret value no longer appears in deacon's output (it's
replaced by the redaction placeholder), while `--no-redact` still surfaces the
raw value for debugging. Hermetic docker test
(`integration_up_secrets_redaction`) asserts both, and fails (leak) if the
stream fix is reverted. (`crates/deacon/src/commands/up/mod.rs`,
`crates/core/src/docker.rs`.)

### 27. `--secrets-file` did not accept the reference's JSON format

The reference CLI (and the containers.dev spec) expect `--secrets-file` to be a
flat **JSON** object (`{"MY_SECRET":"…"}`) — it rejects a `KEY=VALUE` `.env` file
with `Error: Invalid json data`. deacon parsed only `.env` and silently left
secrets unset when given a reference-format JSON file. **Fix:** `parse_secrets_file`
now accepts **both** — a leading `{` (after whitespace) selects JSON, otherwise
`KEY=VALUE`. The two are syntactically disjoint, so JSON support is purely additive
(no `.env` regression) and a deacon superset of the reference. Non-string JSON
values are coerced to their string form (number/bool literal, object/array as
compact JSON, null → empty); a `{`-leading file that isn't valid JSON fails fast
with a clear error (no silent fallback). Verified end-to-end: a JSON secrets file
injects + redacts under deacon exactly as under the reference, and the existing
`.env` path is unchanged. Unit tests cover JSON parsing, whitespace-led detection,
non-string coercion, and the invalid-JSON error. (`crates/core/src/secrets.rs`.)

## Verified non-bugs

- **`exec` env/user/workdir + `userEnvProbe` are at parity.** `deacon exec` matches
  the reference on `whoami`, `pwd` (= workspaceFolder), `containerEnv`, `remoteEnv`
  (incl. `${containerEnv:…}` substitution), and the probed login/interactive env;
  and it honors `userEnvProbe: none` (empty) vs the default (profile+bashrc) — exec
  uses probe-and-inject, so unlike the lifecycle path (fix #25) it was already
  correct.
- **`--additional-features` injection + ordering at parity.** A config feature plus
  a CLI-injected local feature install in the same order under deacon and the
  reference (`basef, extraf`); `./`-relative additional-feature paths resolve
  against the config dir for both.
- **Feature typed-option handling is at parity.** A local feature with `string`,
  `boolean`, and `enum` options, the user setting only one: deacon and the
  reference both apply the omitted options' **defaults**, stringify `boolean`
  to `true`/`false`, and pass an **invalid** enum value through unchanged
  (neither validates enum membership at install time). All cases byte-identical.
- **`build --image-name` resolves to the feature-extended image (single + multiple
  names).** `deacon build --image-name a:1 --image-name b:2` on an image+features
  config tags BOTH names with the layered image (feature marker present), matching
  the reference.
- **Feature `containerEnv` with `${...}` shell refs (incl. a *novel* PATH dir) is
  NOT applied — by deacon AND the reference.** Empirically tested a feature whose
  `containerEnv` adds `/opt/novel/bin:${PATH}`, `DERIVED=got-${NOVEL_VAR}`, and
  `PATHCOPY=${PATH}` on a bare `debian:bookworm-slim`. Both deacon and the
  reference produce identical results: plain values (`NOVEL_VAR=hello`) are set;
  every `${...}`-containing value is left **unset** (novel dir not on PATH,
  `DERIVED`/`PATHCOPY` empty). So fix #7's "skip `${...}` containerEnv at create"
  is exactly reference-correct — emitting them as image `ENV` (build-time
  expansion) would have *diverged* by setting values the reference leaves unset.
- `docker-in-docker` + `--init` on the heavy `typescript-node` image fails to
  keep the container alive **in both deacon and the reference** in this nested
  environment (dind needs `--privileged`; identical failure, identical container
  ID → identity parity). Environmental, not a deacon defect. The `node-ts`
  fixture drops dind/`--init` to stay a reliable green entry.
- A bind mount whose source path does not exist fails at `docker create`
  (`bind source path does not exist`) — docker-level, identical in the reference.
  (`mounts-bind-localenv` was adjusted to bind an existing path.) Confirms
  `${localWorkspaceFolder}`/`${localEnv:…}` are substituted in mount strings.
- `appPort` host-port already in use → `docker -p` bind conflict (environmental;
  `ports-mixed` uses uncommon ports). Note: after fix #6 `forwardPorts` are no
  longer bound, so they cannot cause this.
- **`mergedConfiguration` image-metadata only merges *locally-present* images.**
  The remaining `customizations.vscode` / `capAdd` / `containerUser` /
  `entrypoints` / `mounts` divergences in the round-2 sweep (go/python/dotnet/
  ruby/compose, `universal-jsonc`) are entirely because those base images are
  not pulled in the test environment. deacon's image-metadata fetch is
  best-effort **local-only** (it does not pull during a read-only command); the
  reference CLI pulls the image to read its `devcontainer.metadata` label. With
  the image present, deacon matches: `node-ts` (typescript-node pulled) is
  byte-identical after fix #19. This is a deliberate read-config design choice
  (no implicit network/pull), not a merge-logic bug. Compose configs
  additionally have no `config.image` to inspect at read-config time.

## Corpus (20 configs)

image+features (`node-ts`, `python-features`, `go-minimal`, `dotnet-mounts`,
`feature-order`), Dockerfile build (`dockerfile-build`, `build-args-subst`),
compose (`compose-postgres`, `compose-array`), jsonc/kitchen-sink
(`universal-jsonc`), lifecycle forms (`lifecycle-arrays`, `lifecycle-mixed`),
extends (`extends-child`), substitution (`containerenv-subst`, `name-subst`,
`workspacefolder-custom`), mounts (`mounts-bind-localenv`), ports (`ports-mixed`),
user mapping (`user-mapping`), security (`init-privileged`),
ruby + node-feature (`ruby-node-feature`), bare base + node-feature
(`bare-base-node-feature`).
