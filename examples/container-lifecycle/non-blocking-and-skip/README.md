# Non-Blocking and Skip Flags Example

## What This Demonstrates

This example shows how to use skip flags to control which lifecycle commands execute during container creation:

- **`--skip-post-create`**: Skip the postCreate phase
- **`--skip-non-blocking-commands`**: Skip postStart and postAttach phases (non-blocking commands)

## Lifecycle Phases Overview

According to the [Up SPEC](../../../docs/subcommand-specs/up/SPEC.md#2-command-line-interface), lifecycle commands execute in this order:

1. **onCreate** - Always runs during initial container creation
2. **postCreate** - Runs after features are installed (can be skipped)
3. **postStart** - Runs when container starts (non-blocking, can be skipped)
4. **postAttach** - Runs when attaching to container (non-blocking, can be skipped)

## Skip Flags Behavior

### `--skip-post-create`
Skips the postCreate phase entirely. Useful when:
- Iterating on onCreate commands
- Testing without running expensive dependency installation
- Debugging early lifecycle phases

### `--skip-non-blocking-commands`
Skips both postStart and postAttach phases. Useful when:
- You want faster container startup
- Testing without service initialization
- Pre-build scenarios where user interaction isn't needed

## Testing the Examples

### 1. Normal Execution (All Commands Run)

```bash
# Parse configuration to see all lifecycle commands
deacon read-configuration --config devcontainer.json | jq '{
  onCreate: .onCreateCommand,
  postCreate: .postCreateCommand,
  postStart: .postStartCommand,
  postAttach: .postAttachCommand
}'
```

Expected output shows all four lifecycle phases configured.

### 2. With --skip-post-create Flag

```bash
# In a real scenario (requires Docker):
# deacon up --skip-post-create --workspace-folder .
#
# What happens:
# - onCreate runs: creates /tmp/markers/ directory and onCreate marker
# - postCreate skipped: no postCreate marker created
# - postStart runs: creates postStart marker
# - postAttach runs: creates postAttach marker
#
# Verify markers created:
# docker exec <container-id> ls -1 /tmp/markers/
# Expected: onCreate, postStart, postAttach (NO postCreate)
```

When using `--skip-post-create`:
- onCreate marker: ✅ Created
- postCreate marker: ❌ **Skipped**
- postStart marker: ✅ Created
- postAttach marker: ✅ Created

### 3. With --skip-non-blocking-commands Flag

```bash
# In a real scenario (requires Docker):
# deacon up --skip-non-blocking-commands --workspace-folder .
#
# What happens:
# - onCreate runs: creates /tmp/markers/ directory and onCreate marker
# - postCreate runs: creates postCreate marker
# - postStart skipped: no postStart marker created
# - postAttach skipped: no postAttach marker created
#
# Verify markers created:
# docker exec <container-id> ls -1 /tmp/markers/
# Expected: onCreate, postCreate (NO postStart or postAttach)
```

When using `--skip-non-blocking-commands`:
- onCreate marker: ✅ Created
- postCreate marker: ✅ Created
- postStart marker: ❌ **Skipped**
- postAttach marker: ❌ **Skipped**

### 4. Combining Both Flags

```bash
# In a real scenario (requires Docker):
# deacon up --skip-post-create --skip-non-blocking-commands --workspace-folder .
#
# What happens:
# - onCreate runs: creates /tmp/markers/ directory and onCreate marker
# - postCreate skipped
# - postStart skipped
# - postAttach skipped
#
# Verify markers created:
# docker exec <container-id> ls -1 /tmp/markers/
# Expected: onCreate only
```

When combining both skip flags:
- onCreate marker: ✅ Created
- postCreate marker: ❌ **Skipped**
- postStart marker: ❌ **Skipped**
- postAttach marker: ❌ **Skipped**

## Use Cases

### Development Iteration
When modifying onCreate commands, skip later phases for faster testing:
```bash
deacon up --skip-post-create --skip-non-blocking-commands
```

### Pre-build Scenarios
Skip non-blocking commands when building images:
```bash
deacon up --skip-non-blocking-commands --prebuild
```

### Debugging
Isolate specific phases by skipping others:
```bash
# Test only onCreate and postCreate
deacon up --skip-non-blocking-commands

# Test only onCreate
deacon up --skip-post-create --skip-non-blocking-commands
```

## Verification Strategy

This example creates marker files in `/tmp/markers/` to demonstrate which phases executed:

1. Check which markers exist after container creation
2. Compare against expected markers based on skip flags used
3. Verify phase execution matches Up SPEC behavior

## Key Takeaways

- **onCreate always runs** - Cannot be skipped (fundamental container setup)
- **postCreate is blocking** - Skip with `--skip-post-create` when not needed
- **postStart/postAttach are non-blocking** - Skip together with `--skip-non-blocking-commands`
- **Skip flags enable faster iteration** - Essential for development workflows
- **Marker files provide evidence** - Simple way to verify which phases executed

## References

- [Up SPEC: Lifecycle controls](../../../docs/subcommand-specs/up/SPEC.md#2-command-line-interface)
- [DevContainer Lifecycle Scripts Specification](https://containers.dev/implementors/spec/#lifecycle-scripts)
- Related tests: `crates/deacon/tests/smoke_up_idempotent.rs`
