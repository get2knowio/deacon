# Lockfile Demo Example

This example demonstrates the lockfile functionality for DevContainer feature version management.

## Files

- `devcontainer.json` - DevContainer configuration with features
- `devcontainer-lock.json` - Lockfile with resolved feature versions and integrity information

## Purpose

The lockfile ensures reproducible builds by pinning exact versions and digests of features used in the DevContainer configuration.

## Lockfile Structure

Each feature in the lockfile contains:
- `version`: Semantic version of the feature (e.g., "1.5.0")
- `resolved`: Full OCI reference with digest for integrity verification
- `integrity`: SHA256 digest for additional integrity checking
- `depends_on` (optional): List of feature dependencies

## Usage

The lockfile is automatically read by the `deacon` CLI when building containers to ensure consistent feature versions across builds and environments.

To generate or update a lockfile, use the `upgrade` command (when implemented):
```bash
deacon upgrade
```

To check for outdated features, use the `outdated` command (when implemented):
```bash
deacon outdated
```

## Path Derivation

Lockfiles follow these naming rules:
- If the config file starts with `.` (e.g., `.devcontainer.json`) → `.devcontainer-lock.json`
- Otherwise (e.g., `devcontainer.json`) → `devcontainer-lock.json`
- Location: Same directory as the config file
