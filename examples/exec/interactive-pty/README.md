# Exec: Interactive PTY Mode

This example demonstrates interactive command execution with PTY (pseudo-terminal) allocation for shells, REPLs, and interactive programs.

## Purpose

PTY mode enables interactive programs that require terminal capabilities: shells, text editors, interactive package managers, REPLs, and programs that use terminal control sequences.

## Prerequisites

- Running dev container
- Terminal attached to stdin/stdout (TTY)

## Files

- `devcontainer.json` - Configuration with Python and Node.js
- `interactive-demo.py` - Python script demonstrating interactive input
- `terminal-test.sh` - Script to test terminal capabilities

## Usage

### Interactive Shell
```bash
deacon exec --workspace-folder . bash
# Inside shell: try commands like 'ls', 'vim', 'python3'
# Exit with 'exit' or Ctrl+D
```

### Python REPL
```bash
deacon exec --workspace-folder . python3
# Interactive Python session
# Try: print("Hello"), import sys, sys.version
# Exit with exit() or Ctrl+D
```

### Interactive Script with Input
```bash
deacon exec --workspace-folder . python3 /workspace/interactive-demo.py
```

### Node.js REPL
```bash
deacon exec --workspace-folder . node
# Interactive Node session
# Try: console.log("Hello"), process.version
# Exit with .exit or Ctrl+D
```

### Terminal Size Control
```bash
deacon exec --workspace-folder . \
  --terminal-columns 120 \
  --terminal-rows 40 \
  bash /workspace/terminal-test.sh
```

### TTY Detection
```bash
# With TTY (interactive)
deacon exec --workspace-folder . tty
# Returns: /dev/pts/X

# Without TTY (redirected)
deacon exec --workspace-folder . tty < /dev/null
# Returns: not a tty
```

## Expected Behavior

### PTY Allocation Conditions
PTY is allocated when:
- Both stdin AND stdout are TTYs, OR
- `--log-format json` is specified

### PTY Characteristics
- **Merged streams**: stdout and stderr are combined
- **Interactive input**: Supports raw terminal input (Ctrl+C, Ctrl+Z, etc.)
- **Terminal control**: ANSI escape codes, cursor positioning, colors work
- **Signal handling**: Signals propagate correctly to child process

### Terminal Sizing
- `--terminal-columns` and `--terminal-rows` hint desired dimensions
- Both flags must be provided together or neither
- Actual size may be adjusted by runtime/container
- Some programs may ignore initial size until resize event

## Notes

- PTY mode required for programs expecting terminal (vim, nano, less, etc.)
- Colors and formatting work only in PTY mode
- Exit codes properly propagate even with PTY
- Signal termination (Ctrl+C) returns `128 + signal_number`
