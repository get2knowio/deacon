Vendored from [devcontainers/cli](https://github.com/devcontainers/cli) (MIT) at tag
`v0.87.0`, source path `src/test/configs/dockerfile-with-syntax`.

Adapted for this suite:

- base repinned from `mcr.microsoft.com/devcontainers/typescript-node:1-${VARIANT}` to
  `debian:${VARIANT}`, and `build.args.VARIANT` with it. The `# syntax=docker/dockerfile:1`
  directive on line 1 and the `ARG`-before-`FROM` shape — the whole point of the fixture —
  are unchanged.
- the two `features` entries dropped; one is a v1-style id that no longer resolves, and
  neither bears on how a CLI hands a syntax-directive Dockerfile to the builder.
- one `LABEL` added. Without an authored label the reference's image carries NO labels at
  all (`Config.Labels` is `null`) while deacon's carries its `org.deacon.configHash`, so the
  two sides differ at the `labels` NODE rather than at a key inside it — which the case's
  tolerance, scoped to one key, cannot reach, and which therefore reported as a stale
  tolerance rather than as the already-adjudicated bookkeeping-label extension. The label
  also does a second job: it is a value the frontend has to have processed the file to
  produce.
