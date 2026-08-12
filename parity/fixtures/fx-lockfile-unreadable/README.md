# `fx-lockfile-unreadable`

Not vendored — authored for [#571](https://github.com/get2knowio/deacon/issues/571).

The fixture IS its `.devcontainer-lock.json`: `{ this is not json`, which is not JSON and
never will be. The configuration beside it is the same three-line, one-Feature workspace
the `fx-upstream-lockfile-*` fixtures use, so the only thing that distinguishes a run here
from a run there is the lockfile's bytes.

It exists because the lockfile's `integrity` PINS resolution (#571). Once a lockfile is
read rather than only written, a lockfile that cannot be read is not a cosmetic problem to
be papered over — it is an input the run depends on and cannot have. Both CLIs treat it
that way: measured at oracle 0.87.0, `devcontainer build --workspace-folder .` here exits
1 with `Expected property name or '}' in JSON at position 2` and leaves the file
byte-unchanged.

The Feature is pinned to `:1.3.2` rather than `:1` and the base to `debian:bookworm-slim`
for the reasons the sibling fixtures record — but neither is load-bearing here, because no
case on this fixture ever reaches a registry or a daemon. The refusal happens while
reading a local file, which is why `case-build-lockfile-unreadable-rejected` runs in the
hermetic lane.
