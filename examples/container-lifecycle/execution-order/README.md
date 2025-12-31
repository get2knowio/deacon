# Lifecycle Execution Order Example

## What This Demonstrates

This example clearly shows the exact order in which DevContainer lifecycle commands execute, with timestamps and logging to visualize the sequence.

## Lifecycle Command Execution Order

According to the [DevContainer specification](https://containers.dev/implementors/spec/#lifecycle-scripts) and [Up SPEC](../../../docs/subcommand-specs/up/SPEC.md), the complete execution order is:

1. **initializeCommand** *(Host-side, not shown in this example)*
2. **onCreateCommand** ← *STEP 1*
3. **updateContentCommand** *(Content sync, not shown in this example)*
4. **postCreateCommand** ← *STEP 2*
5. **postStartCommand** ← *STEP 3*
6. **postAttachCommand** ← *STEP 4*

## Command Timing and Frequency

| Command | When It Runs | Frequency |
|---------|-------------|-----------|
| `onCreateCommand` | During initial container creation | **Once** - only when container is first created |
| `postCreateCommand` | After container creation + features | **Once** - only after initial setup |
| `postStartCommand` | When container starts/restarts | **Every time** - on each container start |
| `postAttachCommand` | When attaching to container | **Every time** - on each attach/connect |

## Key Execution Context

- **Sequential**: Commands run in the exact order shown above
- **Blocking**: Each command must complete before the next begins
- **Environment**: All commands run with full container environment variables
- **Working Directory**: Commands execute in the container's workspace folder
- **User Context**: Commands typically run as the configured remote user

## Example Output

When this configuration is used, you'll see output like:

```
STEP 1: onCreate - Wed Oct 25 10:15:30 UTC 2023
onCreate: Container is being created for the first time

STEP 2: postCreate - Wed Oct 25 10:15:32 UTC 2023
postCreate: Container created, features installed
postCreate: This runs after features but before container starts

STEP 3: postStart - Wed Oct 25 10:15:34 UTC 2023
postStart: Container has started and is running
postStart: This runs every time the container starts

STEP 4: postAttach - Wed Oct 25 10:15:36 UTC 2023
postAttach: Ready to attach/connect to the container
=== LIFECYCLE SEQUENCE COMPLETE ===
```

## DevContainer Specification References

This example aligns with:
- **[Lifecycle Scripts](https://containers.dev/implementors/spec/#lifecycle-scripts)**: Command execution order and timing
- **[Container Creation](https://containers.dev/implementors/spec/#creation)**: When onCreate runs during container creation

## Run

Test the configuration:
```sh
deacon read-configuration --config devcontainer.json
```

View the lifecycle commands in order:
```sh
deacon read-configuration --config devcontainer.json | jq -r '
  "1. onCreate: " + (.onCreateCommand | tostring),
  "2. postCreate: " + (.postCreateCommand | tostring), 
  "3. postStart: " + (.postStartCommand | tostring),
  "4. postAttach: " + (.postAttachCommand | tostring)
'
```

## Practical Use Cases

Understanding execution order is crucial for:

### Dependency Management
- **onCreate**: Install system-level dependencies
- **postCreate**: Install project-specific dependencies  
- **postStart**: Start background services
- **postAttach**: Show status/welcome information

### Error Recovery
- **onCreate**: Critical setup that must succeed
- **postCreate**: Project setup that can be retried
- **postStart**: Service startup with health checks
- **postAttach**: User-facing notifications

### Development Workflow
- **onCreate**: Clone repositories, install tools
- **postCreate**: Build project, run initial setup
- **postStart**: Start development servers
- **postAttach**: Display helpful information, run checks

## Notes

- The minimal Alpine image is used to keep focus on lifecycle execution rather than complex features
- Each command includes a small delay (`sleep 1`) to make timing differences visible
- The sequence log (`/tmp/lifecycle-sequence.log`) provides a permanent record of execution order
- This pattern is especially useful for debugging lifecycle issues in complex development environments