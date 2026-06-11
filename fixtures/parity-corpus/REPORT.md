# Parity corpus — findings

Oracle: `@devcontainers/cli` v0.87.0. deacon: this branch. 10 corpus configs.

## Fixed in this PR (two real `up` "bombs")

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

## Open follow-ups (found, not yet fixed)

- **`remoteEnv` `${containerEnv:VAR}` not resolved at exec time** (bug #2, same
  class as the read-config gap below). After fix #2 the container starts cleanly,
  but the remoteEnv PATH-append (`/custom/bin`) is silently dropped at exec (the
  literal is applied then overridden by the login shell). Non-bombing; needs
  `build_effective_env`/remoteEnv substitution to resolve `${containerEnv:…}`
  against the probed container env.
- **Tier-1 divergence A — `${containerWorkspaceFolder}` not substituted in
  `read-configuration` without a container.** deacon leaves the literal; the
  reference resolves it to the workspace path. (`universal-jsonc`.) Container
  exists → it resolves correctly during `up`; only the no-container read-config
  path leaks the literal.
- **Tier-1 divergence B — `extends` output shape.** The reference returns the
  *raw* child config with `extends` preserved (defers the merge to `up`); deacon
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
