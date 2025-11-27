# Data Model - Up Lifecycle Semantics Compliance

## Entities

### LifecyclePhaseState
- **phase**: enum {onCreate, updateContent, postCreate, dotfiles, postStart, postAttach}.
- **status**: enum {pending, executed, skipped, failed}.
- **reason**: optional string describing skip/failure cause (e.g., flag, prebuild mode, error).
- **marker_path**: filesystem path to the completion marker for this phase.
- **timestamp**: optional recorded time when the marker was written.

### InvocationContext
- **mode**: enum {fresh, resume, prebuild, skip_post_create}.
- **flags**: inputs affecting lifecycle (e.g., `skip_post_create` boolean, `prebuild` boolean).
- **workspace_root**: path to the devcontainer workspace.
- **prior_markers**: collection of `LifecyclePhaseState` loaded from disk before deciding which phases run.

### RunSummary
- **phases**: ordered list of `LifecyclePhaseState` after execution reflecting executed/skipped status and reasons.
- **resume_required**: boolean indicating if further phases remain due to a failure/interrupt.
- **output_mode**: text/json indicator to govern reporting separation.

### DotfilesApplication
- **enabled**: boolean derived from configuration and flags.
- **status**: enum {pending, executed, skipped, failed}.
- **reason**: optional string when skipped or failed.

## Relationships and Transitions

- `InvocationContext` determines eligibility of each `LifecyclePhaseState`; phases transition from pending to executed or skipped in strict order.
- Missing or corrupted markers cause earlier phases to transition back to pending to enforce a full ordered rerun before runtime hooks.
- `DotfilesApplication` transitions only after postCreate and before postStart when enabled and not skipped by flags/mode.
- `RunSummary` aggregates final `LifecyclePhaseState` entries in lifecycle order and records whether interruption requires another resume.
