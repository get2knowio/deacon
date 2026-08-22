Vendored from [devcontainers/cli](https://github.com/devcontainers/cli) (MIT) at tag
`v0.87.0`, source path `src/test/configs/image-with-git-feature`.

Adapted for this suite: base images repinned to ones the fixture tree already caches
(V18 — no `latest`).

**Second adaptation (#480 batch 8):** the base is
`mcr.microsoft.com/devcontainers/typescript-node:1-20-bookworm`, not the
`debian:bookworm-slim` batch 1 chose. Upstream pins
`mcr.microsoft.com/vscode/devcontainers/typescript-node:0-16-bullseye`, which is what makes
its `"remoteUser": "node"` resolvable; `debian:bookworm-slim` has no `node` user, so `up`
failed for a reason unrelated to anything the fixture is about, and the fixture could only
ever be READ. The new pin restores upstream's own image/user pairing and is already named
elsewhere in this fixture tree, so it costs no additional pull.
`case-readconfig-upstream-feature-no-version-tag` reads the changed line identically on both
sides; `case-up-upstream-feature-no-version-tag-installs` is the case the change unblocks.
