# Port Events with Docker Compose Example

## What This Demonstrates

This example shows how to use port events with Docker Compose services:

- **Port forwarding with compose**: Exposing ports from compose services
- **Port events**: Machine-readable PORT_EVENT output for tooling integration
- **Port attributes**: Configuring labels and auto-forward behavior for ports
- **Service ports**: Ports defined in docker-compose.yml and referenced in devcontainer.json

## Why This Matters

This configuration pattern is useful for:
- **IDE integration**: Editors and IDEs can parse PORT_EVENT messages to provide port forwarding UI
- **Automated workflows**: CI/CD or testing tools can discover and connect to service ports
- **Port management**: Declarative configuration of port labels, protocols, and auto-forward behavior
- **Multi-service coordination**: Managing ports across multiple compose services

## DevContainer Specification References

This example aligns with:
- **[Port Forwarding](https://containers.dev/implementors/json_reference/#port-attributes)**: forwardPorts, portsAttributes properties
- **[Docker Compose](https://containers.dev/implementors/json_reference/#compose-specific)**: dockerComposeFile, service properties
- **Port Events**: Machine-readable output for tooling (deacon-specific via --ports-events flag)

## Usage

### Start the service and capture port events

```sh
cd examples/compose/port-events
deacon up --ports-events
```

When `--ports-events` is specified, deacon will emit PORT_EVENT messages to stdout in this format:
```
PORT_EVENT: {"port":8080,"protocol":"tcp","hostPort":8080,"label":"Web Server","onAutoForward":"notify"}
```

### Capture and parse port events

Capture port events to a file for processing:
```sh
deacon up --ports-events 2>&1 | tee output.log
grep "PORT_EVENT:" output.log | sed 's/PORT_EVENT: //' | jq .
```

This will extract and pretty-print the JSON port event data.

### Verify the port is accessible

Once the service is running, test the exposed port:
```sh
curl http://localhost:8080
```

You should see: `Hello from port 8080`

### View port configuration

Check the resolved port configuration:
```sh
deacon read-configuration --config devcontainer.json | jq '.forwardPorts, .portsAttributes'
```

### Execute commands in the service

Run commands in the web service:
```sh
deacon exec sh -c 'echo "Server running on port $PORT"'
```

### Stop the service

```sh
deacon down
```

## Port Event Format

Port events are emitted as JSON with the following structure:
```json
{
  "port": 8080,
  "protocol": "tcp",
  "hostPort": 8080,
  "label": "Web Server",
  "onAutoForward": "notify"
}
```

Fields:
- **port**: The container port being forwarded
- **protocol**: Network protocol (typically "tcp")
- **hostPort**: The host port mapped to the container port
- **label**: Human-readable label from portsAttributes
- **onAutoForward**: Auto-forward behavior (notify, openBrowser, silent, etc.)

## Files

- **docker-compose.yml**: Defines a web service with port 8080 exposed
- **devcontainer.json**: DevContainer configuration with forwardPorts and portsAttributes
- **README.md**: This file

## Notes

- The `web` service uses Alpine Linux with `nc` (netcat) to simulate a simple HTTP server
- Port 8080 is both exposed in docker-compose.yml and configured in forwardPorts
- Port events are only emitted when using the `--ports-events` flag with `deacon up`
- The example uses minimal images (alpine) that should be available offline after first pull
- Port attributes like "label" and "onAutoForward" provide metadata for IDE/tooling integration
