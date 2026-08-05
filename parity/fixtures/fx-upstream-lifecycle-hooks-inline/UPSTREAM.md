Vendored from [devcontainers/cli](https://github.com/devcontainers/cli) (MIT) at tag
`v0.87.0`, source path
`src/test/container-features/configs/lifecycle-hooks-inline-commands`.

Adapted for this suite:

- base image repinned from `mcr.microsoft.com/devcontainers/base:ubuntu` to
  `:bookworm`, which the fixture tree already caches (V18).
- `createMarker.sh` rewritten to append one line per hook to `lifecycle-order.log`.
  Upstream names each marker `<counter>.<name>` and dumps `printenv` into it: that pins
  the order, but makes the file's CONTENT a per-host environment snapshot, which is not
  something two CLIs can be compared on. The append-only log records the same two facts
  — WHICH hooks ran and in WHAT order — as one deterministic compared value, and the
  `sleep 1s` is dropped with it.

What the fixture is FOR is untouched: two local Features (`./tiger`, `./panda`, ordered
by `installsAfter`) each declare all five lifecycle phases, in the string form and the
array form, and the configuration declares the same five phases itself. Every phase
therefore has TWO sources, which is the shape #467 and #477 were both defects in.
