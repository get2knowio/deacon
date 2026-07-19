# Normalized observable-state parity differ

The `parity_state_diff` test binary compares the **resulting container state**
of `deacon up` against `@devcontainers/cli up` for the same input — not merely
that both launched. The bugs that bite are outcome divergences on a *successful*
launch: a missing mount (#266, #272), a dropped env var, a colliding project
name (#265). The differ snapshots each CLI's container via `docker inspect`,
normalizes away legitimate differences, and fails on anything left over.

- Differ implementation: `crates/parity-harness/src/normalize.rs`
  (`StateSnapshot`, `container_state` — snapshot from `docker inspect`,
  `diff_states` — ranked divergences, `Divergence`). This is the single,
  shared normalization/equivalence module for the whole parity harness
  (018-harden-parity-harness); there is no per-test copy.
- Pure-logic unit tests (fast loop, no Docker): `#[cfg(test)]` blocks in
  `crates/parity-harness/src/normalize.rs` plus the cross-runner equivalence
  suite `crates/parity-harness/tests/normalize_consistency.rs`. Both run in the
  default/`dev-fast` lanes (hermetic).
- Docker fixtures (live, oracle-gated): `crates/deacon/tests/parity_state_diff.rs`.

Run via the dedicated nextest profile (the single sanctioned entry point;
requires the pinned `@devcontainers/cli` oracle + Docker):

```bash
# whole certified surface + aggregated report
make test-parity
# or target just this binary
cargo nextest run --profile parity -E 'binary(=parity_state_diff)'
```

There is **no** `DEACON_PARITY=1` opt-in gate and no silent skip: a
missing/mismatched oracle, missing Docker, or a normalization failure fails the
test with a cause-specific message. Every subtracted divergence is printed as a
`[state-diff]` line so the normalization/allowlist is visible on each run, and
each CLI's raw `docker inspect`/stdout is preserved under `target/parity/raw/`.

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

## Characterized divergences (waiver records)

The former in-source `KNOWN_INTENTIONAL_DIVERGENCES` / `KNOWN_GAPS` const lists
are gone (018-harden-parity-harness, research D6). Every characterized divergence
— whether an intentional deacon choice or a reported-but-not-failing gap — is now
a **waiver record** under `fixtures/parity-corpus/waivers/`, loaded and enforced
by `crates/parity-harness/src/waiver.rs`
(`Waiver`/`WaiverSet`/`Scope`/`field_matches`). A waiver carries an `id`, a
`scope` (which case/field it applies to), an `expect` (the difference it
tolerates), a required `rationale`, and an `added` date. Divergence
classification in `parity_state_diff.rs` and `parity_observable_state.rs` runs
through this loader.

Waivers are **self-invalidating**: a record whose tolerated difference stops
reproducing fails the run as *stale* (`HarnessError::WaiverStale`), so the
reviewable record can never silently rot — delete or update it in the same PR
that changes the behavior. New entries must be justified; they are the reviewable
record of where deacon is knowingly different. The `waivers/` directory is empty
today (both former const lists were empty), so the harness currently expects full
state parity on every fixture.

Historical note (both were fixed before the const lists were retired):
[#272](https://github.com/get2knowio/deacon/issues/272)
(feature-contributed `mounts` dropped on deacon's compose path) is now
**fixed**: `execute_compose_up` resolves features
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
