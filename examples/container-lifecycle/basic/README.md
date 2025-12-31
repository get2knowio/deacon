# Basic Container Lifecycle Example

## What This Demonstrates

This example showcases the fundamental container lifecycle commands execution in the correct order:

1. **onCreate**: Commands that run once when the container is first created
2. **postCreate**: Commands that run after the container is created and features are installed
3. **postStart**: Commands that run each time the container starts
4. **postAttach**: Commands that run when attaching to the container

## Lifecycle Command Execution Order

Based on the [CLI specification](../../../docs/subcommand-specs/*/SPEC.md), lifecycle commands execute in this order:

1. **initializeCommand**: Host-side initialization (not shown in this example)
2. **onCreateCommand**: Container creation setup
3. **updateContentCommand**: Content synchronization (not shown in this example)
4. **postCreateCommand**: Post-creation configuration
5. **postStartCommand**: Container startup tasks
6. **postAttachCommand**: Attachment preparation

## Key Features

- **Sequential Execution**: Each command array executes in order
- **Logging**: Commands write to `/tmp/setup-logs/` for demonstration
- **Environment Context**: Commands run with full container environment
- **Error Handling**: If any command fails, subsequent commands may not execute

## DevContainer Specification References

This example aligns with:
- **[Lifecycle Scripts](https://containers.dev/implementors/spec/#lifecycle-scripts)**: onCreate, postCreate, postStart, postAttach hooks
- **[Environment Variables](https://containers.dev/implementors/spec/#environment-variables)**: Setting container environment variables

## Run

Test the configuration parsing:
```sh
deacon read-configuration --config devcontainer.json
```

Validate lifecycle command structure:
```sh
deacon read-configuration --config devcontainer.json | jq '.onCreateCommand, .postCreateCommand, .postStartCommand, .postAttachCommand'
```

## Expected Behavior

When this configuration is used with a DevContainer CLI:

1. **Container Creation**: `onCreateCommand` runs, creating the initial log directory
2. **Feature Installation**: Common utils feature installs (managed by DevContainer CLI)
3. **Post-Creation**: `postCreateCommand` runs, installing development dependencies
4. **Container Start**: `postStartCommand` runs every time container starts
5. **Attachment**: `postAttachCommand` runs when connecting to the container, displaying welcome message

## Notes

- Commands run with appropriate user context (typically `vscode` user after feature installation)
- Output is captured and can be viewed in container logs
- Failed commands may prevent subsequent lifecycle phases from executing
- Setup logs persist in `/tmp/setup-logs/` for debugging