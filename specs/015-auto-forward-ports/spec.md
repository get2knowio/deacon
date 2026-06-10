# Feature Specification: Dynamic User-Space Port Forwarding (`up --auto-forward`)

**Feature Branch**: `015-auto-forward-ports`  
**Created**: 2026-06-08  
**Status**: Draft  
**Input**: GitHub issue #186 — "feat(up): add --auto-forward for dynamic user-space port forwarding (daemon)"

## Overview

Today, `deacon up` forwards ports **statically**: ports declared in `forwardPorts` / `appPort` / `--forward-port` are turned into `docker run -p` publish arguments at container-create time. This means a port must be known *before* the container exists, the host can only reach services bound to `0.0.0.0` inside the container, and a server started later (by a lifecycle hook, the entrypoint, or an interactive shell) is never reachable without recreating the container.

This feature adds a `deacon up --auto-forward` flag that turns on **dynamic, user-space port forwarding** for the lifetime of a running devcontainer, modeled on how VS Code Dev Containers forwards ports. When enabled, deacon watches the container for listening sockets as they appear and disappear, and exposes each one on the developer's machine at `127.0.0.1:<host-port>`, returning control to the shell immediately. It works across **multiple devcontainers at once**, allocating non-colliding host ports and reporting the actual local address for each.

## Clarifications

### Session 2026-06-08

- Q: When `--auto-forward` is set, what happens to declared ports (`forwardPorts` / `--forward-port` / `appPort`)? → A: They move to the dynamic forwarder (loopback relay) and are NOT also `-p` published, so the relay and Docker never contend for the same host port. The forwarded set = declared ports ∪ auto-detected ports.
- Q: What host interface are forwarded ports exposed on for v1? → A: Loopback only (`127.0.0.1`). Exposing on `0.0.0.0` / the LAN is explicitly out of scope for v1.
- Q: Should this introduce a `deacon forward` subcommand or an `exec --auto-forward` flag? → A: No. A single switch on `up` covers both headless and interactive workflows; standalone/attach affordances are possible future follow-ups, not this feature.
- Q: For a Compose project with multiple services, which service's ports does auto-forward watch in v1? → A: Match VS Code. Auto-**detection** is scoped to the primary/workspace service container only (other services have separate network namespaces the in-container scan cannot see). **Declared** ports are still forwarded and may target other services via the service-qualified `"service:port"` form (e.g. `"db:5432"`).
- Q: When `--auto-forward` is set, when is the host listener for a declared port (e.g. `forwardPorts: [8080]`) opened if nothing is listening inside yet? → A: Eagerly. The host listener for each declared port is opened immediately at `up` time and its natural host port reserved; the relay connects lazily once the container port starts listening. Connections that arrive before the server is ready are refused until it is. (Auto-detected, undeclared ports remain lazy — bound only once observed listening.)
- Q: If the forwarder fails to start/detach (e.g. detach failure, registry lock busy, no relay mechanism available), what is the `up` exit code? → A: Warn and succeed. The container is up and usable; forwarding is additive/best-effort, so `up` emits a clear, non-silent warning to stderr (and the forwarder log) and returns success (exit 0). It does NOT leave `up` exiting non-zero. The "no relay mechanism" case (FR-019) surfaces as this loud warning rather than failing the command.
- Q: How are privileged container ports (< 1024, e.g. 80/443) allocated on the host, given binding low host ports usually needs elevated privileges? → A: Always remap. For container ports < 1024, the natural same-number host port is skipped and an unprivileged host port (≥ 1024) is allocated and reported. The forwarder never attempts a privileged host bind, keeping the feature root-free.
- Q: Is the port-detection poll interval configurable in v1? → A: No. It is a fixed internal constant (~1s, satisfying the SC-002 ≤~2s target). The v1 CLI surface is just the boolean `--auto-forward`; a tuning flag/env var is a possible future addition.
- Q: Which transport protocols are forwarded in v1? → A: TCP only. Detection reads the container's TCP LISTEN tables and the relay handles TCP streams; UDP is explicitly out of scope for v1 (matches VS Code).

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Reach a loopback-bound server without republishing (Priority: P1)

A developer runs `deacon up --auto-forward`. A web server inside the container binds to `127.0.0.1:3000` (a very common default for dev servers). Without doing anything else, the developer opens `http://127.0.0.1:3000` (or the reported local port) in a browser on their machine and the page loads. Control returns to their shell immediately — no terminal is occupied and they did not have to background any process themselves.

**Why this priority**: This is the core value proposition and the single biggest gap versus static `-p` publishing, which cannot reach a `127.0.0.1`-bound service at all. If only this works, the feature already delivers meaningful value over the status quo.

**Independent Test**: Start a container whose server binds `127.0.0.1:PORT`, run `up --auto-forward`, and assert the host can reach `127.0.0.1:<reported-port>` — something static `-p` provably cannot do.

**Acceptance Scenarios**:

1. **Given** a devcontainer config and a server that binds `127.0.0.1:3000` at startup, **When** the developer runs `deacon up --auto-forward`, **Then** the command returns control to the shell and `127.0.0.1:<reported-port>` on the host serves the container's port 3000.
2. **Given** the same scenario, **When** the developer inspects the command's reported output, **Then** the actual local address for port 3000 is printed to stderr (and the `up` result document on stdout is unchanged).
3. **Given** `--auto-forward` is **not** passed, **When** the developer runs `deacon up`, **Then** behavior is identical to today (static `-p` publishing, no forwarder), preserving backward compatibility.

---

### User Story 2 - Auto-detect a port that starts after the container is up (Priority: P1)

A developer runs `deacon up --auto-forward` and then later starts a dev server — via an interactive `deacon exec` shell (`npm run dev`), a `postStart` hook, the container entrypoint, or a compose `CMD`. Without re-running `up` and without any new flag on `exec`, the newly opened port becomes reachable from the host shortly after it starts listening. When the server stops, the forward goes away.

**Why this priority**: Dynamic detection is the defining behavior that distinguishes this from static publishing and is the second half of the issue's core promise. It must work regardless of *who* opened the port, which is what allows `exec` to remain unchanged.

**Independent Test**: After `up --auto-forward`, open a new listening port from inside the container (e.g. via `exec`) and assert it becomes reachable on the host with no additional command and no `exec` flag; then stop it and assert the forward is withdrawn.

**Acceptance Scenarios**:

1. **Given** `up --auto-forward` is already running for a container, **When** a process inside the container begins listening on a new port, **Then** that port becomes reachable on `127.0.0.1:<host-port>` within a bounded detection interval.
2. **Given** a port that was being forwarded, **When** the process stops listening, **Then** the corresponding host forward is withdrawn and its host port released.
3. **Given** an interactive `deacon exec` session that starts a server, **When** the server begins listening, **Then** it is forwarded with **no changes to the `exec` command or flags**.

---

### User Story 3 - Multiple devcontainers forwarded concurrently without collisions (Priority: P1)

A developer (or a team member on a shared host / CI agent) runs `deacon up --auto-forward` for two different workspaces at the same time, and each container runs a server on the same container port (e.g. both on `3000`). Both servers are reachable from the host on **distinct, non-colliding** local ports, and the developer is clearly told which local port maps to which container. Tearing down one container frees its host ports and leaves the other working.

**Why this priority**: The issue calls out multi-container collision-free allocation as a KEY REQUIREMENT, not an optional extra. Two containers cannot both bind host `127.0.0.1:3000`; without host-global allocation the feature breaks the moment a second devcontainer appears, which is a normal real-world situation.

**Independent Test**: Run `up --auto-forward` on two workspaces both serving container port `3000`; assert two different host ports are allocated, both reachable, both reported; tear one down and assert its host port is released while the other still works.

**Acceptance Scenarios**:

1. **Given** two devcontainers each serving container port `3000`, **When** both are started with `--auto-forward`, **Then** each is assigned a different host port, both are reachable, and each container's actual local port is reported.
2. **Given** the natural host port for a container's service is already taken (by another forward or any process), **When** the forwarder allocates a host port, **Then** it remaps to the next available port and clearly reports that a remap occurred and the new port.
3. **Given** two forwarded containers, **When** one is torn down, **Then** that container's host ports are released and become available for reuse, and the other container's forwards are unaffected.

---

### User Story 4 - Clean teardown with no orphans (Priority: P2)

When the developer runs `deacon down` (or `deacon up --remove-existing-container`), the forwarding process for that container is stopped, its host ports are released, and no leftover process or stale bookkeeping remains. If the container disappears out-of-band (e.g. `docker rm -f`), the forwarder notices and shuts itself down and cleans up on its own.

**Why this priority**: Correct lifecycle is required for the feature to be safe to use repeatedly, but it builds on the forwarding mechanism from P1, so it is sequenced after the core capability. Leaks would accumulate orphan processes and bound host ports across a work session.

**Independent Test**: After `up --auto-forward`, run `down` and assert the forwarder is gone, its marker removed, and its host ports released; separately, remove the container out-of-band and assert the forwarder self-exits and cleans up.

**Acceptance Scenarios**:

1. **Given** an active forwarder for a container, **When** the developer runs `deacon down`, **Then** the forwarder stops, its host ports are released, and no orphan process or leftover bookkeeping remains.
2. **Given** an active forwarder, **When** the developer runs `deacon up --remove-existing-container`, **Then** the old forwarder is stopped before the new container and forwarder are created.
3. **Given** an active forwarder, **When** the container is removed out-of-band, **Then** the forwarder self-exits and releases its host ports without manual intervention.
4. **Given** a forwarder is already active for a container, **When** `deacon up --auto-forward` is run again for the **same** container, **Then** the existing forwarder is reused rather than a duplicate being started.

---

### User Story 5 - Respect declared port intent and attributes (Priority: P3)

A developer's config declares ports and per-port attributes (`forwardPorts`, `appPort`, `portsAttributes`, `otherPortsAttributes`). The forwarder honors these: declared ports are forwarded, ports marked to be ignored are not forwarded, and per-port notification preferences (e.g. silent vs. notify) are respected. Undeclared, auto-detected ports get the default behavior defined by `otherPortsAttributes`.

**Why this priority**: Honoring declared intent aligns the feature with the containers.dev spec vocabulary and avoids surprising the user, but the feature is already valuable for the common "just forward what's listening" case without it, so it is sequenced last.

**Independent Test**: Configure a port with `onAutoForward: ignore` and another with `onAutoForward: silent`; assert the ignored port is not forwarded and the silent port is forwarded without a notification, while a notify port produces a notification.

**Acceptance Scenarios**:

1. **Given** a port with `onAutoForward: ignore`, **When** that port starts listening, **Then** it is not forwarded.
2. **Given** a port with `onAutoForward: silent`, **When** it is forwarded, **Then** no user-facing notification is emitted for it (but it is still reachable).
3. **Given** an auto-detected port with no explicit attribute, **When** it is forwarded, **Then** the behavior defined by `otherPortsAttributes` applies.
4. **Given** declared ports and `--auto-forward`, **When** the container starts, **Then** those declared ports are forwarded by the dynamic forwarder and are NOT also statically `-p` published.

### Edge Cases

- **Natural host port already in use** → remap to the next free host port and clearly report the remap and the actual port (never silently fail or silently bind a different port).
- **No relay mechanism available in the container** → surface a clear, actionable error (stderr + forwarder log) rather than silently doing nothing (consistent with the project's no-silent-fallback principle); per FR-025 this is a loud warning and `up` still succeeds (exit 0) — it does not fail the command.
- **Same port appears on both IPv4 and IPv6 LISTEN tables** → treat as one logical port (deduplicate), not two forwards.
- **Container port bound to `127.0.0.1` vs `0.0.0.0` vs `::` / `::1`** → all are eligible for forwarding (the loopback reach is the headline capability).
- **Rapid open/close churn of a port** → forwards are added/removed to track current state without leaking host ports or processes.
- **Concurrent `up --auto-forward` racing for the same natural host port** → host-port allocation is serialized so two starts cannot bind the same host port.
- **Stale bookkeeping from a crashed prior run** (dead process, container already gone) → pruned automatically on the next start/allocation so it does not block new forwards.
- **Out-of-band container removal** (`docker rm -f`) → forwarder self-exits and cleans up.
- **`--auto-forward` combined with `--ports-events`** → forward/unforward transitions are surfaced through the existing events channel as ports come and go.
- **Compose projects with multiple services** → auto-**detection** is scoped to the primary/workspace service container only (matching VS Code; other services have separate network namespaces the in-container socket scan cannot observe). Ports on other services are forwarded only when **declared** using the service-qualified `"service:port"` form.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: The `up` subcommand MUST accept a boolean `--auto-forward` flag that enables dynamic user-space port forwarding for the container being started.
- **FR-002**: When `--auto-forward` is enabled, the system MUST return control to the invoking shell after starting the container and forwarding, without occupying the terminal or requiring the user to background any process.
- **FR-003**: The system MUST detect **TCP** ports that are listening inside the container, including ports bound to `127.0.0.1` / `::1` (loopback), `0.0.0.0`, and `::`, and MUST treat the same logical port appearing on both IPv4 and IPv6 as a single forward. UDP is out of scope for v1.
- **FR-004**: The system MUST detect ports that start listening **after** the container is up (from entrypoint, lifecycle hooks, compose command, or an interactive `exec` session) within a bounded detection interval, and MUST withdraw a forward when its port stops listening. The detection interval is a fixed internal constant (~1s) in v1 and is NOT user-configurable; the only v1 CLI surface for this feature is the boolean `--auto-forward` flag.
- **FR-005**: For each forwarded port, the system MUST expose it on the host bound to `127.0.0.1` only, and MUST NOT bind forwarded ports on `0.0.0.0` or any non-loopback interface in v1.
- **FR-006**: When `--auto-forward` is enabled, the system MUST forward declared ports (`forwardPorts`, `appPort`, `--forward-port`) via the dynamic forwarder and MUST NOT additionally publish them via static `-p`; the forwarded set is the union of declared ports and auto-detected ports.
- **FR-007**: When `--auto-forward` is absent, the system MUST behave exactly as today (declared ports published via static `-p`, no forwarder), preserving backward compatibility.
- **FR-008**: The system MUST support multiple concurrent `up --auto-forward` invocations on the same host and MUST allocate host ports that do not collide across all active forwarders.
- **FR-009**: When the natural host port (matching the container port number) is unavailable, the system MUST remap to the next available host port and MUST clearly report that a remap occurred and the actual host port chosen.
- **FR-009a**: For container ports below 1024 (privileged), the system MUST NOT attempt to bind the same-numbered (privileged) host port; it MUST allocate an unprivileged host port (≥ 1024) and report the mapping. The forwarder MUST NOT require elevated host privileges to bind any forwarded port.
- **FR-010**: The system MUST report, per forwarded port, the actual local address (`127.0.0.1:<host-port>`) the user can reach. Human-readable mappings MUST go to stderr; the `up` result document on stdout MUST be unaffected.
- **FR-011**: The system MUST maintain host-global bookkeeping of active forwards (which host port maps to which container/container-port) such that concurrent forwarders cooperate; updates to this bookkeeping MUST be performed atomically and be safe under concurrent access.
- **FR-012**: The system MUST ensure at most one forwarder per container: if a live forwarder already exists for a container, a subsequent `up --auto-forward` for the same container MUST reuse it rather than starting a duplicate.
- **FR-013**: On `deacon down`, the system MUST stop the forwarder(s) for the resolved container(s), release their host ports, and remove their bookkeeping, leaving no orphan process or stale entries.
- **FR-014**: On `up --remove-existing-container`, the system MUST stop the existing forwarder before creating the replacement container and forwarder.
- **FR-015**: The forwarder MUST detect when its container has disappeared out-of-band and MUST self-exit and release its host ports without manual intervention.
- **FR-016**: The system MUST prune stale bookkeeping entries (forwarder process no longer alive, or container no longer exists) on forwarder start and on host-port allocation, so leftovers from prior runs do not block new forwards.
- **FR-017**: The system MUST honor `portsAttributes[*].onAutoForward` for declared ports — at minimum `ignore` (do not forward), `silent` (forward without user-facing notification), and `notify` (forward with notification) — and MUST apply `otherPortsAttributes` as the default for auto-detected, undeclared ports.
- **FR-018**: The system MUST require no changes to the `exec` subcommand for forwarding ports opened inside an `exec` session; detection MUST be based on listening sockets and indifferent to which process opened the port.
- **FR-019**: When a relay mechanism required to forward a port is unavailable, the system MUST surface a clear, actionable error (to stderr and the forwarder log) rather than silently not forwarding. Per FR-025, this does not make `up` exit non-zero — forwarding is best-effort — but the error MUST NOT be swallowed.
- **FR-020**: When `--ports-events` is also enabled, the system MUST emit forward and unforward transitions through the existing port-events channel as ports come and go.
- **FR-021**: The forwarder MUST run with no terminal attached and MUST write diagnostics to a per-container log location so its activity is observable after `up` returns.
- **FR-022**: The new host-facing surface (a persistent host process and bound loopback host ports) MUST be documented in the project security documentation, and the loopback-only binding MUST be stated as the primary mitigation.
- **FR-023**: For Compose projects, port auto-detection MUST be scoped to the primary/workspace service container; ports on other services MUST be forwarded only when explicitly declared using the service-qualified `"service:port"` form, matching VS Code behavior.
- **FR-024**: For **declared** ports (`forwardPorts` / `appPort` / `--forward-port`), the system MUST open the host listener and reserve the natural host port **eagerly** at `up` time, before any container-side server is listening, and MUST relay connections lazily once the container port begins listening (connections arriving earlier are refused until ready). **Auto-detected, undeclared** ports MUST remain lazy — their host listener is opened only once the port is observed LISTENING and torn down when it stops.
- **FR-025**: When `--auto-forward` is requested and the forwarder cannot be started or detached (detach failure, registry lock contention, no relay mechanism, etc.), the system MUST emit a clear warning (stderr + forwarder log) and the `up` command MUST still return success (exit 0) with the container running. Forwarding is additive and best-effort; failure to forward MUST NOT fail `up` or leave the container in a partially-created state.

### Key Entities *(include if feature involves data)*

- **Forwarder (daemon)**: A host-side, detached process bound to a single running container for the lifetime of its forwards. Responsible for detecting listening ports, allocating host ports, relaying bytes, reconciling on change, and self-exiting when its container is gone. Identified per-container.
- **Forward mapping**: The association of a container port to an allocated host port for a specific container/workspace, including a human-facing label and whether it was remapped from its natural port. This is what gets reported to the user and what gets released on teardown.
- **Host-global port registry**: Shared bookkeeping across all active forwarders on the host that records every active host-port allocation (host port, container, container port, workspace, owning process, label) so concurrent forwarders avoid collisions and so teardown/pruning can release ports.
- **Per-container marker**: A single-owner record indicating a live forwarder exists for a given container, used to adopt/reuse rather than duplicate, and read by `down` / replace to reap the forwarder.
- **Port attributes**: Per-port and default forwarding preferences sourced from config (`portsAttributes`, `otherPortsAttributes`) — notably the auto-forward action (ignore / silent / notify) and label — that govern whether and how a detected port is forwarded and announced.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: With `up --auto-forward`, a developer can reach a container service bound to `127.0.0.1` from their host browser — a scenario that is impossible with static `-p` publishing today (0% → 100% reachable).
- **SC-002**: A port that starts listening after `up` returns becomes reachable from the host within the detection interval (target: ≤ ~2 seconds under default settings) with no additional command from the user.
- **SC-003**: Running `up --auto-forward` returns control to the shell while forwarding remains active — the user never has to keep a terminal open or manually background a process to keep forwards alive.
- **SC-004**: Two or more devcontainers serving the same container port can be forwarded simultaneously with a 0% host-port collision rate; each container's actual local port is reported to the user.
- **SC-005**: After `down` (or `up --remove-existing-container`, or out-of-band container removal), there are zero orphan forwarder processes and zero leaked host-port registrations attributable to the forwarder.
- **SC-006**: When `--auto-forward` is absent, port-forwarding behavior is unchanged from today (no regressions in existing static-publishing scenarios).
- **SC-007**: When a natural host port is taken, the user is always told the actual local port in use (0% silent remaps).

## Assumptions

- **Loopback-only for v1**: Forwarded ports are bound to `127.0.0.1` on the host. Exposing forwards on `0.0.0.0` / the LAN is deliberately out of scope and tracked as possible future work.
- **Declared ports move to the forwarder when `--auto-forward` is set**: This resolves the open question in the issue in favor of "the daemon owns declared ports, no parallel `-p`," to avoid the daemon and Docker contending for the same host port. (Per Clarifications.)
- **`appPort` is treated like a declared port** and joins the forwarded set when `--auto-forward` is set.
- **Allocation policy is VS Code-like**: prefer the same number (container 3000 → host 3000) when free; otherwise pick the next free host port and report the remap. Privileged container ports (< 1024) are always remapped to an unprivileged host port (≥ 1024) — the same-number attempt is skipped to keep the forwarder root-free (per Clarifications / FR-009a).
- **Detection is poll-based for v1** with a bounded interval; event-driven detection (netlink/inotify) is a possible future optimization, not a requirement.
- **Unix is the primary target for the detach/lifecycle path in v1**; the Windows detach/signaling path is acknowledged and tracked as a follow-up, not a v1 acceptance gate.
- **Per-connection relay is acceptable for v1**; a persistent in-container multiplexing agent is a noted future optimization, not required.
- **No new host-shell-from-workspace trust surface**: forwarding execs *into the container* (standard consumer behavior, like `exec`), so the workspace-trust gate is not required for the exec itself; the new surface is the host process and bound loopback ports, which are documented rather than gated. Whether any additional opt-in beyond the explicit `--auto-forward` flag is warranted is deferred to security review.
- **Host-global registry lives under the user-data folder** (the same location the trust store uses), not a per-workspace container-data folder, because host ports are a host-wide shared resource.
- **Compose scope matches VS Code** (per Clarifications): auto-detection watches only the primary/workspace service container's listening sockets; cross-service forwarding requires explicit service-qualified declared ports. Watching every service's network namespace is a possible future enhancement, not v1.

## Out of Scope (v1)

- Replacing or changing static `-p` publishing when `--auto-forward` is **not** passed.
- Exposing forwarded ports on `0.0.0.0` / the LAN.
- A standalone `deacon forward` subcommand or an `exec --auto-forward` flag (a post-hoc attach affordance is a possible future follow-up).
- UDP (and any non-TCP transport) forwarding — TCP listeners only in v1.
- Remote-tunnel / cloud-relay forwarding (local loopback relay only).
- Browser/preview auto-open behavior beyond honoring notification preferences (`openBrowser` / `openPreview` are optional, not required for v1).
- Any feature-authoring or non-consumer surface.
