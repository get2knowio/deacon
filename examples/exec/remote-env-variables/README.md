# Exec: Remote Environment Variables

This example demonstrates how environment variables are merged from multiple sources using `--remote-env` and configuration.

## Purpose

Shows the environment variable merge order and precedence: shell-derived → config `remoteEnv` → CLI `--remote-env`. This is crucial for overriding variables per execution without changing configuration.

## Prerequisites

- Running dev container
- Understanding of environment variable precedence

## Files

- `devcontainer.json` - Configuration with `remoteEnv` settings
- `check-env.sh` - Script to inspect environment variables

## Usage

1. Start the dev container:
   ```bash
   deacon up --workspace-folder .
   ```

2. Check default environment from config:
   ```bash
   deacon exec --workspace-folder . bash /workspace/check-env.sh
   ```

3. Override a config variable via CLI:
   ```bash
   deacon exec --workspace-folder . --remote-env APP_MODE=production bash /workspace/check-env.sh
   ```

4. Add new variables not in config:
   ```bash
   deacon exec --workspace-folder . \
     --remote-env CUSTOM_VAR=custom-value \
     --remote-env DEBUG=true \
     env | grep -E "(CUSTOM_VAR|DEBUG)"
   ```

5. Set empty value (variable exists but empty):
   ```bash
   deacon exec --workspace-folder . --remote-env EMPTY_VAR= env | grep EMPTY_VAR
   ```

6. Multiple remote-env flags (order matters):
   ```bash
   deacon exec --workspace-folder . \
     --remote-env FOO=first \
     --remote-env FOO=second \
     bash -c 'echo $FOO'
   ```

## Expected Behavior

### Merge Order (later sources override earlier)
1. **Shell-derived**: Variables from shell initialization via `userEnvProbe`
2. **Config remoteEnv**: Variables defined in `devcontainer.json`
3. **CLI --remote-env**: Variables passed on command line (highest precedence)

### Examples
- Config sets `APP_MODE=development`, CLI passes `--remote-env APP_MODE=production` → Result: `production`
- Config sets `API_URL=http://localhost`, no CLI override → Result: `http://localhost`
- CLI passes `--remote-env NEW_VAR=value`, not in config → Result: `NEW_VAR=value`

## Notes

- Empty values (`--remote-env VAR=`) create empty variables (not unset)
- Format must be `name=value` where name is non-empty
- Multiple `--remote-env` flags are allowed and applied in order
- Shell-derived PATH typically persists unless explicitly overridden
