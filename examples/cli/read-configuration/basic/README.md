# Basic read-configuration Example

This example demonstrates basic usage of the `read-configuration` command.

## Usage

```bash
deacon read-configuration --workspace-folder . --config devcontainer.json
```

## Expected Output

The command will output a JSON object with:
- `configuration`: The parsed DevContainer configuration
- `workspace`: Workspace path information

## What It Demonstrates

- Basic configuration reading
- Variable substitution (${localWorkspaceFolderBasename})
- Workspace information output
