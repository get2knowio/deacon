# Phase 1 Data Model: Dynamic User-Space Port Forwarding

Entities below are the runtime/persisted structures for the forwarder. Types are indicative Rust shapes for `crates/core/src/port_forward/`; field names in persisted JSON are the binding contract (see `contracts/`).

---

## 1. `DetectedPort` (in-memory, from `detect.rs`)

One LISTEN socket observed inside the container.

| Field | Type | Notes |
|-------|------|-------|
| `port` | `u16` | Container-side listening port. |
| `bind_addr` | `BindScope` enum | `Loopback` (`127.0.0.1`/`::1`) or `AnyInterface` (`0.0.0.0`/`::`). Informational; both are forwarded. |
| `family` | `IpFamily` enum | `V4` / `V6`. Used only for dedup; the forward is per logical `port`. |

**Validation / rules**:
- Only rows with state `st == 0A` (TCP_LISTEN) are emitted.
- The same `port` appearing on V4 and V6 collapses to a single logical port (FR-003).
- TCP only — UDP tables are not read (FR-003).

**Derivation**: pure function `parse_proc_net_tcp(&str) -> Vec<DetectedPort>` over the concatenated `/proc/net/tcp` + `/proc/net/tcp6` text. No IO; fully unit-testable with fixtures.

---

## 2. `ForwardSpec` (in-memory, the daemon's intent for one port)

The reconciled decision of "this container port should be forwarded."

| Field | Type | Notes |
|-------|------|-------|
| `container_port` | `u16` | Source port inside the container. |
| `origin` | `ForwardOrigin` enum | `Declared` (from `forwardPorts`/`appPort`/`--forward-port`) or `AutoDetected`. |
| `service` | `Option<String>` | For compose `"service:port"` declared ports; `None` = primary service. |
| `attributes` | `ResolvedPortAttributes` | Effective `portsAttributes[port]` or `otherPortsAttributes` default. |
| `eager` | `bool` | `true` for `Declared` (bind host listener at startup), `false` for `AutoDetected` (bind on first observation). FR-024. |

**Rules**:
- `Declared` ⇒ `eager = true`, host port reserved at `up` time (FR-024); suppressed from static `-p` (FR-006).
- `AutoDetected` ⇒ `eager = false`; created when observed LISTEN, removed when it stops (FR-004).
- If `attributes.on_auto_forward == Ignore`, no `ForwardSpec`/listener is created (FR-017).

---

## 3. `ResolvedPortAttributes`

Effective per-port forwarding preferences (subset of the existing `PortAttributes`, `config.rs:192`).

| Field | Type | Notes |
|-------|------|-------|
| `label` | `Option<String>` | Human label for reporting. |
| `on_auto_forward` | `OnAutoForward` | `Ignore` / `Silent` / `Notify` (v1 honored); `OpenBrowser`/`OpenPreview` accepted but treated as `Notify` for v1. |

**Derivation**: declared port → `config.ports_attributes[port]`; auto-detected port with no explicit entry → `config.other_ports_attributes` default, else implicit `Notify`.

---

## 4. `ActiveForward` (in-memory live state, owned by the daemon)

A `ForwardSpec` that currently has a host listener bound.

| Field | Type | Notes |
|-------|------|-------|
| `spec` | `ForwardSpec` | The intent. |
| `host_port` | `u16` | Allocated loopback host port (≥1024). |
| `remapped` | `bool` | `true` if `host_port != container_port` (drives the "remapped" report, FR-009). |
| `listener` | `TcpListener` handle | Bound to `127.0.0.1:host_port`. |
| `connections` | task set | Per-connection relay tasks. |

**State transitions**:
```
            declared (eager)                       observed LISTEN (lazy)
   (none) ───────────────► Reserved ──relay dials──► Active
        ▲                     │ container port observed │
        │                     ▼                          ▼
        └────── released ◄── Withdrawn ◄── port stops / container gone / down
```
- `Reserved`: host listener open, container side not yet listening → host connections refused until ready (FR-024).
- `Active`: relaying.
- `Withdrawn`: listener closed, registry entry released (FR-004, FR-013, FR-015).

---

## 5. `RegistryEntry` (persisted — `forwarded_ports.json`)

One row in the host-global allocation registry. **This JSON shape is a binding contract** (`contracts/registry.schema.json`).

| Field | JSON key | Type | Notes |
|-------|----------|------|-------|
| host port | `host_port` | number (u16) | Allocated loopback host port. Unique across the file. |
| container id | `container_id` | string | Owning container (full id). |
| container port | `container_port` | number (u16) | Source port inside the container. |
| workspace | `workspace` | string | Canonical workspace path (for reporting / human disambiguation). |
| pid | `pid` | number | Owning forwarder process id (for stale-reaping). |
| label | `label` | string \| null | Effective port label, if any. |

**Rules / invariants**:
- `host_port` is unique across the entire file (the collision-avoidance invariant, FR-008/SC-004).
- Writes go through an `fs2` advisory lock + temp-file `rename` (Decision 5); no partial/truncated writes.
- Entries are pruned when `pid` is not alive or `container_id` no longer exists (FR-016).
- All of a container's entries are removed on reap/self-exit (FR-013, FR-015).

---

## 6. `DaemonMarker` (persisted — `forward_daemon_<container_id>.pid`)

Single-owner record proving a live forwarder exists for a container. **Binding contract** (`contracts/marker.schema.json`).

| Field | JSON key | Type | Notes |
|-------|----------|------|-------|
| pid | `pid` | number | Forwarder process id. |
| container id | `container_id` | string | The container this forwarder owns. |
| workspace | `workspace` | string | Canonical workspace path. |
| started at | `started_at` | string (RFC3339) | For diagnostics. |
| log path | `log_path` | string | Absolute path to the per-container log. |

**Rules**:
- Located at `{user_data_folder}/forward_daemon_<container_id>.pid` (mirrors env-probe key pattern).
- `up --auto-forward` reads it: live pid ⇒ adopt/reuse (no duplicate, FR-012); dead/missing ⇒ spawn fresh.
- Removed on `down` / replace reap and on daemon self-exit.
- Cleared on `up --remove-existing-container` for the replaced container (FR-014).

---

## Entity relationships

```
ContainerIdentity (existing) ──derives──► container_id, labels, workspace_hash
        │                                         │
        │ one per `up --auto-forward`             │ keys
        ▼                                         ▼
   DaemonMarker (1 per container) ───owns───► forwarder process (pid)
        │                                         │ maintains
        │                                         ▼
        │                                  ActiveForward[] (N per container)
        │                                         │ each allocates
        ▼                                         ▼
   forwarded_ports.json ◄──────────────── RegistryEntry[] (host-global, all containers)
   (1 per host, flock-guarded)             host_port unique across the whole file
```

- A single `forwarded_ports.json` is shared by **all** forwarders on the host (host-global). Every `RegistryEntry.host_port` is unique file-wide.
- Each container has exactly one `DaemonMarker` and one forwarder process, which owns N `ActiveForward`s and contributes N `RegistryEntry`s.
- `ForwardSpec.attributes` is resolved from the existing `DevContainerConfig.ports_attributes` / `other_ports_attributes` — reused, not redefined.
