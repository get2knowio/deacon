# Exec: Remote User Execution

This example demonstrates how commands execute as the configured `remoteUser` and how user identity affects file permissions and environment.

## Purpose

Commands run as the user specified by `remoteUser` in the dev container configuration (or container default). This affects:
- File ownership and permissions
- Home directory location
- User-specific environment variables
- Access to resources

## Prerequisites

- Running dev container
- Understanding of Unix user permissions

## Files

- `devcontainer.json` - Configuration with `remoteUser: node`
- `user-check.sh` - Script to verify user identity and permissions
- `shared-workspace/` - Directory to test permissions

## Usage

### Basic User Verification
```bash
deacon exec --workspace-folder . whoami
# Expected output: node
```

### Check User Details
```bash
deacon exec --workspace-folder . bash /workspace/user-check.sh
```

### User ID and Groups
```bash
deacon exec --workspace-folder . id
# Shows UID, GID, and group memberships
```

### Home Directory
```bash
deacon exec --workspace-folder . bash -c 'echo $HOME && ls -la $HOME'
```

### File Creation and Ownership
```bash
# Create file as remoteUser
deacon exec --workspace-folder . touch /workspace/shared-workspace/created-by-exec.txt

# Check ownership
deacon exec --workspace-folder . ls -l /workspace/shared-workspace/
```

### Compare with Root User
```bash
# Run as root (overriding remoteUser)
deacon exec --workspace-folder . bash -c 'whoami' --user root

# Note: --user flag support depends on implementation
```

### User-Specific Environment
```bash
# Check user environment
deacon exec --workspace-folder . env | grep -E "(USER|HOME|SHELL)"
```

### Permission Testing
```bash
# Try accessing user-owned directory
deacon exec --workspace-folder . ls -la /home/node/

# Try writing to workspace
deacon exec --workspace-folder . bash -c \
  'echo "test" > /workspace/shared-workspace/test.txt && cat /workspace/shared-workspace/test.txt'
```

## Expected Behavior

### User Identity
- Commands execute as user specified in `remoteUser` field
- If `remoteUser` not specified, uses image default (often root)
- User must exist in container's `/etc/passwd`
- User's shell and environment are initialized

### Permissions
- Created files owned by `remoteUser`
- Read/write access based on user permissions
- Cannot access resources restricted to other users (without sudo)
- Workspace typically mounted with appropriate permissions

### Environment Impact
- `$HOME` points to user's home directory
- `$USER` and `$LOGNAME` reflect remoteUser
- `$SHELL` uses user's configured shell
- User-specific PATH modifications apply

## Configuration Examples

### Default User (root)
```json
{
  "image": "debian",
  // No remoteUser specified = runs as root
}
```

### Non-Root User
```json
{
  "image": "mcr.microsoft.com/devcontainers/javascript-node:18",
  "remoteUser": "node"  // Runs as 'node' user
}
```

### Custom User
```json
{
  "image": "ubuntu",
  "remoteUser": "developer",
  "postCreateCommand": "useradd -m developer"
}
```

## Notes

- Running as non-root improves security
- Some operations (installing packages, system config) require root/sudo
- Workspace permissions must allow remoteUser access
- Home directory must exist for user
- User initialization scripts (~/.bashrc, etc.) are sourced based on `userEnvProbe`
