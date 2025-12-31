# Quickstart - Force PTY toggle for up lifecycle

## Implementation Summary

The force PTY toggle feature allows operators running `deacon up` with JSON logging to allocate pseudo-terminals (PTYs) for lifecycle exec commands, enabling interactive commands to behave correctly while maintaining structured JSON logs.

### Usage

**Via CLI Flag (Global Flag):**
```bash
# Enable PTY allocation for lifecycle commands when using JSON logs
deacon up --log-format json --force-tty-if-json

# Default behavior (no PTY allocation)
deacon up --log-format json
```

**Via Environment Variable:**
```bash
# Enable PTY allocation
export DEACON_FORCE_TTY_IF_JSON=true
deacon up --log-format json

# Disable PTY allocation (default)
export DEACON_FORCE_TTY_IF_JSON=false
deacon up --log-format json
```

**Truthy values** (case-insensitive): `true`, `1`, `yes`
**Falsey values**: `false`, `0`, `no`, or unset

**Precedence:**
1. CLI flag (`--force-tty-if-json`)
2. Environment variable (`DEACON_FORCE_TTY_IF_JSON`)
3. Default (no PTY allocation)

### Behavior

- **Scope**: Only applies when `--log-format json` is active
- **Target**: Affects all lifecycle exec commands in `deacon up` (onCreate, postCreate, postStart, etc.)
- **Output Separation**: JSON logs remain on stderr, machine-readable output on stdout, regardless of PTY allocation
- **Non-JSON Mode**: Flag/env var has no effect; existing TTY behavior is preserved
- **Other Commands**: `exec` and other entry points retain their existing TTY behavior

### Important Notes

- This setting only applies when `--log-format json` is active
- With PTY allocation enabled, interactive commands work correctly while JSON logs remain structured on stderr and machine-readable output stays on stdout
- PTY allocation failures surface clear, actionable error messages
- All lifecycle exec steps within a single `up` run honor the resolved PTY preference consistently

## Implementation Checklist

1) Read the spec (`/workspaces/deacon/specs/001-force-pty-up/spec.md`) and research (`/workspaces/deacon/specs/001-force-pty-up/research.md`); align with Constitution gates (spec parity, no silent fallbacks, stdout/stderr separation).
2) Implement PTY preference resolution: flag `--force-tty-if-json` overrides env `DEACON_FORCE_TTY_IF_JSON` (truthy `true/1/yes`; falsey `false/0/no`; unset disabled); default is no PTY. Apply only when JSON log mode is active.
3) Apply resolved PTY to all lifecycle exec steps inside `deacon up`; preserve existing behavior for non-JSON mode and other exec entry points.
4) Maintain strict stdout/stderr separation: JSON outputs remain on stdout; logs/diagnostics on stderr even under PTY. Surface clear errors if PTY allocation fails.
5) Tests: add integration coverage for PTY-on (flag/env + JSON), PTY-off (unset), and exec regression. Configure any new integration binaries in `.config/nextest.toml` test groups. Use `make test-nextest-fast` for fast loops; expand to `make test-nextest` before PR if required.
6) Tooling cadence: `cargo fmt --all`, `cargo fmt --all -- --check`, `cargo clippy --all-targets -- -D warnings`, then targeted `make test-nextest-*` per changes.
7) Update examples/fixtures if lifecycle exec behavior or CLI flags impact documented workflows; keep README and exec.sh scripts aligned if touched.
