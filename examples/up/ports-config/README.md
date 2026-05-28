# Up: Ports — Full Spec Coverage

The DevContainer ports surface has four properties that work together,
and most existing examples cover only one or two. This example puts them
side by side:

- **`forwardPorts`** accepts a bare integer (`80`), a `"host:port"`
  string (`"127.0.0.1:9090:9090"`), or any mix in an array.
- **`portsAttributes`** maps port → attributes:
  - `label`, `description`
  - `protocol`: `"http"` | `"https"`
  - `onAutoForward`: `"notify"` | `"openBrowser"` | `"openPreview"` |
    `"silent"` | `"ignore"`
  - `requireLocalPort`: when `true`, host port must equal container port
  - `elevateIfNeeded`: prompt to elevate for privileged ports
- **`otherPortsAttributes`** sets defaults for ports NOT explicitly
  listed in `portsAttributes` (e.g., the `3000` here picks up
  `onAutoForward: "ignore"`).
- **`appPort`** declares the "main" port (number / string / array) that
  the consumer should open by default.

## Files

- `.devcontainer/devcontainer.json` — exercises all four properties on a
  vanilla `nginx:1.25-alpine` image. The image actually listens on 80 so
  we can verify the bind round-trip.

## Scenarios exercised by `exec.sh`

1. **`read-configuration` parses every property.** Parse the config to
   JSON and assert each field is present with the spec shape.
2. **`docker inspect` confirms published ports.** After `up`, the
   container has `HostConfig.PortBindings` for 80, 9090, 3000, and
   appPort 8080.
3. **`"host:port"` form honored.** Port 9090 is published bound to
   `127.0.0.1` only — `HostIp` should be `127.0.0.1`, not the default
   empty (all interfaces).
4. **HTTP round-trip on port 80.** Curl `http://localhost:<host80>`
   inside the container to confirm nginx is reachable on the published
   port.

## Manual usage

```sh
deacon read-configuration --workspace-folder . | jq '{
	appPort,
	forwardPorts,
	portsAttributes,
	otherPortsAttributes
}'

deacon up --workspace-folder . --remove-existing-container

# Inspect bound ports.
docker inspect <cid> --format '{{json .HostConfig.PortBindings}}' | jq

# Hit nginx on the host-side bound port.
HOST80=$(docker inspect <cid> --format '{{(index (index .NetworkSettings.Ports "80/tcp") 0).HostPort}}')
curl -sS "http://localhost:${HOST80}/" | head -1
```

## Spec references

- `forwardPorts`, `portsAttributes`, `otherPortsAttributes`, `appPort`:
  <https://containers.dev/implementors/json_reference/>
- IDE behavior contract (open/notify/silent/ignore):
  <https://github.com/devcontainers/spec/blob/main/docs/specs/devcontainer-reference.md>
