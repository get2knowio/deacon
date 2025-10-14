# Extends Chain Configuration Example

## What This Demonstrates

This example showcases the powerful configuration inheritance and layering capabilities of DevContainers through the `extends` mechanism. It demonstrates:

- **Multi-level extends chain**: Base → Middle → Leaf configuration inheritance (3 layers)
- **Deep merge semantics**: How objects merge deeply while scalars override
- **Environment variable overrides**: Same key (`SHARED_VAR`) overridden at each layer
- **Features map merging**: Features from all layers combined by key
- **Array replacement**: Arrays like `forwardPorts` replace rather than concatenate (note: append/prepend directives planned per spec)
- **Customizations deep merge**: VSCode extensions and settings combined across layers
- **Lifecycle command inheritance**: Commands defined at different layers
- **Cycle detection**: Demonstrates error handling for circular extends (see `cycle.json`)

## Why This Matters

Configuration layering through extends is essential for:
- **Organizational standards**: Define base configurations with company-wide tooling and policies
- **Project templates**: Create mid-level templates for different project types (web, mobile, data science)
- **Team customization**: Allow teams to extend organizational standards with project-specific needs
- **Reduced duplication**: Share common configuration elements across multiple projects
- **Maintainability**: Update base configurations to affect all extending configurations
- **Progressive enhancement**: Start simple and layer on complexity as needed

## DevContainer Specification References

This example implements key aspects from the [DevContainer Specification](https://containers.dev/implementors/spec/):

- **[Configuration Resolution Workflow](https://containers.dev/implementors/spec/#extends)**: Complete extends chain resolution with merge semantics
- **[Extends](https://containers.dev/implementors/spec/#extends)**: Single and multi-level configuration inheritance
- **[Merge Logic](https://containers.dev/implementors/spec/#merge-logic)**: Deep merge for objects, replacement for arrays and scalars
- **[Features](https://containers.dev/implementors/spec/#features)**: Feature map merging across configuration layers
- **[Environment Variables](https://containers.dev/implementors/spec/#environment-variables)**: Environment variable precedence and overrides

## File Structure

```
extends-chain/
├── base/
│   └── devcontainer.json       # Base configuration (lowest precedence)
├── mid/
│   └── devcontainer.json       # Middle layer (extends base)
├── leaf/
│   └── devcontainer.json       # Leaf configuration (extends mid, highest precedence)
├── cycle.json                  # Demonstrates cycle detection (self-referential)
└── README.md                   # This file
```

## Configuration Layers

### Base Layer (`base/devcontainer.json`)
Defines foundational settings:
- Base Ubuntu 22.04 image
- Common utilities feature (zsh)
- Base environment variables
- Port 3000 forwarding
- Basic VSCode TypeScript extension

### Middle Layer (`mid/devcontainer.json`)
Extends base with additional capabilities:
- Git feature added
- Overrides `SHARED_VAR` environment variable
- Adds privileged flag to runArgs
- Additional port 8080 forwarding
- onCreate lifecycle command

### Leaf Layer (`leaf/devcontainer.json`)
Final project-specific configuration:
- Node.js 18 feature added
- Workspace folder specified
- Final override of `SHARED_VAR`
- Port 5000 forwarding
- npm install postCreate command
- ESLint VSCode extension
- Format on save setting

## Expected Merge Results

After merging all three layers, the final configuration should have:

**Name**: "Leaf Container Configuration" (leaf overrides all)

**Image**: "ubuntu:22.04" (from base, not overridden)

**Container Environment Variables**:
- `BASE_VAR`: "base_value" (from base)
- `MIDDLE_VAR`: "middle_value" (from mid)
- `LEAF_VAR`: "leaf_value" (from leaf)
- `SHARED_VAR`: "from_leaf" (leaf overrides mid and base)

**Features** (merged map):
- `ghcr.io/devcontainers/features/common-utils:1`: `{"installZsh": true}` (from base)
- `ghcr.io/devcontainers/features/git:1`: `{"version": "latest"}` (from mid)
- `ghcr.io/devcontainers/features/node:1`: `{"version": "18"}` (from leaf)

**Run Args** (array concatenation):
- `["--init", "--privileged"]` (base's `--init` + mid's `--privileged`)
  
**Forward Ports** (array replacement):
- `[5000]` (leaf replaces all previous port arrays)

**Customizations** (deep object merge):
- VSCode extensions: `["dbaeumer.vscode-eslint"]` (leaf replaces base array)
- VSCode settings: `{"editor.formatOnSave": true}` (from leaf)

**Lifecycle Commands**:
- `onCreateCommand`: "echo 'Middle layer setup'" (from mid)
- `postCreateCommand`: "npm install" (from leaf)

## Try It

### Basic Configuration Read
Load and display the merged leaf configuration:
```sh
cd examples/configuration/extends-chain/leaf
deacon read-configuration --config devcontainer.json
```

### With Merged Configuration Metadata
Include layer provenance tracking to see `__meta.layers[]` with source paths and hashes:
```sh
cd examples/configuration/extends-chain/leaf
deacon read-configuration --config devcontainer.json --include-merged-configuration
```

The output will include a `__meta` section showing:
- Each configuration layer's source path
- SHA-256 hash of each layer for integrity checking
- Precedence order (0 = base, 1 = mid, 2 = leaf)

### Inspect Specific Merged Properties
Use `jq` to examine merged properties:

**Features Map**:
```sh
cd examples/configuration/extends-chain/leaf
deacon read-configuration --config devcontainer.json | jq '.features'
```

**Environment Variables**:
```sh
cd examples/configuration/extends-chain/leaf
deacon read-configuration --config devcontainer.json | jq '.containerEnv'
```

**Customizations** (deep merge):
```sh
cd examples/configuration/extends-chain/leaf
deacon read-configuration --config devcontainer.json | jq '.customizations'
```

**Layer Metadata** (with `--include-merged-configuration`):
```sh
cd examples/configuration/extends-chain/leaf
deacon read-configuration --config devcontainer.json --include-merged-configuration | jq '.__meta.layers'
```

### Demonstrate Cycle Detection
Attempt to load a configuration with a circular extends reference:
```sh
cd examples/configuration/extends-chain
deacon read-configuration --config cycle.json
```

This should fail with an error like:
```
Error: Configuration cycle detected: cycle.json extends itself
```

## Key Observations

1. **Scalar Override**: Simple values like `name` and `SHARED_VAR` are replaced by the highest precedence layer
2. **Object Deep Merge**: Maps like `features` and `customizations` merge deeply, combining keys from all layers
3. **Array Replacement (most arrays)**: Most arrays like `forwardPorts` and `mounts` replace completely (last writer wins)
4. **Array Concatenation (security arrays)**: Security-related arrays (`runArgs`, `capAdd`, `securityOpt`) concatenate from all layers
5. **Arrays in Deep Objects**: Arrays within merged objects (like VSCode extensions) replace completely
6. **Feature Addition**: Each layer can add new features to the features map
7. **Environment Accumulation**: Environment variables accumulate, with later layers overriding earlier ones for the same key
8. **Lifecycle Commands**: Different layers can define different lifecycle commands; they don't override unless explicitly set
9. **Cycle Prevention**: The loader detects and prevents circular extends chains

## Notes

- **Offline-friendly**: All configurations are local JSON files, no external dependencies
- **Array Merge Semantics**: Most arrays replace (last writer wins), but security arrays (`runArgs`, `capAdd`, `securityOpt`) concatenate
- **Array Merge Future**: The spec includes future directives for more flexible append/prepend array merging (not yet implemented)
- **Path Resolution**: Extends paths are resolved relative to the file containing the extends field
- **Multiple Extends**: A configuration can extend multiple files (array form), merged left-to-right
- **Provenance Tracking**: Use `--include-merged-configuration` for debugging complex extends chains

## Spec References

Per subcommand-specs/*/SPEC.md "Configuration Resolution Workflow":
- Extends chain is resolved recursively with cycle detection
- Configurations merge in order: base → overlay with later taking precedence
- Objects deep merge, arrays and scalars replace
- Feature maps merge by key
- Environment maps override by key
