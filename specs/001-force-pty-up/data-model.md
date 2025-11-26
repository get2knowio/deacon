# Data Model - Force PTY toggle for up lifecycle

## Entities

### TTY Preference
- **Fields**: source (flag | env | default), value (force_pty | no_pty | default), requires_json_mode (bool).
- **Derivation**: resolve in precedence order flag > env (`DEACON_FORCE_TTY_IF_JSON`, truthy `true/1/yes`, falsey `false/0/no`, unset disabled) > default (no PTY).
- **Constraints**: Only applied when JSON log mode is active for `up`; otherwise, existing non-JSON TTY behavior remains unchanged.

### Lifecycle Exec Step
- **Fields**: command descriptor (per lifecycle), resolved_tty (bool from TTY Preference), log_mode (json | text), stderr_channel (structured logs).
- **Behavior**: Executes within `up` lifecycle using resolved_tty; propagates exit codes; must preserve stdout/stderr separation.
- **Relationships**: Consumes TTY Preference to decide PTY allocation; participates in the `up` lifecycle sequence.

### Log Output Channels
- **Fields**: stdout_payload (JSON results where applicable), stderr_logs (structured logs/diagnostics).
- **Constraints**: Separation between stdout and stderr is mandatory, including under PTY; JSON outputs remain machine-readable without interleaving log noise.

## State & Transitions
- **Resolution**: Determine TTY Preference at `up` invocation time based on flag/env/default.
- **Execution**: Apply preference to each lifecycle exec step while honoring JSON log mode; maintain consistent behavior across steps.
- **Completion**: Exit codes propagate from lifecycle exec steps regardless of PTY selection.

## Cross-Cutting Constraints
- PTY allocation failures surface clear errors without silently changing log formatting or TTY expectations.
- Exec entry points outside `up` continue to use existing TTY behavior and are not altered by this preference.
