Vendored from [devcontainers/cli](https://github.com/devcontainers/cli) (MIT) at tag
`v0.87.0`, source path `src/test/configs/image-containerEnv-issue`.

Adapted for this suite:

- the Compose service's image repinned to `debian:bookworm-slim` (V18; upstream names a
  multi-GB devcontainers image the fixture does not need).
- the service's bind mount changed from `../..` to `..`. Relative Compose paths resolve
  against the Compose file's directory, and under `parity/fixtures/` `../..` would mount
  the whole fixture tree.
- `workspaceFolder` changed from `/workspaces/cli` to `/workspaces`. Upstream's value is
  only reachable because its own checkout directory happens to be named `cli`, so it names
  a path that does not exist inside the container here — which `up` tolerates (it never
  cds there) but `exec` does not: both CLIs fail with `current working directory is
  outside of container mount namespace root` before the command runs. The mount target is
  `/workspaces`, so pointing `workspaceFolder` at it is the same directory upstream meant.
  This is what lets a case OBSERVE a `containerEnv` value through `exec` rather than only
  through `docker inspect`.

What the fixture is FOR is untouched: a `containerEnv` block of values that survive a
round trip only if nothing re-interprets them — `$$`-escaped `${…}` placeholders, embedded
double quotes, a literal `$`, a backslash, leading/trailing spaces, and a value that is
itself a shell command — declared on a Compose service that ALSO sets two of the same
names in its own `environment:`.
