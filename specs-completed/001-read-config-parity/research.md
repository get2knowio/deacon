# Research: Read-Configuration Spec Parity

Date: 2025-10-31
Branch: 001-read-config-parity
Spec: ./spec.md

## Unknowns and Decisions

### 1) Docker Compose v2 Detection Strategy
- Decision: Prefer `docker compose` when available; fallback to `docker-compose` binary.
- Rationale: Aligns with modern Docker; supports environments without v2.
- Alternatives: Hard-require v2 (rejected; reduces portability), Always use `docker-compose` (rejected; legacy-first).

### 2) Additional Features Merge Semantics
- Decision: Deep-merge per feature with additional-features precedence; arrays replaced, objects deep-merged.
- Rationale: Predictable overrides; avoids ambiguous array concatenation.
- Alternatives: Overwrite whole feature (too destructive); error on conflicts (too restrictive).

### 3) Container Inspect Failure Behavior (Merged Requested)
- Decision: Error and exit non-zero; no fallback or silent omission.
- Rationale: Constitution “No Silent Fallbacks”; avoids misleading outputs.
- Alternatives: Fallback to non-container merge (violates constitution); omit field with warning (silent partial behavior).

### 4) `${devcontainerId}` Computation
- Decision: Deterministic hash/concat of sorted id-label pairs; order-insensitive; adding/removing labels changes ID.
- Rationale: Matches spec intent; stable across invocation order.
- Alternatives: Preserve input order (non-deterministic); single-label only (too restrictive).

### 5) Output Contract Enforcement
- Decision: Always print a single JSON document to stdout; route all logs to stderr; fail fast on errors with no stdout.
- Rationale: Scriptability and machine-readability; already a repo contract.
- Alternatives: Mixed-mode stdout (breaks consumers).

## Best Practices Notes
- Validate `--id-label` against regex `/.+=.+/` early; provide precise error messages.
- Pair `--terminal-columns` and `--terminal-rows`; reject singletons with actionable error.
- Keep feature planning network-free in this command; reuse caches only.
- Ensure tests cover container-only mode and merged-with-container path.
