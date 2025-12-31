# Research: Workspace Mount Consistency and Git-Root Handling

## Findings

- **Decision**: Propagate the user-selected workspace mount consistency value into every default workspace mount for both Docker and Compose outputs.  
  **Rationale**: The spec requires the requested consistency to be visible; aligning both runtimes prevents divergent filesystem semantics.  
  **Alternatives considered**: Leave Compose using defaults while only Docker honors consistency (rejected: violates acceptance); derive consistency from runtime defaults (rejected: hides user intent).

- **Decision**: Detect git root via standard repository top-level discovery relative to the working directory (git top-level), and use it as the host path when the git-root flag is set.  
  **Rationale**: Matches user expectation and spec wording; avoids mounting partial repos when invoked from subdirectories.  
  **Alternatives considered**: Use current working directory even when git-root is requested (rejected: defeats flag purpose); scan for nearest `.git` directory upward without confirming top-level (rejected: risks nested repo mis-selection).

- **Decision**: Apply the git-root selection uniformly to all workspace mounts generated for Docker and for every Compose service that mounts the workspace.  
  **Rationale**: Ensures parity across runtimes and services; acceptance requires both Docker and Compose to use the git root when requested.  
  **Alternatives considered**: Limit git-root handling to Docker only (rejected: fails acceptance); apply per-service overrides (rejected: unnecessary complexity and risk of drift).

- **Decision**: When git-root detection fails (not a git repo), fall back to the workspace root with an explicit surfaced fallback while still applying the requested consistency.  
  **Rationale**: Keeps launches working without silent divergence; surfaces intent vs. reality.  
  **Alternatives considered**: Hard-fail when git root missing (rejected: too disruptive for non-git workspaces); silently continue with workspace root (rejected: hides divergence).

- **Decision**: Testing focus on deterministic path selection and mount rendering, using unit-level coverage for discovery/rendering and targeted integration coverage only if output formatting paths differ between Docker and Compose.  
  **Rationale**: Keeps fast feedback while covering parity between runtimes; aligns with nextest guidance.  
  **Alternatives considered**: Rely solely on integration tests (rejected: slower, harder to isolate); rely solely on unit tests without Docker/Compose render validation (rejected: risk of parity regressions).
