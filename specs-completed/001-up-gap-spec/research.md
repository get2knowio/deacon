# Research for Devcontainer Up Gap Closure

## Output contract and logging separation
- **Decision**: Enforce stdout-only JSON result/error payloads with all logs on stderr, preserving tracing spans and redaction.
- **Rationale**: Aligns with spec §§9–10 and constitution Principle V; decouples machine-readable outputs from human diagnostics and prevents secret leakage.
- **Alternatives considered**: Mixed stdout/stderr for convenience (rejected: violates contract); gated JSON mode only (rejected: spec requires JSON always for up).

## Validation and fail-fast strategy
- **Decision**: Perform clap-level validation plus early normalization checks (workspace/id-label requirements, mount/remote-env regex, terminal dims pairing, expect-existing) before any runtime/build actions.
- **Rationale**: Matches SPEC §3 rules, avoids costly docker/compose calls on invalid inputs, and satisfies “No Silent Fallbacks.”
- **Alternatives considered**: Defer some checks to runtime (rejected: slower feedback, potential side effects); best-effort parsing with warnings (rejected: conflicts with fail-fast).

## Testing cadence for parity changes
- **Decision**: Use `make dev-fast` during implementation loops (fmt, clippy, unit/bins/examples, doctests) and run full gate (build, full tests, doctests, fmt/clippy) before PR.
- **Rationale**: Constitution Principle II requires green builds; fast loop keeps iteration speed while ensuring coverage expansions land with validation.
- **Alternatives considered**: Running only targeted unit tests (rejected: risk of regressions across CLI and integration paths); skipping doctests until end (rejected: constitution requires them).
