# Research Log: Build Subcommand Parity Closure

## Decision 1: Compose Service Targeting
- **Decision**: Build only the service named in the resolved devcontainer configuration; fail if missing.
- **Rationale**: Aligns with reference CLI behavior and ensures deterministic builds matching the user's selected service.
- **Alternatives Considered**: Build all services (rejected as it wastes compute and diverges from spec), build first service in file (rejected due to non-determinism).

## Decision 2: BuildKit Capability Gating
- **Decision**: Detect BuildKit availability via existing Docker buildx checks and emit documented errors when required flags (`--push`, `--output`, `--platform`, `--cache-to`) are used without BuildKit.
- **Rationale**: Matches specification expectations and Constitution Principle III (fail fast instead of silently downgrading).
- **Alternatives Considered**: Silently ignore BuildKit-only flags (rejected due to spec violation), attempt fallback to classic Docker build (rejected because features require BuildKit semantics).

## Decision 3: JSON Output Contract
- **Decision**: Emit spec-compliant stdout payloads `{ "outcome": "success", "imageName": [...] }` or `{ "outcome": "error", "message": ..., "description": ... }`, routing logs to stderr.
- **Rationale**: Upholds Principle V and enables tooling to parse results reliably.
- **Alternatives Considered**: Reuse legacy `BuildResult` structure (rejected as incompatible), mix logs with result JSON (rejected; breaks contract).

## Decision 4: Metadata Label Injection
- **Decision**: Serialize merged devcontainer metadata (config, features, customizations) into the standard label and append user-specified labels during build.
- **Rationale**: Provides parity with upstream CLI and enables downstream workflows to reconstruct configuration provenance.
- **Alternatives Considered**: Only label with config hash (current behavior; insufficient metadata), skip user labels (rejected due to spec gap).

## Decision 5: Feature Installation Workflow
- **Decision**: Reuse core feature resolution to generate build contexts/scripts and integrate them into Dockerfile/image/Compose builds, honoring skip flags and lockfile requirements.
- **Rationale**: Ensures features work consistently across configuration modes and satisfies FR-008/FR-011 without introducing duplicate logic.
- **Alternatives Considered**: Defer feature installation to future iteration (rejected; leaves major spec gaps), implement mode-specific feature installers (rejected due to maintenance overhead).
