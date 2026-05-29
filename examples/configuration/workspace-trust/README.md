# Configuration: Workspace-Trust Gate for `initializeCommand`

`initializeCommand` runs on the **developer's host** before any container
sandboxing. deacon gates any host-side exec sourced from the workspace behind
a workspace-trust check (see `SECURITY.md`). This canary verifies both ends of
that gate.

## Files

- `.devcontainer.json` — an `initializeCommand` that writes
  `./initialize-ran.marker` on the host.

## Scenarios exercised by `exec.sh`

1. **Denied.** `DEACON_NO_PROMPT=1 deacon up …` fails closed: the command
   errors with a workspace-trust message and the host marker is **not**
   written (CI-safe default).
2. **Allowed.** `deacon up --trust-workspace …` runs the host
   `initializeCommand`, so the marker appears.

`exec.sh` removes the marker and container on exit.

## Spec / security references

- This gate is deacon-specific (not mandated by containers.dev). See
  `SECURITY.md` and the "Workspace-Trust Gate" section of `CLAUDE.md`.
