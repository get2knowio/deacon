# CLI Port Forwarding Example

This example demonstrates using the `--forward-port` CLI flag to forward ports from a container to the host.

## Overview

The devcontainer configuration defines port attributes for port 80 (nginx), but doesn't include it in `forwardPorts`. Instead, we use the CLI flag to forward ports dynamically.

## Usage

### Forward a single port

Forward port 8080 from container to host:

```bash
deacon up --workspace-folder . --forward-port 8080
```

### Forward multiple ports

Forward multiple ports by repeating the flag:

```bash
deacon up --workspace-folder . --forward-port 8080 --forward-port 3000
```

### Map host port to different container port

Forward host port 8080 to container port 80:

```bash
deacon up --workspace-folder . --forward-port 8080:80
```

### Combine with config ports

If the devcontainer.json has `forwardPorts` defined, CLI ports are merged:

```bash
# This would forward both 80 (from config) and 8080 (from CLI)
deacon up --workspace-folder . --forward-port 8080
```

### View port events

Use `--ports-events` to see port forwarding events:

```bash
deacon up --workspace-folder . --forward-port 8080:80 --ports-events
```

This will emit `PORT_EVENT:` lines with JSON data about forwarded ports.

## Port Event Format

Port events are emitted to stdout with the prefix `PORT_EVENT:` followed by JSON:

```json
{
  "port": 8080,
  "protocol": "tcp",
  "label": "Nginx Web Server",
  "onAutoForward": "notify",
  "autoForwarded": true,
  "localPort": 8080,
  "hostIp": "0.0.0.0",
  "description": "Main web server port"
}
```

## Benefits of CLI Port Forwarding

1. **Dynamic Configuration**: Forward ports without modifying devcontainer.json
2. **Environment-Specific**: Different port mappings for different environments
3. **Temporary Testing**: Test port configurations without committing changes
4. **Conflict Resolution**: Map to different host ports when default ports are in use

## See Also

- [Port Configuration Documentation](../../../docs/subcommand-specs/*/SPEC.md#http-and-network-handling)
- [Compose Port Events Example](../../compose/port-events/)
