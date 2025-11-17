# Exec: ID Label Targeting

This example demonstrates using `--id-label` to target containers by their assigned labels, which is useful when multiple containers exist.

## Purpose

Labels provide semantic targeting without needing to know container IDs. The `devcontainer.local_folder` label is automatically assigned by the dev container CLI and can be used to identify containers by workspace.

## Prerequisites

- A running dev container
- Understanding of Docker label syntax

## Files

- `devcontainer.json` - Configuration with custom labels
- `app.py` - Sample Python application

## Usage

1. Start the dev container:
   ```bash
   deacon up --workspace-folder .
   ```

2. Execute using the workspace label (most common):
   ```bash
   deacon exec --id-label "devcontainer.local_folder=$(pwd)" python3 /workspace/app.py
   ```

3. Execute using custom label:
   ```bash
   deacon exec --id-label "app.name=exec-example" python3 --version
   ```

4. Use multiple labels for precise targeting:
   ```bash
   deacon exec \
     --id-label "devcontainer.local_folder=$(pwd)" \
     --id-label "app.environment=development" \
     whoami
   ```

5. List environment variables:
   ```bash
   deacon exec --id-label "app.name=exec-example" env | sort
   ```

## Expected Behavior

- Container is located by matching ALL provided labels
- If multiple containers match, the first one found is used
- If no containers match, error: "Dev container not found."
- Labels are matched exactly (case-sensitive)

## Label Format

Labels must follow the format `name=value`:
- Name and value must both be non-empty
- Multiple `--id-label` flags are ANDed together
- Invalid format results in parse error

## Notes

- The `devcontainer.local_folder` label uses absolute paths
- Custom labels defined in `containerLabels` are also searchable
- Labels are more stable than container IDs across recreations
