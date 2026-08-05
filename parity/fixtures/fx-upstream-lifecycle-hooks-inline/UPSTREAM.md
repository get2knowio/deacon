Vendored from [devcontainers/cli](https://github.com/devcontainers/cli) (MIT) at tag
`v0.87.0`, source path
`src/test/container-features/configs/lifecycle-hooks-inline-commands`.

Adapted for this suite:

- **base repinned to `debian:bookworm-slim`, and it must stay a base with no `remoteUser`.**
  See "Do not upgrade this base" below — this is not a size or speed choice.
- `createMarker.sh` rewritten to append one line per hook to `lifecycle-order.log`.
  Upstream names each marker `<counter>.<name>` and dumps `printenv` into it: that pins
  the order, but makes the file's CONTENT a per-host environment snapshot, which is not
  something two CLIs can be compared on. The append-only log records the same two facts
  — WHICH hooks ran and in WHAT order — as one deterministic compared value, and the
  `sleep 1s` is dropped with it.

What the fixture is FOR is untouched: two local Features (`./tiger`, `./panda`, ordered by
`installsAfter`) each declare all five lifecycle phases, in the string form and the array
form, and the configuration declares the same five phases itself. Every phase therefore has
TWO sources, which is the shape #467 and #477 were both defects in.

## Do not upgrade this base to `mcr.microsoft.com/devcontainers/base:bookworm`

It was that image until #480's CI run, and the swap is load-bearing.

`base:bookworm` sets `remoteUser: vscode` (uid 1000) in its `devcontainer.metadata`, so on
any host whose uid is not 1000 BOTH CLIs perform a uid remap. The reference stages its
generated remap Dockerfile at a path scoped only by its own version —
`/tmp/devcontainercli-runner/updateUID.Dockerfile-<version>` — and renames a
timestamp-suffixed temp file onto it. Two concurrent reference `up` runs that both remap
therefore race on that rename, and one loses:

```
Error: ENOENT: no such file or directory, rename
  '/tmp/devcontainercli-runner/updateUID.Dockerfile-0.87.0-1785891315906'
  -> '/tmp/devcontainercli-runner/updateUID.Dockerfile-0.87.0'
```

The `docker-shared` driver runs at concurrency 4, so with `base:bookworm` this fixture and
its two `up` siblings remapped at the same time and the REFERENCE exited 1 — a red case that
indicts neither side's devcontainer behavior. It reproduced only in CI: a dev container whose
host uid is 1000 matches `vscode` exactly, so no remap happens there at all and the race is
invisible.

This fixture claims hook collection across five phases and two sources. It does not claim
anything about uid remapping — that coverage lives in `fx-upstream-updateuid-remoteuser`,
the one fixture written for it, whose `remoteUser` is pinned to uid 1234 so the claim cannot
pass by coincidence. Reintroducing a `remoteUser`-bearing base here adds an interaction this
case does not measure and makes it flaky on the reference's account.
