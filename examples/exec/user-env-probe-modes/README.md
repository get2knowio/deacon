# Exec: User Environment Probe Modes

This example demonstrates the different `userEnvProbe` modes that control how shell environment variables are collected before command execution.

## Purpose

The `userEnvProbe` setting determines how the container's shell initialization is executed to collect environment variables (especially PATH). Different modes trade off between completeness and speed.

## Prerequisites

- Running dev container
- Understanding of shell initialization files

## Files

- `devcontainer.json` - Configuration with `userEnvProbe` set
- `probe-test.sh` - Script to verify environment collection
- `.bashrc` - Custom bash initialization (demonstrates shell init)

## Probe Modes

### `loginInteractiveShell` (default)
Runs shell as both login (`-l`) and interactive (`-i`):
- Sources `/etc/profile`, `~/.profile`, `~/.bash_profile`
- Sources `~/.bashrc`
- Most complete environment
- Slowest mode

### `interactiveShell`
Runs shell as interactive only (`-i`):
- Sources `~/.bashrc` only
- Skips login scripts
- Faster than loginInteractiveShell
- May miss login-only PATH modifications

### `loginShell`
Runs shell as login only (`-l`):
- Sources `/etc/profile`, `~/.profile`, `~/.bash_profile`
- Skips `~/.bashrc`
- May miss interactive-only environment

### `none`
Skips environment probing:
- Uses basic container environment
- Fastest mode
- May have incomplete PATH
- Good for simple commands not needing full environment

## Usage

### Test Default Probe (loginInteractiveShell)
```bash
deacon exec --workspace-folder . bash /workspace/probe-test.sh
```

### Override Probe Mode
```bash
# Use interactiveShell mode
deacon exec --workspace-folder . \
  --default-user-env-probe interactiveShell \
  bash /workspace/probe-test.sh

# Use loginShell mode
deacon exec --workspace-folder . \
  --default-user-env-probe loginShell \
  bash /workspace/probe-test.sh

# Use none mode (no probing)
deacon exec --workspace-folder . \
  --default-user-env-probe none \
  bash /workspace/probe-test.sh
```

### Compare PATH Across Modes
```bash
echo "=== loginInteractiveShell ==="
deacon exec --workspace-folder . \
  --default-user-env-probe loginInteractiveShell \
  bash -c 'echo $PATH'

echo ""
echo "=== interactiveShell ==="
deacon exec --workspace-folder . \
  --default-user-env-probe interactiveShell \
  bash -c 'echo $PATH'

echo ""
echo "=== loginShell ==="
deacon exec --workspace-folder . \
  --default-user-env-probe loginShell \
  bash -c 'echo $PATH'

echo ""
echo "=== none ==="
deacon exec --workspace-folder . \
  --default-user-env-probe none \
  bash -c 'echo $PATH'
```

### Check Custom Variables
```bash
# CUSTOM_VAR is set in .bashrc (interactive)
deacon exec --workspace-folder . \
  --default-user-env-probe interactiveShell \
  bash -c 'echo $CUSTOM_VAR'

# May not be present with loginShell mode
deacon exec --workspace-folder . \
  --default-user-env-probe loginShell \
  bash -c 'echo $CUSTOM_VAR'
```

## Expected Behavior

### Variable Availability by Mode

| Variable Source | loginInteractiveShell | interactiveShell | loginShell | none |
|----------------|----------------------|------------------|------------|------|
| `/etc/profile` | ✓ | ✗ | ✓ | ✗ |
| `~/.profile` | ✓ | ✗ | ✓ | ✗ |
| `~/.bash_profile` | ✓ | ✗ | ✓ | ✗ |
| `~/.bashrc` | ✓ | ✓ | ✗ | ✗ |
| Container defaults | ✓ | ✓ | ✓ | ✓ |

### Performance Impact
- **none**: Fastest (no shell startup)
- **loginShell** or **interactiveShell**: Medium (one shell init)
- **loginInteractiveShell**: Slowest (full initialization)

### Caching
Environment probe results may be cached per container session, reducing overhead on subsequent exec calls.

## Configuration

### Set in devcontainer.json
```json
{
  "image": "debian",
  "userEnvProbe": "interactiveShell"
}
```

### Override via CLI
```bash
deacon exec --default-user-env-probe none ...
```

## Shell Initialization Files

### Bash (most common)
- Login: `/etc/profile`, `~/.bash_profile` or `~/.profile`
- Interactive: `~/.bashrc`

### Zsh
- Login: `/etc/zprofile`, `~/.zprofile`
- Interactive: `~/.zshrc`

## Notes

- Default mode is `loginInteractiveShell` when not specified
- Mode in devcontainer.json is `userEnvProbe`; CLI flag is `--default-user-env-probe`
- CLI flag only applies when config doesn't specify `userEnvProbe`
- Probe mode affects ALL subsequent environment merges
- Choose based on whether tools need full shell environment or just basic PATH
