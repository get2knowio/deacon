# Quickstart: Fix Lifecycle Command Format Support

**Feature Branch**: `012-fix-lifecycle-formats`

## What Changed

Lifecycle commands in devcontainer.json now correctly support all three formats defined by the DevContainer specification:

1. **String** — executed through a shell (existing behavior, preserved)
2. **Array** — exec-style, passed directly to the OS without shell interpretation
3. **Object** — named parallel commands, all entries execute concurrently

## Examples

### String Format (shell)
```jsonc
{
  "postCreateCommand": "npm install && npm run build"
}
```
Runs through `/bin/sh -c` in the container. Shell features like `&&`, pipes, and redirects work as expected.

### Array Format (exec-style)
```jsonc
{
  "postCreateCommand": ["npm", "install", "--save-dev"]
}
```
Runs `npm` directly with `install` and `--save-dev` as arguments. No shell interpretation — arguments with spaces or special characters are passed literally.

### Object Format (parallel)
```jsonc
{
  "postCreateCommand": {
    "install": "npm install",
    "build": ["npm", "run", "build"],
    "setup": "cp .env.example .env"
  }
}
```
All three entries (`install`, `build`, `setup`) execute **concurrently**. String values run through a shell; array values run exec-style. The phase completes when all entries finish. If any entry fails, the phase fails.

### All Lifecycle Phases
All three formats work with every lifecycle command:
- `initializeCommand` (runs on host)
- `onCreateCommand`
- `updateContentCommand`
- `postCreateCommand`
- `postStartCommand`
- `postAttachCommand`

## Verification

```bash
# Build and run
cargo build
cargo run -- up --workspace-folder /path/to/project

# Run tests
make test-nextest-fast
```

## What to Watch For

- **Array commands** no longer go through a shell — if your array command relied on shell features (e.g., `["sh", "-c", "echo $HOME"]`), it still works because `sh` is the executable
- **Object commands** now run concurrently — if your object entries depend on each other's completion order, restructure them into separate lifecycle phases
- Existing string-format commands are unaffected
