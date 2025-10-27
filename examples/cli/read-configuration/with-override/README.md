# Override Configuration Example

This example demonstrates using `--override-config` for configuration overrides.

## Usage

```bash
deacon read-configuration --workspace-folder . \
  --config devcontainer.json \
  --override-config override.json
```

## Expected Output

The configuration will be merged with the override taking precedence:
- `name`: "override-config" (from override)
- `image`: "mcr.microsoft.com/devcontainers/base:ubuntu" (from base)
- `containerEnv`: Both BASE_VAR and OVERRIDE_VAR will be present

## What It Demonstrates

- Configuration override mechanism
- Merge behavior (deep merge for objects)
- Override takes highest precedence

## Use Cases

- Environment-specific overrides (dev/staging/prod)
- User-specific customizations
- CI/CD pipeline modifications
- Testing configuration variations
