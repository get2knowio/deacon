Vendored from [devcontainers/cli](https://github.com/devcontainers/cli) (MIT) at tag
`v0.87.0`, source path `src/test/container-features/configs/lockfile-frozen`.

Upstream's `src/test/container-features/lockfile.test.ts` drives this config from four
tests that share one precondition — **the workspace contains a lockfile that already
matches what resolution would produce** — and assert that a frozen build/up succeeds and
leaves the file byte-unchanged:

- `frozen lockfile`
- `--frozen-lockfile succeeds with matching lockfile`
- `devcontainer up --frozen-lockfile succeeds with matching lockfile`
- `--no-lockfile ignores existing lockfile`

and one that is the whole point of the committed bytes:

- `frozen lockfile matches despite formatting differences` — the same lockfile with its
  trailing newline stripped still satisfies `--frozen-lockfile`, because the reference
  compares the PARSED document, not the bytes.

Adapted for this suite:

- **Feature repinned** from `ghcr.io/codspace/features/{flower,color}:1` to
  `ghcr.io/devcontainers/features/git:1.3.2`, for the reason spelled out in
  `fx-upstream-lockfile-no-lockfile/UPSTREAM.md`: the `codspace` namespace is unpinnable
  third-party content, and no claim here depends on which Feature is declared.
  The upstream fixture pins the FLOATING major (`:1`); this one pins the exact
  `:1.3.2` deliberately, so the digest the lockfile records cannot move when upstream
  publishes a new patch and turn a green case red for a reason that is not about deacon.
- **Base repinned** to `debian:bookworm-slim` — same two reasons as the sibling fixture
  (locally extractable, and no `remoteUser`, so no case here opts into the reference's
  uid-remap ENOENT race). Do not "upgrade" it.
- **`.devcontainer-lock.json` is the reference CLI's OWN output, copied verbatim.** It was
  produced by running `devcontainer build --workspace-folder <this fixture>` at oracle
  0.87.0 and copying the file it wrote — key order, indentation and trailing newline
  included. That is the point of the fixture: the committed bytes are a lockfile the
  reference CLI wrote, so "does this CLI accept a lockfile the other one produced?" is a
  question the case can actually ask. Regenerating it by hand, or normalizing its key
  order, would destroy the only property that makes it evidence.
