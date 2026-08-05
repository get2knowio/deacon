Vendored from [devcontainers/cli](https://github.com/devcontainers/cli) (MIT) at tag
`v0.87.0`, source path `src/test/configs/dockerfile-with-target`.

Adapted for this suite:

- the multi-stage Dockerfile's bases repinned (`alpine` → `alpine:3.19`,
  `mcr.microsoft.com/devcontainers/typescript-node:1-${VARIANT}` → `debian:${VARIANT}`),
  and `build.args.VARIANT` moved with it. The three stages and their ORDER — a decoy
  before `desired-image` and a decoy after it — are what the fixture is for, and are
  unchanged.
- `sudo tee` in the target stage replaced by a plain redirect (the repinned base has no
  `sudo`, and the RUN already executes as root).
- the two `features` entries dropped. One (`codspace/myfeatures/helloworld`) is a v1-style
  id that no longer resolves anywhere, and neither bears on the claim.
- the three lifecycle hooks dropped. This fixture backs a `build` case, which runs no
  hooks; keeping them would put a configuration pick into `devcontainer.metadata` and make
  the case red on the already-adjudicated label-whitespace choice instead of on
  `build.target` / `build.options`.
