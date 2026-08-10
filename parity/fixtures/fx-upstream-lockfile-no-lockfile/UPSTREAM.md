Vendored from [devcontainers/cli](https://github.com/devcontainers/cli) (MIT) at tag
`v0.87.0`, source paths
`src/test/container-features/configs/lockfile-no-lockfile` and its identically-shaped
sibling `src/test/container-features/configs/lockfile-frozen-no-lockfile`.

Upstream's `src/test/container-features/lockfile.test.ts` drives these two configs from
five tests, all of which share one precondition — **the workspace contains no lockfile** —
and differ only in the flags:

- `--no-lockfile prevents lockfile creation`
- `--frozen-lockfile fails when lockfile missing` (`Lockfile does not exist.`)
- `devcontainer up --frozen-lockfile fails when lockfile missing`
- `--no-lockfile and {--frozen-lockfile,--experimental-frozen-lockfile,--experimental-lockfile}
  are mutually exclusive`
- `read-only commands do not create a lockfile`

One fixture therefore serves them all; the two upstream directories differ in nothing but
their names.

Adapted for this suite:

- **Feature repinned** from `ghcr.io/codspace/features/{flower,color}` to
  `ghcr.io/devcontainers/features/git:1.3.2`. The `codspace` namespace is third-party
  content this project does not control and cannot pin — the reason the whole lockfile
  family was held out of #480 batch 1. Nothing any of these cases claims depends on WHICH
  Feature is declared: every claim is about whether a lockfile is written, read, or
  refused. `git:1.3.2` is an immutable published version already named by five other
  fixtures here, so its digest is stable and the prepull script already warms nothing new.
- **Base repinned** from `mcr.microsoft.com/devcontainers/base:ubuntu` to
  `debian:bookworm-slim`. Two reasons, both load-bearing:
  - `base:ubuntu` is one of the images this dev container's overlay2-on-btrfs setup fails
    to extract (see `docs/HANDOFF.md`), so a fixture on it cannot be measured locally.
  - `base:*` images set `remoteUser: vscode` in their `devcontainer.metadata`, which opts
    every `up` into the reference's uid-remap path and its `updateUID.Dockerfile-<version>`
    ENOENT race (#480, batch 1). **No case on this fixture claims anything about uid
    remapping**, so the base must stay one with no `remoteUser`. Do not "upgrade" it.
- The config keeps the ROOT `.devcontainer.json` form upstream uses. That is not
  cosmetic: it selects the dot-prefixed `.devcontainer-lock.json` lockfile name, which is
  the spec's naming rule (`devcontainer-lockfile.md`) and is what every case here
  observes.

What the fixture is FOR is untouched: an image-based configuration with exactly one
lockfile-eligible (OCI) Feature and no lockfile on disk.
