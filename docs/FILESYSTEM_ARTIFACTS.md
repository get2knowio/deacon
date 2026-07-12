# Filesystem Artifacts

This document enumerates every file and directory `deacon` writes outside of the
container itself, so you know what to expect, what is safe to delete, and what to
add to your project's `.gitignore`.

**Design goal (issue #280): keep the project clean.** Like the reference
DevContainers CLI, `deacon` writes almost nothing into your project. Machine-level
state (OCI cache, lifecycle markers, build cache, entrypoint wrappers, trust store,
port registry) lives in the **host user-data folder** (`~/.deacon/` by default,
override with the global `--user-data-folder`). The **only** artifact written into
your project is the spec-mandated feature lockfile — which is meant to be committed.

## Host user-data folder (`~/.deacon/` by default)

Machine-level state, never inside your project. Everything here is keyed so it
survives `down && up` where that matters (a stable per-workspace hash) and is safe
to delete when you want to reclaim space.

| Path | Written by | Purpose | Cleanup |
| --- | --- | --- | --- |
| `~/.deacon/cache/` | any command that pulls OCI features/images | Content-addressed OCI blob/feature/manifest cache (shared across all workspaces). Override with `DEACON_CACHE_DIR`. | Persistent; safe to delete anytime. |
| `~/.deacon/state/<workspace_hash>/[prebuild/]<phase>.json` | `up`, `build`, `run-user-commands`, `set-up` | Lifecycle-phase resume markers (`onCreate`, `postCreate`, …). Keyed by a stable workspace hash so they survive `down && up` (resumed runs skip completed phases). | Cleared by `up --remove-existing-container`; safe to delete. |
| `~/.deacon/build-cache/<workspace_hash>/<config_hash>.json` | `build` | Build metadata cache keyed by config hash, under a per-workspace subdir. | Auto-invalidated when the cached image disappears; safe to delete. |
| `~/.deacon/entrypoints/<workspace_hash>/entrypoint-wrapper.sh` | `up` (only when multiple feature entrypoints must be chained) | Wrapper script bind-mounted into the container; the stable path lets it survive container restarts. | Safe to delete when the container is gone. |
| `~/.deacon/trusted_workspaces.json` | `--trust-workspace-persist` | Workspace-trust allowlist for host-side hooks (`initializeCommand`). | Persistent. |
| `~/.deacon/settings.json` | host-CA / settings / profiles features | Read-only host settings: CA injection (`hostCa`), `browser`, plus `profiles` / `defaultProfile` / `mergeConfig`. Profile `mergeConfig` fragment paths (deep-overlaid onto the base config; the CLI `--merge-config` is the analogue, and `--override-config` instead *replaces* the base per #285) resolve relative to this file's folder (absolute paths accepted as-is). Loaded/resolved only — no write command yet (deferred to #198). | Persistent. |
| `~/.deacon/forwarded_ports.json`, `~/.deacon/forward_daemon_<id>.{pid,log}` | `up --auto-forward`, `down` | Host-global port-forward registry + per-container daemon markers/logs. | Reaped on `down` / daemon exit. |

## Project workspace

Only **one** artifact lands in your project:

| Path | Written by | Purpose | Commit it? |
| --- | --- | --- | --- |
| `.devcontainer/devcontainer-lock.json` (or `.devcontainer-lock.json` for a root `.devcontainer.json`) | `up`, `build`, `upgrade` | Spec-mandated feature lockfile pinning resolved feature versions/digests. | **Yes** — like `package-lock.json`, commit it for reproducible builds. |

This matches the reference DevContainers CLI, which also writes the lockfile
in-project and keeps everything else out (see the comparison below).

## Parity with the reference CLI

Verified empirically against `@devcontainers/cli` v0.87.0 (`up`/`build` on clean
projects, diffing the tree before/after):

| Artifact | Reference CLI | deacon |
| --- | --- | --- |
| Feature lockfile | `.devcontainer/devcontainer-lock.json` (in project) | same |
| Lifecycle markers | inside the container (`~/.devcontainer/.<phase>CommandMarker`) | host user-data folder (`~/.deacon/state/…`) |
| Build cache | Docker layer cache + image labels (not in project) | host user-data folder (`~/.deacon/build-cache/…`) |
| Entrypoint chaining | composed inside the image/container | host user-data folder (`~/.deacon/entrypoints/…`) |
| OCI cache | host-global (not in project) | host user-data folder (`~/.deacon/cache/`) |

`deacon` keeps lifecycle markers on the host (rather than in the container, as the
reference does) so resume state survives `down && up` — a deliberate deacon feature —
but it stores them in the user-data folder, not the project, so there is no stray
project pollution either way.

## Recommended `.gitignore` snippet

Because deacon no longer writes markers/build-cache/wrappers into your project,
you generally need **nothing** in `.gitignore` for deacon (and you should *keep*
`devcontainer-lock.json` tracked). If you used an older deacon version, or want a
belt-and-suspenders rule, add:

```gitignore
# Legacy deacon artifacts from versions < 0.2.0 (now under ~/.deacon/)
.deacon/
.deacon-temp-build/
.devcontainer-state/
.devcontainer/build-cache/
.devcontainer/.deacon/
```

A stray `.deacon/` in a repo from an old version is safe to delete; deacon warns
about it on the next run.
