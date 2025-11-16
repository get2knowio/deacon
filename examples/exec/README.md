# Exec Examples Quickstart

This quickstart demonstrates environment and PTY-related usage for `deacon exec`.

Prerequisites
- Docker running
- `deacon` CLI built and available in PATH

Examples

1. Verify environment variable injection (empty values preserved):

```sh
# inject FOO=bar and BAZ (empty)
deacon exec --container-id <id> --env FOO=bar --env BAZ= -- sh -lc 'echo "$FOO"; echo "<$BAZ>"'
# Expected output:
# BAR
# <>
```

2. Force PTY allocation when JSON log format is requested:

```sh
# Force JSON log-format; PTY should be allocated even if stdout isn't a TTY
deacon exec --container-id <id> --log-format json -- echo hello
```

3. Non-interactive (no PTY) example:

```sh
# Disable PTY explicitly
cat file.txt | deacon exec --container-id <id> --no-tty -- sh -c 'cat > /tmp/out'
```

Notes
- Use `--container-id` for direct targeting; `--id-label` and `--workspace-folder` discovery are also supported.
- `--env` accepts `KEY=VALUE` with empty values preserved.
- `--no-tty` disables PTY; `--log-format json` forces PTY allocation for consistent streaming.
