# Exec: Exit Code Handling

This example demonstrates how exit codes propagate from executed commands, including POSIX signal mapping for terminated processes.

## Purpose

Reliable exit code handling is essential for:
- CI/CD pipelines (detecting failures)
- Scripting and automation
- Error handling and debugging
- Signal-based termination (Ctrl+C, timeouts)

## Prerequisites

- Running dev container
- Understanding of Unix exit codes and signals

## Files

- `devcontainer.json` - Basic configuration
- `exit-codes.sh` - Script demonstrating various exit scenarios
- `signal-test.sh` - Script for testing signal termination
- `timeout-test.sh` - Long-running script for timeout testing

## Exit Code Rules

### Standard Exit Codes
- **0**: Success
- **1-125**: Command-defined failure codes
- **126**: Command found but not executable
- **127**: Command not found
- **128+N**: Terminated by signal N (POSIX convention)

### Common Signal Codes
- **130** (128+2): SIGINT (Ctrl+C)
- **137** (128+9): SIGKILL (kill -9)
- **143** (128+15): SIGTERM (kill default)

## Usage

### Success (Exit 0)
```bash
deacon exec --workspace-folder . true
echo "Exit code: $?"
# Expected: 0
```

### Standard Failure
```bash
deacon exec --workspace-folder . false
echo "Exit code: $?"
# Expected: 1
```

### Custom Exit Codes
```bash
deacon exec --workspace-folder . bash /workspace/exit-codes.sh 42
echo "Exit code: $?"
# Expected: 42
```

### Command Not Found (Exit 127)
```bash
deacon exec --workspace-folder . nonexistent-command
echo "Exit code: $?"
# Expected: 127
```

### Signal Termination (Ctrl+C)
```bash
# Start long-running process and press Ctrl+C
deacon exec --workspace-folder . bash /workspace/timeout-test.sh
# Press Ctrl+C
echo "Exit code: $?"
# Expected: 130 (128 + SIGINT=2)
```

### Timeout with SIGTERM
```bash
# Using timeout command
deacon exec --workspace-folder . timeout 2s bash /workspace/timeout-test.sh
echo "Exit code: $?"
# Expected: 143 (128 + SIGTERM=15) or 124 (timeout's code)
```

### Killed Process (SIGKILL)
```bash
# In one terminal, start long process:
deacon exec --workspace-folder . bash /workspace/timeout-test.sh

# In another terminal, get container ID and kill:
CONTAINER_ID=$(docker ps --filter "label=devcontainer.local_folder=$(pwd)" --format "{{.ID}}" | head -n1)
docker exec $CONTAINER_ID pkill -9 -f timeout-test.sh

# Back in first terminal, check exit code:
echo "Exit code: $?"
# Expected: 137 (128 + SIGKILL=9)
```

### Testing All Exit Scenarios
```bash
deacon exec --workspace-folder . bash /workspace/exit-codes.sh test-all
```

### CI/CD Pattern
```bash
#!/bin/bash
# Example CI script using exec

set -e  # Exit on any failure

echo "Running tests..."
deacon exec --workspace-folder . npm test
TEST_EXIT=$?

if [ $TEST_EXIT -eq 0 ]; then
    echo "✓ Tests passed"
else
    echo "✗ Tests failed with exit code: $TEST_EXIT"
    exit $TEST_EXIT
fi

echo "Running linter..."
deacon exec --workspace-folder . npm run lint
LINT_EXIT=$?

if [ $LINT_EXIT -ne 0 ]; then
    echo "✗ Linting failed"
    exit $LINT_EXIT
fi

echo "✓ All checks passed"
```

## Expected Behavior

### Exit Code Propagation
- Numeric exit code from command propagates directly
- Zero indicates success; non-zero indicates failure
- Exit codes 0-255 are valid
- Codes > 255 are wrapped (modulo 256)

### Signal Mapping
When process terminated by signal:
- Exit code = 128 + signal_number
- SIGINT (2) → 130
- SIGKILL (9) → 137
- SIGTERM (15) → 143
- Other signals follow same pattern

### Unknown Exit Conditions
If exit status cannot be determined:
- Default to exit code 1
- Log relevant error information

## Common Signals Reference

| Signal | Number | Code | Cause |
|--------|--------|------|-------|
| SIGHUP | 1 | 129 | Terminal hangup |
| SIGINT | 2 | 130 | Interrupt (Ctrl+C) |
| SIGQUIT | 3 | 131 | Quit (Ctrl+\\) |
| SIGKILL | 9 | 137 | Force kill |
| SIGTERM | 15 | 143 | Graceful termination |
| SIGSTOP | 19 | 147 | Stop (Ctrl+Z) |

## Notes

- Exit codes are essential for automation reliability
- Always check exit codes in scripts (`set -e` or explicit checks)
- Signal-based exits indicate abnormal termination
- Some programs use exit codes > 128 for their own purposes (rare)
- PTY and non-PTY modes both preserve exit codes correctly
