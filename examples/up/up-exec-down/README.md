# up → exec → down (compound-flow resolution)

Demonstrates that subcommands resolve the **same** container across a session
using only `--workspace-folder` — no `--container-id` and no `--id-label`.

This is the regression guard for [#187](https://github.com/get2knowio/deacon/issues/187):
`up` and `exec`/`run-user-commands` must compute the **same**
`devcontainer.configHash` label for an identical `devcontainer.json`. When they
diverge, `exec --workspace-folder X` cannot find the container that
`up --workspace-folder X` just created.

## What it does

```bash
# 1. up creates the container; postCreate writes /tmp/identity-marker.
#    --mount-workspace-git-root false keeps the canary self-contained in-repo
#    (see below); it affects only up's bind mount, not container identity.
deacon up --workspace-folder . --mount-workspace-git-root false

# 2. exec resolves THAT container purely by workspace folder and reads the marker
deacon exec --workspace-folder . cat /tmp/identity-marker
#   -> up-exec-down-ok

# 3. run-user-commands shares the same identity-resolution path
deacon run-user-commands --workspace-folder .

# 4. down resolves and removes the container by workspace folder
deacon down --workspace-folder .
```

The canary fails loudly if `exec` resolves a *different* container (the marker
won't match) or none at all (the #187 symptom).

## Why `--mount-workspace-git-root false`

So the example is self-contained when run from inside this monorepo: `up`
otherwise mounts the enclosing git root. Container **identity** is anchored to
`--workspace-folder` regardless of the mount source, which is precisely the
property under test.

## Run

```bash
./exec.sh
```

Requires Docker. The script removes the container on exit.
