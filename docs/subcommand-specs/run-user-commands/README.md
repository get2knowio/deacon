**Executive Summary**
- Purpose: Run devcontainer lifecycle commands inside an existing container with correct ordering, idempotency, env resolution, and secrets handling.
- Typical uses:
  - Refresh project after syncing sources: run onCreate/updateContent/post* without rebuilding.
  - CI prebuild to run create/update steps then stop (`--prebuild`).
  - Personalize container with dotfiles then hand off to editor (`--stop-for-personalization`).

**Quick Reference**
- Select container: one of `--container-id`, `--id-label <k=v>`, or `--workspace-folder`.
- Stop early:
  - `--skip-non-blocking-commands` (stops at `waitFor`, default `updateContentCommand`).
  - `--prebuild` (runs onCreate/updateContent, returns `prebuild`).
  - `--stop-for-personalization` (after dotfiles install).
- Environment:
  - `--remote-env KEY=VALUE` (repeatable), `--default-user-env-probe`.
  - `--secrets-file secrets.json` (env injection + log redaction).
- Post-attach:
  - `--skip-post-attach` to omit `postAttachCommand`.

**Files**
- SPEC: docs/subcommand-specs/run-user-commands/SPEC.md
- Diagrams: docs/subcommand-specs/run-user-commands/DIAGRAMS.md
- Data Structures: docs/subcommand-specs/run-user-commands/DATA-STRUCTURES.md

**Implementation Checklist**
- CLI args and validation as per Section 2
- Container lookup via `container-id` or inferred labels from `workspace-folder`/`config`
- Two-phase substitution (pre-container id, container env)
- Image metadata merge and lifecycle map
- ContainerProperties creation (exec/pty, env, folders, timestamps)
- Remote env probe with optional cache
- Lifecycle execution with markers and exit semantics
- Dotfiles install and stop-for-personalization path
- Secrets loading and log masking
- Output contract: single JSON line to stdout; logs to stderr

