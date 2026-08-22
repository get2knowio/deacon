Vendored from [devcontainers/cli](https://github.com/devcontainers/cli) (MIT) at tag
`v0.87.0`, source path `src/test/configs/compose-Dockerfile-alpine`.

Adapted for this suite:

- base images repinned to ones the fixture tree already caches (V18 — no `latest`).
- the two decoy users are PINNED and a third was added: upstream writes
  `adduser foo` then `adduser bar`, which takes whatever the base image hands out
  (1000 and 1001 on `alpine:3.19`) and puts `remoteUser: bar` at 1001. That makes the
  `updateRemoteUserUID` collision this fixture exists to reach depend on the HOST's uid:
  it fires on a uid-1000 dev container and is a silent no-op on a GitHub-hosted runner,
  whose uid is 1001 and therefore equals `bar`'s own. Measured on PR
  [#623](https://github.com/get2knowio/deacon/pull/623)'s CI run — the case agreed on
  every channel there while the defect was unfixed. `foo` is now pinned to 1000, `foo2`
  added at 1001, and `bar` moved to 1002, so the remap to the host uid collides on both
  plausible substrates and the case is a claim about deacon rather than about the runner.
  See [#618](https://github.com/get2knowio/deacon/issues/618).
