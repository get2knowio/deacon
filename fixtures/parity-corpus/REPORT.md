# Parity corpus — findings

Oracle: `@devcontainers/cli` v0.87.0. deacon: this branch. 20 corpus configs.

## Fixed in this PR (four real bugs)

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

## Open follow-ups (found, not yet fixed)

- **`remoteEnv` `${containerEnv:VAR}` not resolved at exec time** (same class as
  divergence A below). After fix #2 the container starts cleanly, but a remoteEnv
  PATH-append (`/custom/bin`) is silently dropped at exec (the literal is applied
  then overridden by the login shell). Non-bombing; needs `build_effective_env`
  to resolve `${containerEnv:…}` against the probed container env.
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
- **`forwardPorts` published via static `-p` at create (divergence, intentional
  per SPEC §2.1).** deacon binds `forwardPorts`/`appPort` with `docker -p` by
  default (`--auto-forward` suppresses this for the loopback daemon). The
  reference treats `forwardPorts` as forwarding *hints* and never binds them, so
  a real config declaring common ports (3000/8080/9229…) makes deacon `up`
  **bomb on a host-port conflict** where the reference succeeds. Documented
  design, but a real-world UX risk worth revisiting (e.g. fail-soft on bind
  conflict for `forwardPorts`).
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
- `forwardPorts`/`appPort` host-port already in use → `docker -p` bind conflict.
  Environmental; see the `forwardPorts` divergence above. (`ports-mixed` uses
  uncommon ports.) The `127.0.0.1:HOST:CONTAINER` host-IP form is passed through
  to `-p` unchanged (loopback bind preserved, not silently widened to 0.0.0.0).

## Corpus (20 configs)

image+features (`node-ts`, `python-features`, `go-minimal`, `dotnet-mounts`,
`feature-order`), Dockerfile build (`dockerfile-build`, `build-args-subst`),
compose (`compose-postgres`, `compose-array`), jsonc/kitchen-sink
(`universal-jsonc`), lifecycle forms (`lifecycle-arrays`, `lifecycle-mixed`),
extends (`extends-child`), substitution (`containerenv-subst`, `name-subst`,
`workspacefolder-custom`), mounts (`mounts-bind-localenv`), ports (`ports-mixed`),
user mapping (`user-mapping`), security (`init-privileged`).
