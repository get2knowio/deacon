Vendored from [devcontainers/cli](https://github.com/devcontainers/cli) (MIT) at tag
`v0.87.0`, source path `src/test/configs/updateUID`.

Adapted for this suite:

- base image repinned from `debian:latest` to `debian:bookworm-slim` (V18 — no `latest`).
- a `postCreateCommand` was added that writes `whoami` into the bind-mounted workspace.
  Upstream asserts the remapped uid by `exec`-ing `id -u` from its own test harness; a
  declarative case has no such side channel, so the same two facts — the hook ran AS
  `remoteUser`, and that user could write to a host-owned bind mount, which is only true
  once its uid has been remapped — are recorded in one marker file.
