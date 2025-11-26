# Research - Force PTY toggle for up lifecycle

## Decisions

1) PTY toggle input resolution  
**Decision**: Use the `force_tty_if_json` flag with the env var `DEACON_FORCE_TTY_IF_JSON`; parse truthy `true/1/yes` (case-insensitive) as enable and `false/0/no` or unset as disable.  
**Rationale**: Mirrors the flag name, keeps parsing deterministic, and prevents accidental enablement when unset.  
**Alternatives considered**: Other env names (`DEACON_FORCE_TTY`, `FORCE_TTY_IF_JSON`) or auto-detecting TTY presence—rejected for inconsistency with the flag and for changing defaults in non-JSON modes.

2) PTY allocation behavior in lifecycle exec  
**Decision**: Apply the resolved PTY preference only when `up` is in JSON log mode; request PTY for lifecycle exec steps when the preference is enabled, otherwise run non-PTY. Preserve stdout JSON/stderr log separation regardless of PTY.  
**Rationale**: Aligns with spec acceptance (force PTY only with JSON logs), avoids changing non-JSON defaults, and keeps log channels predictable.  
**Alternatives considered**: Always enabling PTY when available, or routing logs differently under PTY—rejected for breaking existing non-JSON behavior and risking JSON log interleaving.

3) Test coverage approach  
**Decision**: Add integration coverage showing PTY allocation when flag/env + JSON logs are set, non-PTY when unset, and unchanged behavior for other exec paths. Use `make test-nextest-fast` for fast loops and targeted subsets as needed.  
**Rationale**: Directly mirrors acceptance criteria and guards against regressions in exec behavior and log separation.  
**Alternatives considered**: Manual-only validation or unit-only coverage—rejected because PTY/log channel interactions require end-to-end verification.
