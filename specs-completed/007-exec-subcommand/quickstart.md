# Quickstart: Exec Subcommand

This guide mirrors the user-facing behavior for the `exec` subcommand.

## Target a container by ID

```bash
# Run a simple command by container ID
cargo run -- exec --container-id $CID echo hello
```

## Target by label

```bash
# Use one or more labels (name=value)
cargo run -- exec \
  --id-label devcontainer.local_folder="$(pwd)" \
  --id-label service=web \
  env
```

## Discover by workspace folder

```bash
cargo run -- exec \
  --workspace-folder "/abs/path/to/workspace" \
  pwd
```

## Environment merge and overrides

```bash
# CLI-provided env overrides config remoteEnv
cargo run -- exec \
  --remote-env FOO=bar \
  --remote-env EMPTY= \
  env | grep -E '^(FOO|EMPTY)='
```

## Interactive PTY and terminal sizing

```bash
# PTY is allocated in TTY contexts; force with json log format
cargo run -- exec --log-format json bash -lc 'tty; stty size'

# Override PTY size
cargo run -- exec \
  --terminal-columns 120 \
  --terminal-rows 40 \
  bash -lc 'stty size'
```

## Expected errors

- Missing selection flags: one of `--container-id`, `--id-label`, or `--workspace-folder` is required.
- Invalid label: must be `name=value` with non-empty parts.
- Config discovery failure: "Dev container config (<path>) not found."

See `spec.md` for complete requirements and `contracts/exec.openapi.yaml` for a conceptual contract.
