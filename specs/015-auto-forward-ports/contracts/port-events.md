# Contract: Port events & human-readable mappings

Two output surfaces, both honoring the Output Streams Contract (Principle VI): the **`up` result JSON stays on stdout, unchanged**; everything below goes to **stderr** (human mappings) or the existing event channel.

## 1. Human-readable mappings (stderr)

Emitted by the forwarder as forwards become active. One line per forward. Format is human-facing (not parsed); examples:

```
Forwarding container 3000 -> http://127.0.0.1:3000 (web)
Forwarding container 3000 -> http://127.0.0.1:3001 (remapped; host 3000 in use)
Forwarding container 80   -> http://127.0.0.1:8080 (remapped; privileged port)
Unforwarded container 3000 (server stopped)
```

Rules:
- MUST always state the **actual** host port, especially on remap (FR-009, FR-010, SC-007) — no silent remaps.
- `silent` (`onAutoForward: silent`) ports are forwarded but produce **no** mapping line (FR-017).
- `ignore` ports produce no line and are not forwarded.

## 2. `PORT_EVENT:` machine channel (when `--ports-events` is set)

Reuses the existing `PortEvent` struct and `PORT_EVENT: <json>` emission (`crates/core/src/ports.rs`). The forwarder emits an event when a port is **forwarded** and when it is **unforwarded** (FR-020), extending today's create-time-only emission to the dynamic lifetime.

Existing `PortEvent` shape (camelCase JSON), unchanged:

```json
PORT_EVENT: {"port":3000,"protocol":"http","label":"web","onAutoForward":"notify","autoForwarded":true,"localPort":3001,"hostIp":"127.0.0.1"}
```

| Field | Forward event | Unforward event |
|-------|---------------|-----------------|
| `port` | container port | container port |
| `autoForwarded` | `true` | `false` |
| `localPort` | allocated host port | the freed host port (or null) |
| `hostIp` | `"127.0.0.1"` | `"127.0.0.1"` |
| `onAutoForward` | effective attribute | effective attribute |

Rules:
- `hostIp` is always `127.0.0.1` in v1 (loopback-only, FR-005).
- Event stream ordering reflects real-time forward/unforward transitions; a `silent` port still emits a `PORT_EVENT` (the event channel is machine-facing; `silent` suppresses only the human stderr line).
- Channel selection (stdout vs stderr) follows the existing `--ports-events` implementation and the JSON-mode output contract; no change to where `PORT_EVENT:` is written relative to today.

## 3. `up` result document (stdout) — UNCHANGED

The `up` JSON result on stdout MUST NOT gain forward-mapping fields in v1 (FR-010). Tools that need the live mappings use `--ports-events` (channel 2) or read the registry file. This keeps the stdout result contract stable and backward compatible.
