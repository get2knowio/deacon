# Configuration Examples

This directory contains comprehensive examples demonstrating DevContainer configuration capabilities as specified in the [DevContainer Specification](https://containers.dev/implementors/spec/) and implemented in the Deacon CLI per `docs/subcommand-specs/read-configuration/SPEC.md` and `docs/subcommand-specs/up/SPEC.md`.

## Overview

Each example is self-contained and runnable with the `deacon` CLI. They demonstrate different aspects of configuration management:

| Example | Focus Area | Key Concepts |
|---------|------------|--------------|
| `basic/` | Complete production-ready config | Features, lifecycle commands, environment setup |
| `with-variables/` | Variable substitution | Built-in variables, environment variable access |
| `extends-chain/` | Configuration inheritance | Multi-level extends, merge semantics, cycle detection |
| `nested-variables/` | Advanced substitution | Nested variables, chained evaluation, strict mode |

## Quick Start

### Basic Configuration
```bash
cd basic
deacon read-configuration --config devcontainer.jsonc
```

### Variable Substitution
```bash
cd with-variables
deacon read-configuration --config devcontainer.jsonc
```

### Extends Chain
```bash
cd extends-chain/leaf
deacon read-configuration --config devcontainer.json --include-merged-configuration
```

### Nested Variables
```bash
cd nested-variables
deacon config substitute --config devcontainer.json --dry-run
```

## Example Details

### basic/
**Purpose**: Demonstrates a complete, production-ready DevContainer configuration.

**What it shows**:
- Multiple features from the DevContainer features registry
- Lifecycle commands (onCreate, postCreate, postStart, postAttach)
- Container and remote environment variables
- Port forwarding configuration
- Volume mounts for persistent caching
- Security settings (privileges, init)

**Commands**:
```bash
deacon read-configuration --config devcontainer.jsonc
```

### with-variables/
**Purpose**: Showcases variable substitution patterns for dynamic, context-aware configurations.

**What it shows**:
- Workspace variables (`${localWorkspaceFolder}`)
- Container variables (`${devcontainerId}`)
- Environment variables (`${localEnv:VAR}`)
- Variable composition
- Missing variable handling

**Commands**:
```bash
deacon read-configuration --config devcontainer.jsonc
```

### extends-chain/
**Purpose**: Demonstrates configuration inheritance through multi-level extends chains.

**What it shows**:
- Three-layer inheritance (base → mid → leaf)
- Deep merge semantics for objects
- Array replacement vs concatenation behavior
- Environment variable overrides across layers
- Features map merging by key
- Customizations deep merge
- Lifecycle command inheritance
- Cycle detection for circular extends

**Structure**:
```
extends-chain/
├── base/devcontainer.json      # Base layer (Ubuntu, common-utils)
├── mid/devcontainer.json       # Middle layer (extends base, adds git)
├── leaf/devcontainer.json      # Leaf layer (extends mid, adds node)
├── cycle.json                  # Demonstrates cycle detection
└── README.md
```

**Commands**:
```bash
cd leaf
deacon read-configuration --config devcontainer.json --include-merged-configuration | jq '.__meta.layers'
deacon read-configuration --config devcontainer.json --include-merged-configuration | jq '.features'
```

**Merge Behavior**:
- **Objects** (features, customizations): Deep merge by key
- **Environment variables**: Accumulate, later layers override
- **Arrays** (forwardPorts, mounts): Last writer wins (replacement)
- **Security arrays** (runArgs, capAdd, securityOpt): Concatenate
- **Scalars** (name, image): Last writer wins (replacement)

### nested-variables/
**Purpose**: Demonstrates advanced variable substitution with nested and chained references.

**What it shows**:
- Nested variable references (`${containerEnv:VAR1}/path`)
- Chained evaluation (VAR1 → VAR2 → VAR3)
- Cross-context references (local, container, remote)
- Multi-pass resolution phases
- Unresolved placeholder handling
- Strict vs non-strict substitution modes

**Commands**:
```bash
# Non-strict mode (default)
deacon read-configuration --config devcontainer.json

# Strict mode (errors on unresolved variables)
deacon config substitute --config devcontainer.json --strict-substitution

# Dry run to preview substitutions
deacon config substitute --config devcontainer.json --dry-run
```

**Substitution Phases**:
1. **Pre-container**: `localWorkspaceFolder`, `localEnv:*`
2. **Post-container**: `containerEnv:*`, `remoteEnv:*`
3. **Multi-pass**: Nested references resolved iteratively

## Common Patterns

### Using Extends for Team Standards
```json
{
  "extends": "../company-base/devcontainer.json",
  "name": "My Project",
  "features": {
    "ghcr.io/devcontainers/features/node:1": {"version": "18"}
  }
}
```

### Dynamic Path Construction
```json
{
  "containerEnv": {
    "WORKSPACE_ROOT": "${localWorkspaceFolder}",
    "PROJECT_DIR": "${containerEnv:WORKSPACE_ROOT}/src",
    "LOG_DIR": "${containerEnv:PROJECT_DIR}/logs"
  }
}
```

### Environment-Specific Configuration
```json
{
  "containerEnv": {
    "ENV": "${localEnv:ENVIRONMENT}",
    "API_URL": "${localEnv:API_URL}",
    "DEBUG": "${localEnv:DEBUG}"
  }
}
```

## Validation and Testing

### Validate Configuration
```bash
deacon read-configuration --config devcontainer.json
```

### Check Merged Configuration
```bash
deacon read-configuration --config devcontainer.json --include-merged-configuration
```

### Test Variable Substitution
```bash
deacon config substitute --config devcontainer.json --dry-run
```

### Strict Mode Validation
```bash
deacon config substitute --config devcontainer.json --strict-substitution
```

## Specification References

These examples implement concepts from:

- **[Configuration Resolution Workflow](https://containers.dev/implementors/spec/#configuration-resolution)**: See Read-Configuration SPEC: ../../docs/subcommand-specs/read-configuration/SPEC.md#4-configuration-resolution
- **[Extends](https://containers.dev/implementors/spec/#extends)**: Configuration inheritance
- **[Variable Substitution](https://containers.dev/implementors/spec/#variables-in-devcontainer-json)**: Complete substitution syntax
- **[Merge Logic](https://containers.dev/implementors/spec/#merge-logic)**: Deep merge for objects, replacement for arrays
- **[Features](https://containers.dev/implementors/spec/#features)**: Feature installation and configuration
- **[Lifecycle Scripts](https://containers.dev/implementors/spec/#lifecycle-scripts)**: Command execution phases

## Tips and Best Practices

1. **Start Simple**: Begin with `basic/` to understand core configuration
2. **Use Variables**: Leverage `with-variables/` patterns for portability
3. **Layer Configurations**: Use `extends-chain/` for organizational standards
4. **Validate Early**: Test configurations with `read-configuration` before building
5. **Use Strict Mode**: Catch substitution errors early in development
6. **Document Layers**: Use `--include-merged-configuration` to debug extends chains
7. **Keep Examples Updated**: These examples serve as living documentation

## Related Documentation

- Main examples index: `../README.md`
- CLI specifications: `../../docs/subcommand-specs/read-configuration/SPEC.md`, `../../docs/subcommand-specs/up/SPEC.md`
- DevContainer spec: https://containers.dev/implementors/spec/
- Container lifecycle examples: `../container-lifecycle/`
- Feature examples: `../feature-management/`
- Template examples: `../template-management/`
