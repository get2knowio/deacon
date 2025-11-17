# Exec: Non-Interactive Streaming

This example demonstrates non-PTY mode with separate stdout/stderr streams, suitable for automation, CI/CD, and binary-safe I/O.

## Purpose

Non-interactive mode is essential for scripting, CI/CD pipelines, and any scenario where:
- Separate stdout and stderr streams are needed
- Binary data must be preserved
- No terminal is available
- Output is redirected to files or pipes

## Prerequisites

- Running dev container
- Understanding of stream redirection

## Files

- `devcontainer.json` - Basic configuration
- `generate-output.sh` - Script producing both stdout and stderr
- `binary-test.sh` - Script demonstrating binary-safe I/O
- `data/sample.bin` - Binary test file

## Usage

### Separate Streams (Redirect to Files)
```bash
# Capture stdout and stderr separately
deacon exec --workspace-folder . bash /workspace/generate-output.sh \
  > output.txt 2> errors.txt

# Verify separate streams
echo "=== stdout ==="
cat output.txt
echo "=== stderr ==="
cat errors.txt
```

### Pipe stdout to Another Command
```bash
# Only stdout goes to grep; stderr visible on terminal
deacon exec --workspace-folder . bash /workspace/generate-output.sh \
  | grep "output line"
```

### Binary-Safe Operation
```bash
# Read binary file and pipe through exec without corruption
deacon exec --workspace-folder . cat /workspace/data/sample.bin \
  | xxd | head -n 5
```

### Process Substitution
```bash
# Use output as input to another command
cat <(deacon exec --workspace-folder . bash /workspace/generate-output.sh)
```

### CI/CD Pattern (Capture Exit Code)
```bash
# Capture both output and exit code
deacon exec --workspace-folder . bash /workspace/generate-output.sh \
  > build.log 2>&1
EXIT_CODE=$?
echo "Build exit code: $EXIT_CODE"
```

### JSON Output (Forces PTY but streams continuously)
```bash
# Even with JSON, output streams continuously
deacon exec --workspace-folder . --log-format json \
  bash /workspace/generate-output.sh
```

### Large Output Streaming
```bash
# Large outputs stream without truncation
deacon exec --workspace-folder . bash -c \
  'for i in {1..1000}; do echo "Line $i"; done' \
  | wc -l
```

## Expected Behavior

### Non-PTY Characteristics
- **Separate streams**: stdout and stderr remain distinct
- **Binary-safe**: No data corruption or encoding issues
- **Buffering**: Line-buffered by default for text output
- **No terminal codes**: ANSI escapes may not render properly
- **Clean exit codes**: Numeric exit status propagates correctly

### PTY vs Non-PTY Selection
Non-PTY mode is used when:
- stdout is not a TTY (redirected to file/pipe), OR
- stdin is not a TTY, OR
- `--log-format json` is NOT specified

### Stream Handling
- **stdout**: Command's standard output (file descriptor 1)
- **stderr**: Command's error output (file descriptor 2)
- **stdin**: Can accept piped/redirected input
- **Exit code**: Numeric value from command (0-255)

## Notes

- Non-PTY mode required for reliable stream separation
- Binary data passes through safely without PTY processing
- No terminal features (colors, cursor control) in non-PTY mode
- Ideal for batch processing and automation
- Progress bars and interactive prompts won't work properly
