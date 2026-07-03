# BuildKit `--progress=plain` fixtures (Part 1 / B1)

Captured logs used by the `ui::build_progress` parser unit tests. They are real
`docker buildx build --progress=plain` output (BuildKit) over a **deacon-shaped
feature-install Dockerfile**, so they exercise both the generic BuildKit grammar
(`#N [stage k/M] op`, `#N <elapsed> <msg>`, `#N DONE`, `#N CACHED`, `#N ERROR`,
the trailing `------` / `Dockerfile:NN` block) and deacon's feature-step marker
(`--mount=type=bind,from=dev_containers_feature_content_source,source=<id>_<level>,…`,
e.g. `source=node_0`, `source=ai-clis_0`).

## Files

- `feature_build_success.plain.log` — clean build of two features (`node_0`,
  `ai-clis_0`). The `node` feature exposes `npm` only via
  `containerEnv.PATH`, emitted as `ENV` lines between the two install steps
  (the #252 fix), so `ai-clis_0` finds `npm` and the build succeeds.
- `feature_build_failure.plain.log` — same two features **without** the
  `ENV PATH` lines, reproducing the original `npm: not found` (exit 127)
  failure. Contains the `#N ERROR`, the `------` failing-step echo, the
  `Dockerfile:NN` reference, and the `>>>` source-context block that
  `failing_step_log()` must isolate.

## Provenance / how to regenerate

Synthesized in the devcontainer (the `../hola` project is not present there).
The grammar — not the specific feature set — is what the parser consumes, and
the marker shape mirrors `DockerfileGenerator::generate_feature_install_command`
(`crates/core/src/dockerfile_generator.rs`), so these are faithful for parser
tests. A `node` + `ai-clis` build was reproduced with alpine as the base and
`#!/bin/sh` install scripts, built via:

```sh
docker buildx build --load --no-cache --progress=plain \
  --build-context dev_containers_feature_content_source=<feat-dir> \
  -f <Dockerfile> -t deacon-bkfix .
```

A hola-derived capture (real `node` + `ai-clis` feature set) can be added as an
additional case once B2's streaming sink makes a clean success capture trivial
(today `deacon` buffers the build via `.output()` and swallows it on success).
