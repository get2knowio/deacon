# Research - Up Lifecycle Semantics Compliance

## Decisions

### Decision 1: Prebuild marker isolation
- Decision: Prebuild uses isolated/temporary markers so a subsequent normal `up` reruns onCreate and updateContent before postCreate/postStart/postAttach.
- Rationale: Avoids skipping required setup when moving from prebuild to an interactive run; prevents partial prebuild state from appearing complete.
- Alternatives considered: (a) Share markers with normal runs (risk skipping needed setup), (b) Write no markers (forces all phases every time, reducing prebuild value).

### Decision 2: Marker strategy for resume and failure recovery
- Decision: Maintain ordered per-phase markers (onCreate, updateContent, postCreate, dotfiles, postStart, postAttach); if markers are missing/corrupted, rerun from the earliest phase to restore a valid sequence.
- Rationale: Guarantees deterministic resumes and prevents out-of-order execution when state is uncertain.
- Alternatives considered: (a) Single aggregate marker (insufficient granularity for partial runs), (b) Heuristic/timestamp checks (risk incorrect skipping).

### Decision 3: Dotfiles execution policy
- Decision: Apply dotfiles after postCreate and before postStart on normal runs; skip dotfiles entirely when `--skip-post-create` or prebuild mode is active, and report them as skipped with a reason.
- Rationale: Matches spec-defined ordering and avoids applying dotfiles during limited-scope runs.
- Alternatives considered: (a) Run dotfiles before postCreate (breaks ordering), (b) Allow partial dotfiles during prebuild (contradicts skip rules).

### Decision 4: Summary/reporting content
- Decision: Summaries list phases in lifecycle order with executed/skipped status and reasons (including dotfiles), reflecting marker outcomes.
- Rationale: Supports acceptance checks for ordering, skip behaviors, and resume clarity.
- Alternatives considered: (a) Minimal summary without reasons (reduces debuggability), (b) Sorting by execution time (breaks deterministic order expectations).

## Deferrals

- None; no deferred work recorded.
