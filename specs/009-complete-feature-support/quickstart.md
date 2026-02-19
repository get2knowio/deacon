# Quickstart: Complete Feature Support During Up Command

**Feature**: 009-complete-feature-support
**Date**: 2025-12-28

## Overview

This guide explains how to use the complete feature support capabilities in `deacon up`, including:
- Feature security options (privileged, init, capAdd, securityOpt)
- Feature lifecycle commands
- Feature mounts and entrypoints
- Local and HTTPS feature references

---

## Feature Security Options

Features can declare security options that are automatically merged with your config.

### Example: Docker-in-Docker Feature

```json
{
    "image": "mcr.microsoft.com/devcontainers/base:ubuntu",
    "features": {
        "ghcr.io/devcontainers/features/docker-in-docker:2": {}
    }
}
```

The `docker-in-docker` feature declares `"privileged": true` in its metadata. When you run `deacon up`, the container is automatically created with `--privileged`.

### Merge Behavior

| Option | Merge Rule |
|--------|------------|
| `privileged` | OR: true if ANY source (config or feature) declares true |
| `init` | OR: true if ANY source declares true |
| `capAdd` | Union: all capabilities merged, deduplicated, uppercase |
| `securityOpt` | Union: all options merged, deduplicated |

### Example: Explicit Security Options

```json
{
    "image": "mcr.microsoft.com/devcontainers/base:ubuntu",
    "features": {
        "ghcr.io/my-org/features/debugger:1": {}
    },
    "capAdd": ["NET_ADMIN"]
}
```

If the `debugger` feature declares `"capAdd": ["SYS_PTRACE"]`, the final container gets both:
```bash
docker create --cap-add=NET_ADMIN --cap-add=SYS_PTRACE ...
```

---

## Feature Lifecycle Commands

Features can define lifecycle commands that run BEFORE your config's commands.

### Execution Order

1. Feature lifecycle commands (in installation order)
2. Config lifecycle commands

### Example

**Feature metadata** (devcontainer-feature.json):
```json
{
    "id": "my-sdk",
    "postCreateCommand": "sdk setup"
}
```

**Your config** (devcontainer.json):
```json
{
    "image": "mcr.microsoft.com/devcontainers/base:ubuntu",
    "features": {
        "ghcr.io/my-org/features/my-sdk:1": {}
    },
    "postCreateCommand": "npm install"
}
```

**Execution during `deacon up`**:
1. `sdk setup` (from feature)
2. `npm install` (from config)

### Fail-Fast Behavior

If any lifecycle command fails (exits non-zero), `deacon up` stops immediately with exit code 1. Remaining lifecycle commands are skipped.

---

## Feature Mounts

Features can declare mounts that are merged with your config mounts.

### Precedence

Config mounts take precedence over feature mounts for the same target path.

### Example

**Feature metadata**:
```json
{
    "id": "cache-feature",
    "mounts": ["type=volume,source=build-cache,target=/cache"]
}
```

**Your config**:
```json
{
    "image": "ubuntu",
    "features": {
        "ghcr.io/my-org/features/cache-feature:1": {}
    },
    "mounts": ["type=bind,source=${localWorkspaceFolder}/my-cache,target=/cache"]
}
```

**Result**: Your bind mount to `/cache` is used (config takes precedence).

---

## Feature Entrypoints

Features can declare entrypoint wrappers that run during container startup.

### Chaining

When multiple features have entrypoints:
1. A wrapper script is generated
2. Entrypoints execute in installation order
3. User command runs last via `exec "$@"`

### Example

**Feature 1** (installed first):
```json
{
    "id": "docker-in-docker",
    "entrypoint": "/usr/local/share/docker-init.sh"
}
```

**Feature 2** (installed second):
```json
{
    "id": "ssh-agent",
    "entrypoint": "/usr/local/share/ssh-init.sh"
}
```

**Generated wrapper**:
```bash
#!/bin/sh
/usr/local/share/docker-init.sh || exit $?
/usr/local/share/ssh-init.sh || exit $?
exec "$@"
```

---

## Local Feature References

Reference features from your repository using relative paths.

### Path Format

- `./feature-name` - Relative to devcontainer.json directory
- `../shared/feature` - Can reference parent directories

### Directory Structure

```
project/
├── .devcontainer/
│   ├── devcontainer.json
│   └── my-feature/
│       ├── devcontainer-feature.json
│       └── install.sh
└── shared-features/
    └── common/
        ├── devcontainer-feature.json
        └── install.sh
```

### Configuration

```json
{
    "image": "ubuntu",
    "features": {
        "./my-feature": {},
        "../shared-features/common": {"version": "1.0"}
    }
}
```

### Requirements

Each local feature directory MUST contain:
- `devcontainer-feature.json` - Feature metadata

---

## HTTPS Feature References

Reference features via direct HTTPS URLs to tarballs.

### URL Format

```json
{
    "image": "ubuntu",
    "features": {
        "https://example.com/releases/my-feature-1.0.tgz": {}
    }
}
```

### Tarball Structure

The tarball must contain at its root:
```
devcontainer-feature.json
install.sh (optional)
... other files ...
```

### Download Behavior

- Timeout: 30 seconds
- Retry: Once on transient network errors (5xx, timeout)
- No retry on: 404, 401, 403, TLS errors

---

## Error Messages

### Feature Not Found

```
Error: Local feature not found: ./missing-feature
```

**Fix**: Check the path is correct relative to devcontainer.json.

### Missing Metadata

```
Error: Missing devcontainer-feature.json in: ./my-feature
```

**Fix**: Add a devcontainer-feature.json to your local feature directory.

### Lifecycle Command Failed

```
Error: Lifecycle command exited with code 1 (feature:my-sdk)
```

**Fix**: Check the feature's lifecycle command for errors. The error identifies which feature failed.

### HTTPS Download Failed

```
Error: Feature not found: https://example.com/feature.tgz
```

**Fix**: Verify the URL is correct and accessible.

---

## Debugging

### View Merged Configuration

```bash
deacon read-configuration --workspace-folder . --output json | jq
```

### View Feature Resolution

```bash
DEACON_LOG=debug deacon up 2>&1 | grep -i feature
```

### Check Container Security Options

After `deacon up`:
```bash
docker inspect <container_id> | jq '.[0].HostConfig.Privileged'
docker inspect <container_id> | jq '.[0].HostConfig.CapAdd'
```

---

## Best Practices

1. **Pin feature versions**: Use tags like `:1` or `:1.0.0` instead of `:latest`
2. **Test features locally first**: Develop features in `./` before publishing
3. **Keep lifecycle commands idempotent**: They may run on container rebuild
4. **Document security requirements**: If your feature needs `privileged`, explain why
5. **Use entrypoints sparingly**: Only when environment setup is truly needed at startup
