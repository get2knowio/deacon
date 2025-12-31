# Doctor Command Example: Host Requirements and Storage

## What This Demonstrates

This example shows how to use the `deacon doctor` command to collect system diagnostics, with specific focus on:

- **Host requirements validation**: Understanding how `hostRequirements` in `devcontainer.json` relate to actual system resources
- **Storage availability checks**: Real disk space reporting from the current working directory
- **System diagnostics**: Comprehensive environment information including Docker, OS, and platform details
- **Output formats**: Both human-readable text and machine-readable JSON formats

## Why This Matters

The doctor command is essential for:
- **Troubleshooting**: Quickly collect system information when debugging environment issues
- **Pre-validation**: Check if the host system meets the requirements before attempting to start containers
- **Support bundles**: Create comprehensive diagnostic bundles for issue reporting
- **CI/CD validation**: Verify build environment capabilities in automated pipelines

## DevContainer Configuration

The included `devcontainer.json` specifies host requirements that will be validated when starting the container:

```json
{
  "hostRequirements": {
    "cpus": 2,
    "memory": "4GB",
    "storage": "10GB"
  }
}
```

These requirements are checked against actual system resources. The doctor command shows you what resources are available.

## Run

### Text Mode (Human-Readable)

Run the doctor command to see a formatted diagnostic report:

```sh
deacon doctor --workspace-folder .
```

Expected output sections:
- **CLI Version**: Deacon version information
- **Host OS**: Operating system name, version, and architecture
- **Platform**: Platform type, WSL status, capability support
- **Docker**: Installation status, version, daemon health, resource usage
- **Disk Space**: Total, available, and used storage for the current directory
- **Configuration Discovery**: Found configuration files

Example output:
```
Deacon Doctor Diagnostics
========================

CLI Version: 0.1.0

Host OS:
  Name: linux
  Version: Ubuntu 22.04.1 LTS
  Architecture: x86_64

...

Disk Space:
  Total: 25.6 GiB
  Available: 20.5 GiB
  Used: 5.1 GiB
```

### JSON Mode (Machine-Readable)

For programmatic access or integration with other tools:

```sh
deacon doctor --workspace-folder . --json
```

This outputs a JSON object with all diagnostic information. Key fields related to storage:

```json
{
  "disk_space": {
    "total_bytes": 27538529280,
    "available_bytes": 22030823424,
    "used_bytes": 5507705856
  }
}
```

You can pipe this to `jq` for filtering:

```sh
# Check just storage information
deacon doctor --workspace-folder . --json 2>/dev/null | jq '.disk_space'

# Check if available storage meets the 10GB requirement
deacon doctor --workspace-folder . --json 2>/dev/null | \
  jq '.disk_space.available_bytes >= (10 * 1024 * 1024 * 1024)'
```

## Understanding Storage Thresholds

### Storage Calculation

The doctor command reports storage for the **current working directory** (or the directory specified with `--workspace-folder`). This is the same location where container workspace folders and volumes would be created.

The storage check uses platform-specific APIs:
- **Linux/Unix**: `df` command to query filesystem statistics
- **macOS**: `df -k` with kilobyte units
- **Windows**: PowerShell `Get-PSDrive` cmdlet

### Host Requirements Validation

When you run `deacon up`, the CLI validates that your system meets the `hostRequirements` specified in `devcontainer.json`:

```sh
# This will fail if your system has < 10GB available storage
deacon up --workspace-folder .

# You can bypass the check if needed (not recommended)
deacon up --workspace-folder . --ignore-host-requirements
```

If requirements are not met, you'll see an error like:
```
Error: Host requirements not met:
  - Storage: Required 10 GB, available 8.5 GB
```

### Expected Messages

| Scenario | Message/Behavior |
|----------|------------------|
| Requirements met | Container starts normally, no warnings |
| Insufficient storage | Error: "Host requirements not met: Storage: Required X, available Y" |
| No requirements specified | No validation performed, container starts |
| Requirements bypassed with flag | Warning logged, container starts anyway |

### Storage Thresholds

The `hostRequirements.storage` field accepts various formats:
- **Numeric with unit**: `"10GB"`, `"500MB"`, `"1TB"`
- **Binary units**: `"10GiB"`, `"500MiB"` (1024-based)
- **Decimal units**: `"10GB"`, `"500MB"` (1000-based)

Example configurations:

```json
// Minimal storage - suitable for lightweight containers
{
  "hostRequirements": {
    "storage": "1GB"
  }
}

// Moderate storage - typical development environment
{
  "hostRequirements": {
    "storage": "10GB"
  }
}

// Large storage - for builds with many dependencies
{
  "hostRequirements": {
    "storage": "50GB"
  }
}
```

## Creating Support Bundles

The doctor command can create a ZIP bundle containing all diagnostic information and configuration files:

```sh
deacon doctor --workspace-folder . --bundle /tmp/support-bundle.zip
```

This bundle includes:
- `doctor.json`: Complete diagnostic information
- Configuration files from the workspace (if found)

## Offline Usage

The doctor command works **completely offline** - it only checks local system resources and doesn't require network access. This makes it ideal for:
- Air-gapped environments
- CI/CD systems without internet access
- Quick local diagnostics without external dependencies

## Spec Reference

This example aligns with the CLI specification sections:
- **Host Requirements**: System resource validation (CPU, memory, storage)
- **Configuration Resolution**: Discovery of devcontainer.json files
- **Doctor Command**: Environment diagnostics and troubleshooting

## Troubleshooting

### Storage appears as 0 bytes

If the doctor command shows 0 bytes for storage, this indicates the platform-specific storage check failed. Possible causes:
- Path doesn't exist or isn't accessible
- Unsupported filesystem type
- Permission issues accessing filesystem information

Check the logs with `--log-level debug` for more details:
```sh
deacon doctor --workspace-folder . --log-level debug
```

### Docker information unavailable

If Docker information shows as "not installed" or "daemon not running", the doctor command continues and reports what it can. This is expected in environments without Docker.

### Large storage requirements not met

If your configuration specifies large storage requirements (e.g., 100GB) but the check fails:
1. Verify you're checking the right directory with `--workspace-folder`
2. Consider using `--ignore-host-requirements` temporarily for testing
3. Clean up unnecessary files to free space
4. Adjust the `hostRequirements.storage` value if it's too conservative
