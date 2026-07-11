# Filesystem Artifacts

This document enumerates every file and directory `deacon` writes outside of the
container itself, so you know what to expect, what is safe to delete, and what to
add to your project's `.gitignore`.

There are two locations: the **host user-data folder** (machine-level state,
never inside your project) and the **project workspace** (a few devcontainer
artifacts, mirroring the reference DevContainers CLI).

## Host user-data folder (`~/.deacon/` by default)

Machine-level state that is **not** written into your project. Override the root
with the global `--user-data-folder` flag where supported.

| Path | Written by | Purpose | Cleanup |
| --- | --- | --- | --- |
| `~/.deacon/cache/` | any command that pulls OCI features/images | Content-addressed OCI blob/feature/manifest cache (shared across all workspaces). Override with `DEACON_CACHE_DIR`. | Persistent; safe to delete anytime (re-populated on demand). |
| `~/.deacon/trusted_workspaces.json` | `--trust-workspace-persist` | Workspace-trust allowlist for host-side hooks (`initializeCommand`). | Persistent. |
| `~/.deacon/settings.json` | host-CA / settings features | Read-only host settings (e.g. CA injection). | Persistent. |
| `~/.deacon/forwarded_ports.json` | `up --auto-forward` | Host-global port-forward registry. | Reaped when daemons exit. |
| `~/.deacon/forward_daemon_<container_id>.{pid,log}` | `up --auto-forward` | Per-container port-forward daemon marker + log. | Reaped on `down` / daemon exit. |

> **Note:** earlier deacon releases cached under a project-local `.deacon/cache`
> (relative to the current working directory). That was a bug ([#280]) — the
> cache is workspace-agnostic and now lives in the user-data folder. If you have
> a stray `.deacon/` directory in a repo from an old version, it is safe to
> delete; deacon warns about it on the next run.

## Project workspace

These land in your project (the `--workspace-folder`, or its `.devcontainer/`
directory). They mirror the reference CLI's behavior.

| Path | Written by | Purpose | Commit it? |
| --- | --- | --- | --- |
| `.devcontainer/devcontainer-lock.json` (or `.devcontainer-lock.json` for a root `.devcontainer.json`) | `up`, `build`, `upgrade` | Spec-mandated feature lockfile pinning resolved feature versions/digests. | **Yes** — like `package-lock.json`, commit it for reproducible builds. |
| `.devcontainer-state/*.json` | `up`, `build`, `exec`, `run-user-commands` | Lifecycle-phase resume markers (`onCreate`, `postCreate`, …). Intentionally survive `down && up` so resumed runs skip completed phases. | No — gitignore. Cleared by `up --remove-existing-container`. |
| `.devcontainer/build-cache/*.json` | `build` | Build metadata cache keyed by config hash. | No — gitignore. |
| `.devcontainer/.deacon/entrypoint-wrapper.sh` | `up` (only when multiple feature entrypoints must be chained) | Persistent wrapper script bind-mounted into the container; must survive container restarts, so it is intentionally not removed by `down`. | No — gitignore. |
| `.deacon-temp-build/` | `build` (image-reference builds) | Transient Dockerfile build context. Removed automatically after the build (RAII-guarded, so it is also cleaned on error/interruption). | No — gitignore (only lingers after a hard `SIGKILL`). |

## Recommended `.gitignore` snippet

Add this to your project's `.gitignore` to exclude deacon's non-committed
project artifacts (keep `devcontainer-lock.json` tracked):

```gitignore
# deacon / DevContainers runtime artifacts
.devcontainer-state/
.devcontainer/build-cache/
.devcontainer/.deacon/
.deacon-temp-build/
# Legacy project-local cache from deacon < 0.2.0 (now in ~/.deacon/cache)
.deacon/
```

[#280]: https://github.com/get2knowio/deacon/issues/280
