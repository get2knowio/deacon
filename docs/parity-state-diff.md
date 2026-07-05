# Normalized observable-state parity differ

The `parity_state_diff` test binary compares the **resulting container state**
of `deacon up` against `@devcontainers/cli up` for the same input — not merely
that both launched. The bugs that bite are outcome divergences on a *successful*
launch: a missing mount (#266, #272), a dropped env var, a colliding project
name (#265). The differ snapshots each CLI's container via `docker inspect`,
normalizes away legitimate differences, and fails on anything left over.

- Differ implementation: `crates/deacon/tests/parity_utils.rs`
  (`StateSnapshot`, `snapshot_from_inspect`, `diff_states`,
  `classify_divergence`, `assert_state_parity` / `assert_snapshots_parity`).
- Pure-logic unit tests (fast loop, no Docker):
  `crates/deacon/tests/integration_state_diff.rs`.
- Docker fixtures (opt-in, gated): `crates/deacon/tests/parity_state_diff.rs`.

Run (triple-gated on `DEACON_PARITY=1` + Docker + `devcontainer` in PATH):

```bash
DEACON_PARITY=1 cargo test --test parity_state_diff -- --test-threads=1 --nocapture
```

Every subtracted divergence is printed as a `[state-diff]` line so the
normalization/allowlist is visible on each run.

## Fields compared

`mounts` (by destination → type + read-only), `env` (by key), `labels` (by key,
after namespace filtering), `exposed_ports` (set), `published_ports`
(`HostConfig.PortBindings`, set), `user`, `working_dir`.

Captured for debugging but **not** diffed: `entrypoint` / `cmd` (keep-alive
strategy differs by CLI), `networks` (compose-project-prefixed).

## Fixture matrix

`crates/deacon/tests/parity_state_diff.rs` exercises the differ across:

| Fixture | Axis exercised |
|---|---|
| single-container (image) | config bind mount, `containerEnv` |
| compose + local Feature | feature `containerEnv` (baked) + feature `mounts` (#272) |
| intra-deacon single-vs-compose | compose-drops-X vs single (no upstream needed) |
| default workspace mount | `workspaceFolder` mount-target default (#273) |
| Dockerfile build + `containerUser`/`remoteUser` | image-build path + non-root user (#274) |
| `appPort` | published-port parity (`HostConfig.PortBindings`) |
| mount variety | read-only bind + `tmpfs` mount |
| compose db sidecar + named volume | multi-service compose + compose-declared volume |

## Normalization (legitimate differences subtracted before diffing)

| What | Why it is not a divergence |
|---|---|
| Mount **sources** (bind host paths) | Each CLI runs in its own workspace temp dir; sources legitimately differ. Mounts are compared by destination + type + read-only. |
| Env noise keys: `PATH`, `HOME`, `HOSTNAME`, `TERM`, `container` | Present in every container / runtime-injected. |
| Label namespaces: `devcontainer.*`, `com.docker.*`, `desktop.*`, `dev.containers.*` | Per-CLI identity, metadata blob, compose bookkeeping, Docker Desktop. |
| Compose-project prefix on volume/network names | Volumes/networks are named after the (deliberately different) project name (#265). |
| `Config.User` empty vs `"root"` | Empty means "image default", which is root for these bases. A real non-root `remoteUser`/`containerUser` still diverges. |

## Intentional divergences (`KNOWN_INTENTIONAL_DIVERGENCES`)

Currently **empty** — every intentional deacon divergence (project name,
identity labels, keep-alive command, project-prefixed networks) is already
handled by normalization or by a field the differ does not compare. New entries
here must be justified; they are the reviewable record of where deacon is
knowingly different.

## Known gaps (`KNOWN_GAPS`) — open bugs, reported but not failing

Currently **empty**. [#272](https://github.com/get2knowio/deacon/issues/272)
(feature-contributed `mounts` dropped on deacon's compose path) was the only
global entry and is now **fixed**: `execute_compose_up` resolves features
before folding their `mounts` into `additional_mounts` (same `merge_mounts`
call the single-container path uses), so compose feature mounts now match
upstream (`crates/deacon/src/commands/up/compose.rs`).

[#274](https://github.com/get2knowio/deacon/issues/274) (`containerUser` not
reflected in `Config.User`) is also **fixed**: `create_container`
(`crates/core/src/docker.rs`) now passes `--user <containerUser>` at container
create time when `containerUser` is configured, mirroring the reference CLI's
`PV()`/`getContainerUser` resolution (`containerUser` alone — `remoteUser` is
never consulted for `Config.User`, only for exec/lifecycle time). Both fixtures
(`state_diff_compose_parity_with_feature_mount_gap`,
`state_diff_dockerfile_build_and_nonroot_user`) now assert full parity instead
of a tracked gap shape.

[#273](https://github.com/get2knowio/deacon/issues/273) (default
workspace-mount target) was **investigated and closed as an intentional,
characterized divergence** — see below; it never used a KNOWN_GAP tracking
entry (`state_diff_default_workspace_mount_target_divergence` characterizes
both CLIs' behavior independently rather than diffing them).

## Divergences discovered while building the differ

The differ surfaced these on its first runs (as test *failures*, before any
allowlisting):

1. **Default workspace-mount target — investigated, closed as intentional
   ([#273](https://github.com/get2knowio/deacon/issues/273)).** On the
   single-container path, with `workspaceFolder` set and no explicit
   `workspaceMount`, deacon mounts the workspace **at `workspaceFolder`**
   (`/workspace`), while the reference CLI mounts it at the spec default
   **`/workspaces/<basename>`** and uses `workspaceFolder` only as the working
   directory (`crates/core/src/docker.rs:2150-2162`).

   **Why deacon does NOT align to the reference here:** empirically, the
   reference CLI's own behavior in this exact shape is broken — mounting at
   `/workspaces/<basename>` while defaulting `remoteWorkspaceFolder` (and thus
   the exec/lifecycle working directory) to `workspaceFolder` means `/workspace`
   never exists in the container. Verified with `@devcontainers/cli` 0.87.0
   against `{"image": "debian:bookworm-slim", "workspaceFolder": "/workspace"}`
   (no `workspaceMount`): `devcontainer exec … pwd` fails with `OCI runtime exec
   failed: … chdir to cwd ("/workspace") set in config.json failed: no such
   file or directory`. Aligning deacon to the reference default would import
   this footgun. deacon's behavior — mounting at `workspaceFolder` so it
   actually exists — is the more robust choice and is kept as a deliberate,
   tested characterization (`state_diff_default_workspace_mount_target_divergence`),
   not a bug to fix.
2. **Compose workspace mount — NOT a parity gap (verified).** deacon's compose
   path only mounts the workspace root when `--workspace-mount-consistency` is
   passed, and ignores `workspaceMount` on compose
   (`crates/deacon/src/commands/up/compose.rs`) — but the **reference
   CLI behaves the same on compose**: a plain compose devcontainer yields *zero*
   workspace binds on both CLIs (the compose file is responsible for mounting the
   workspace). The differ's intra-deacon single-vs-compose fixture surfaces the
   internal single-vs-compose difference, which mirrors upstream's own; it is
   allowed (`mount:/workspace`) there with a comment, not filed.

The differ's own fixtures pin `workspaceMount` explicitly (single-container) so
divergence (1) does not mask real findings in the cross-CLI comparison.
