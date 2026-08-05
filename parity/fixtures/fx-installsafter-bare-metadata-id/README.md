# fx-installsafter-bare-metadata-id

Two local Features that are individually beyond reproach — `.devcontainer/`
folder present, each Feature in a sub-folder of it named for its `id`, both
declared by `devcontainer.json` in the legal `./`-relative form. The single
illegal thing in the workspace is how `./app` names `./base`:

```jsonc
// .devcontainer/app/devcontainer-feature.json
"installsAfter": [ "base" ]   // the sibling's METADATA id, not its path
```

`feature-dependencies.md` gives `installsAfter` "the same syntax as the
`features` object", and `"features": { "base": {} }` does not name the local
`./base` on either CLI — a bare single-segment id is the deprecated v1 (GitHub
Release era) form, mapped only for the eighteen ids that survived into
`ghcr.io/devcontainers/features`. `base` is not one of them.

## Why the target is DECLARED

`./base` is in the `features` map deliberately. It makes the reference resolvable
by deacon's matcher, which resolves a dependency reference by canonical id,
source **or metadata-id alias** — so before #505 deacon did not merely tolerate
this document, it *ordered by it*, installing `base` before `app` and exiting 0.
Had the target been absent, deacon would have soft-skipped an unmatched
`installsAfter` entry and still exited 0, but for a reason that says nothing
about the alias. Its sibling fixture `fx-installsafter-unmatched-bare-id` covers
that second path on purpose.

MEASURED at oracle 0.87.0: the reference exits 1 with
`Legacy feature 'base' not supported.` — it canonicalizes every non-path
`installsAfter` entry at ingress, exactly as it does a `features` map key, before
any ordering question is asked.

`./base` is the spelling that makes this same reference legal, and
`fx-upstream-dependson-installsafter-local` pins that the path form resolves.

The base image is `debian:bookworm-slim` and is never pulled — the case reads
configuration only — but it stays a plain base with no `remoteUser` so the
fixture can be reused by a container-running case without dragging in the uid
remap.

Authored for #505, not vendored from upstream — the reference CLI's e2e suite has
no fixture for this clause.
