# Contract: PTY toggle for `up` lifecycle exec

## Scope
Behavior of `deacon up` lifecycle exec when JSON log mode is active, `--force-tty-if-json` flag and/or `DEACON_FORCE_TTY_IF_JSON` env var are present, and when they are absent.

## Inputs
- CLI: `deacon up --json` (JSON log mode assumed)  
- Flag: `--force-tty-if-json` (explicit request to allocate PTY in JSON mode)  
- Env: `DEACON_FORCE_TTY_IF_JSON` with truthy values `true/1/yes` (case-insensitive) enabling PTY; falsey `false/0/no` or unset disabling; overridden by explicit flag.

## Behavior
- Resolve PTY preference: flag > env > default (no PTY).
- Apply preference only when JSON log mode is active; non-JSON mode keeps existing TTY behavior regardless of flag/env.
- Lifecycle exec steps invoked by `up` allocate PTY when preference is enabled; otherwise run without PTY.
- Other exec entry points are unaffected by this preference (retain existing TTY/log behavior).

## Outputs
- Stdout: JSON output remains machine-readable; no log noise interleaving under PTY or non-PTY.
- Stderr: Structured logs and diagnostics; preserved separation from stdout in all modes.
- Exit codes: Lifecycle exec exit codes propagate unchanged; special PTY handling does not alter exit semantics.

## Error Handling
- If PTY allocation is requested but cannot be honored, emit a clear, actionable error; do not silently downgrade or alter log formatting.
- Conflicts between flag and env resolve in favor of the flag; no ambiguity in the applied preference.

## Examples (non-interactive)
- PTY enabled via flag: `DEACON_FORCE_TTY_IF_JSON=false deacon up --json --force-tty-if-json` → lifecycle exec uses PTY; JSON output intact.
- PTY enabled via env: `DEACON_FORCE_TTY_IF_JSON=true deacon up --json` → lifecycle exec uses PTY.
- Default non-PTY: `deacon up --json` with env unset → lifecycle exec runs without PTY.
- Non-JSON mode: `DEACON_FORCE_TTY_IF_JSON=true deacon up` → TTY behavior matches existing non-JSON defaults.
