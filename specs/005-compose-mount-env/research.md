# Research Summary: Compose mount & env injection

## Decision: Inject mounts/env directly into primary service definition
- **Rationale**: Aligns with spec to avoid temporary override files while ensuring mounts and env are present at container start; keeps compose project naming intact.
- **Alternatives considered**: (1) Generate transient override compose files (rejected: violates spec requirement to avoid temp overrides); (2) Apply mounts/env via runtime exec after start (rejected: env would miss process start and mounts cannot be hot-added); (3) Inject into all services (rejected: spec targets primary service only unless explicitly requested).

## Decision: Preserve external volumes as declared
- **Rationale**: Spec calls for honoring external volume references so existing data persists; avoiding substitution prevents accidental data loss.
- **Alternatives considered**: (1) Convert external volumes to bind mounts (rejected: changes semantics and risks data drift); (2) Skip external volumes when injecting mounts (rejected: would break services relying on them).

## Decision: Apply CLI env with precedence over env-files/service defaults
- **Rationale**: Spec acceptance requires remote env visibility; precedence ensures user intent overrides defaults without removing other env-file values.
- **Alternatives considered**: (1) Keep compose/env-file precedence ahead of CLI env (rejected: user-provided overrides might be ignored); (2) Merge only non-conflicting keys (rejected: ambiguous resolution and violates clear override semantics).

## Decision: Align mountWorkspaceGitRoot with CLI mount rules
- **Rationale**: Spec requires Git root mount to follow same conventions as other CLI mounts; consistent path handling avoids special-case drift.
- **Alternatives considered**: (1) Separate Git root handling with unique target path logic (rejected: increases inconsistency and risk of misalignment); (2) Disable Git root mount when other mounts present (rejected: contradicts spec expectation of combined mounts).

## Decision: Maintain profile/env-file/project-name semantics during injection
- **Rationale**: Spec requires profiles/env-files/project naming to stay respected; injections must not alter service selection or naming prefixes.
- **Alternatives considered**: (1) Flatten profiles or rename project for injected runs (rejected: breaks parity and resource naming consistency); (2) Ignore env-files when remote env passed (rejected: would drop required config and violate spec).
