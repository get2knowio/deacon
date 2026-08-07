# Non-Blocking and Skip Flags Example

## What This Demonstrates

This example shows how to use skip flags to control which lifecycle commands execute during container creation:

- **`--skip-post-create`**: Defer **every** lifecycle phase (onCreate through postAttach) plus dotfiles
- **`--skip-non-blocking-commands`**: Stop after the configured `waitFor` phase (default `updateContentCommand`)

## Lifecycle Phases Overview

According to the [Up SPEC](https://containers.dev/implementors/spec/), lifecycle commands execute in this order:

1. **onCreate** - Always runs during initial container creation
2. **postCreate** - Runs after features are installed (can be skipped)
3. **postStart** - Runs when container starts (non-blocking, can be skipped)
4. **postAttach** - Runs when attaching to container (non-blocking, can be skipped)

## Skip Flags Behavior

### `--skip-post-create`
Despite its name, this defers the **whole** lifecycle — `onCreate`, `updateContent`,
`postCreate`, `postStart`, `postAttach` and dotfiles — matching the reference
DevContainers CLI. The container is still created and Features are still installed;
only the hooks wait. Run them later with `deacon run-user-commands`. Useful when:
- You want a container quickly and will run the setup yourself
- Testing without running expensive dependency installation
- Building a container in CI whose hooks belong to a later step

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
# - every phase is deferred; no marker is created at all
#
# Verify (the directory the onCreate hook would have made does not exist):
# docker exec <container-id> ls -1 /tmp/markers/
# Expected: no such file or directory
#
# Then run the deferred hooks:
# deacon run-user-commands --workspace-folder .
# docker exec <container-id> ls -1 /tmp/markers/
# Expected: onCreate, postCreate, postStart, postAttach
```

When using `--skip-post-create`:
- onCreate marker: ❌ **Deferred**
- postCreate marker: ❌ **Deferred**
- postStart marker: ❌ **Deferred**
- postAttach marker: ❌ **Deferred**

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
# - --skip-post-create already defers everything, so the second flag adds nothing
#
# Verify markers created:
# docker exec <container-id> ls -1 /tmp/markers/
# Expected: no such file or directory
```

When combining both skip flags:
- onCreate marker: ❌ **Deferred**
- postCreate marker: ❌ **Deferred**
- postStart marker: ❌ **Deferred**
- postAttach marker: ❌ **Deferred**

## Use Cases

### Development Iteration
When modifying onCreate commands, stop at `waitFor` so the later phases don't run:
```bash
deacon up --skip-non-blocking-commands
```

### Pre-build Scenarios
Skip non-blocking commands when building images:
```bash
deacon up --skip-non-blocking-commands --prebuild
```

### Debugging
Isolate specific phases by skipping others:
```bash
# Test only onCreate and updateContent (the default waitFor cutoff)
deacon up --skip-non-blocking-commands

# Run no hook at all, then run them on demand
deacon up --skip-post-create
deacon run-user-commands
```

## Verification Strategy

This example creates marker files in `/tmp/markers/` to demonstrate which phases executed:

1. Check which markers exist after container creation
2. Compare against expected markers based on skip flags used
3. Verify phase execution matches Up SPEC behavior

## Key Takeaways

- **`--skip-post-create` defers everything** - the flag's name is narrower than its
  effect; it gates the entire lifecycle runner, exactly as the reference CLI's does
- **Deferred is not lost** - `deacon run-user-commands` runs the deferred phases
- **`--skip-non-blocking-commands` is a different rule** - it stops at the configured
  `waitFor` phase (default `updateContentCommand`), so onCreate and updateContent still run
- **Skip flags enable faster iteration** - Essential for development workflows
- **Marker files provide evidence** - Simple way to verify which phases executed

## References

- [Up SPEC: Lifecycle controls](https://containers.dev/implementors/spec/)
- [DevContainer Lifecycle Scripts Specification](https://containers.dev/implementors/spec/#lifecycle-scripts)
- Related tests: `crates/deacon/tests/smoke_up_idempotent.rs`
