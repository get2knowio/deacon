# Quickstart: Read-Configuration Spec Parity

This guide shows how to exercise the updated `read-configuration` command behaviors.

## Basic configuration

```bash
# Resolve configuration from a workspace folder
deacon read-configuration --workspace-folder examples/configuration/basic
```

Expect: stdout JSON contains `configuration` only; logs go to stderr.

## Include merged configuration

```bash
# Include merged view without a running container
deacon read-configuration --workspace-folder examples/configuration/basic --include-merged-configuration
```

Expect: stdout JSON contains `configuration` and `mergedConfiguration`.

## Include features configuration

```bash
# Force feature resolution output
deacon read-configuration --workspace-folder examples/features/minimal-feature --include-features-configuration
```

Expect: stdout JSON contains `configuration` and `featuresConfiguration`.

## Container-aware mode

```bash
# Using an explicit container id
deacon read-configuration --container-id <id> --include-merged-configuration

# Using id-label(s)
deacon read-configuration --id-label devcontainer.local_folder=/workspaces/myproj --include-merged-configuration
```

Expect: before-container `${devcontainerId}` resolves. When merged is requested and container is selected, merged uses container-derived metadata; if inspect fails → error (no fallback).

## Validation errors

```bash
# Missing selector
deacon read-configuration  # Error: One of --container-id, --id-label or --workspace-folder is required.

# Invalid id-label
deacon read-configuration --id-label invalid  # Error: id-label must match <name>=<value>.

# Terminal dimension pairing
deacon read-configuration --terminal-columns 120  # Error: terminal dimensions must be paired.
```
