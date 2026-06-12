# Parity corpus — findings

Oracle: `@devcontainers/cli` v0.87.0. deacon: this branch. 20 corpus configs.

Source of truth: the official containers.dev spec and the reference CLI's
behavior — not any deacon-authored spec doc.

## Fixed in this PR (seven real bugs)

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
reference. (`crates/core/src/host_requirements.rs`, `up`/`build` call sites,
`docs/subcommand-specs/up/SPEC.md`.)

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

## Open follow-ups (found, not yet fixed)

- **Transitive feature dependencies (`dependsOn`) not auto-installed.** deacon
  warns `Feature 'node' depends on 'common-utils' which is not in the feature
  set` and proceeds. Harmless when the base image already bundles the dependency
  (the devcontainers Ruby image bundles `common-utils`), but on a bare base a
  `dependsOn` feature would be skipped where the reference auto-installs it.
- **Fully reference-correct feature env** would emit feature `containerEnv` as
  image `ENV` (build-time `${PATH}` expansion) so a feature adding a *novel*
  PATH dir not already in the base image still lands. Fix #7 relies on the image
  ENV already carrying the value (true for the realistic features); the ENV-
  generation approach would close the novel-path gap.

- **Divergence A (residual) — `${containerWorkspaceFolder}` without an explicit
  `workspaceFolder`.** Now fixed for the common case (workspaceFolder set, fix
  #4). The residual case — no `workspaceFolder`, `read-configuration` with no
  container — still leaks the literal (`universal-jsonc`). The reference falls
  back to the host workspace path; the spec-correct value would be
  `/workspaces/<basename>`. Resolving it in the shared loader risks corrupting
  `up`'s container-aware pass, so deferred pending a read-config-only seam.
- **Divergence B — `extends` output shape.** The reference returns the *raw*
  child config with `extends` preserved (defers the merge to `up`); deacon
  eagerly merges via `load_with_extends` and drops `extends`. Functionally
  equivalent at `up`; differs only in read-config presentation.
- **Observation — `remoteWorkspaceFolder: "/workspaces"`** reported by `up` for
  image configs without an explicit `workspaceFolder` (spec default is
  `/workspaces/${localWorkspaceFolderBasename}`). Worth verifying against the
  reference's container workspace mount target.

## Verified non-bugs

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

## Corpus (20 configs)

image+features (`node-ts`, `python-features`, `go-minimal`, `dotnet-mounts`,
`feature-order`), Dockerfile build (`dockerfile-build`, `build-args-subst`),
compose (`compose-postgres`, `compose-array`), jsonc/kitchen-sink
(`universal-jsonc`), lifecycle forms (`lifecycle-arrays`, `lifecycle-mixed`),
extends (`extends-child`), substitution (`containerenv-subst`, `name-subst`,
`workspacefolder-custom`), mounts (`mounts-bind-localenv`), ports (`ports-mixed`),
user mapping (`user-mapping`), security (`init-privileged`),
ruby + node-feature (`ruby-node-feature`).
