Vendored from [devcontainers/cli](https://github.com/devcontainers/cli) (MIT) at tag
`v0.87.0`, source path `src/test/container-features/configs/lifecycle-hooks-advanced`.

Adapted for this suite:

- **base repinned to `debian:bookworm-slim`, and it must stay a base with no `remoteUser`.**
  See "Do not upgrade this base" below — this is not a size or speed choice.

Everything else is upstream's, because the shapes are the point: both Features AND the
configuration declare `postCreateCommand` in the OBJECT (parallel) form with one member a
string and the other an array; both Features declare `postStartCommand`/`postAttachCommand`
as arrays; and the configuration's own `postStart`/`postAttach` are shell strings containing
COMMAND SUBSTITUTION of binaries the Features install, so they succeed only if the Feature's
`install.sh` already ran and put its binary on the PATH the hook is given.

## Do not upgrade this base to `mcr.microsoft.com/devcontainers/base:bookworm`

It was that image until #480's CI run, and the swap is load-bearing.

`base:bookworm` sets `remoteUser: vscode` (uid 1000) in its `devcontainer.metadata`, so on
any host whose uid is not 1000 BOTH CLIs perform a uid remap. The reference stages its
generated remap Dockerfile at a path scoped only by its own version —
`/tmp/devcontainercli-runner/updateUID.Dockerfile-<version>` — and renames a
timestamp-suffixed temp file onto it, so two concurrent reference `up` runs that both remap
race on that rename and one loses with
`ENOENT: no such file or directory, rename '…-<timestamp>' -> '…-0.87.0'`. The
`docker-shared` driver runs at concurrency 4. The race reproduced only in CI: a dev
container whose host uid is 1000 matches `vscode` exactly, so no remap happens there and it
is invisible locally.

This fixture claims parallel/array hook forms and Feature-installed binaries reachable from
a configuration hook. It does not claim anything about uid remapping — that coverage lives
in `fx-upstream-updateuid-remoteuser`, the one fixture written for it.
