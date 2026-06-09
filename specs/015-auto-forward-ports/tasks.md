---
description: "Task list for Dynamic User-Space Port Forwarding (up --auto-forward)"
---

# Tasks: Dynamic User-Space Port Forwarding (`up --auto-forward`)

**Input**: Design documents from `/specs/015-auto-forward-ports/`
**Prerequisites**: plan.md, spec.md, research.md, data-model.md, contracts/

**Tests**: INCLUDED — required by Constitution Principle VII (Testing Completeness) and the spec's per-story Independent Tests + testing plan. Unit tests are hermetic; integration tests are Docker-gated.

**Organization**: Tasks are grouped by user story (US1–US5) to enable independent implementation and testing. P1 stories (US1–US3) are all required for a usable feature; US1 is the demonstrable MVP.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependencies on incomplete tasks)
- **[Story]**: US1–US5 (user-story phases only)
- All paths are repo-relative from `/workspaces/deacon/`

## Path Conventions

- Core logic: `crates/core/src/port_forward/`
- CLI wiring: `crates/deacon/src/commands/up/`, `crates/deacon/src/commands/`
- Integration tests: `crates/deacon/tests/`
- Examples: `examples/auto-forward/`

---

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: Dependencies and module scaffolding.

- [X] T001 Add `nix` (Unix-only, `features = ["process","signal"]`) and `fs2` to `Cargo.toml` `[workspace.dependencies]`, then reference both in `crates/core/Cargo.toml` (`nix` under `[target.'cfg(unix)'.dependencies]`, `fs2` under `[dependencies]`); per research.md Decision 1 & 5.
- [X] T002 Create the module scaffold `crates/core/src/port_forward/{mod,detect,registry,relay,daemon}.rs` (empty stubs with module docs) and add `pub mod port_forward;` to `crates/core/src/lib.rs`.

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Shared types, paths, CLI surface, the detached-daemon entrypoint, and cross-platform gating that ALL user stories build on.

**⚠️ CRITICAL**: No user story work can begin until this phase is complete.

- [X] T003 Define core data types + `PortForwardError` (`thiserror`) in `crates/core/src/port_forward/mod.rs` per data-model.md: `DetectedPort`/`BindScope`/`IpFamily`, `ForwardSpec`/`ForwardOrigin` (incl. `service: Option<String>`), `ResolvedPortAttributes`, `RegistryEntry` (serde, matches `contracts/registry.schema.json`), `DaemonMarker` (serde, matches `contracts/marker.schema.json`).
- [X] T004 Add host-path helpers in `crates/core/src/port_forward/mod.rs` — `registry_path()`, `marker_path(container_id)`, `log_path(container_id)` — reusing the user-data-folder resolution from `crates/core/src/trust.rs` (`--user-data-folder` else `~/.deacon`).
- [X] T005 Add `--auto-forward` boolean to `Commands::Up` in `crates/deacon/src/cli.rs` and `auto_forward: bool` to `UpArgs` in `crates/deacon/src/commands/up/args.rs`; thread the mapping (default `false`). Per `contracts/cli.md`.
- [X] T006 Add the hidden `__forward-daemon` subcommand (`#[command(hide = true)]`) to `crates/deacon/src/cli.rs` with args `--container-id`, `--workspace`, `--user-data-folder`, `--declared-port` (repeatable), `--config`, per `contracts/cli.md`.
- [X] T007 Create the daemon entrypoint `crates/deacon/src/commands/forward_daemon.rs`: parse the hidden-subcommand args, call `nix::unistd::setsid()`, reopen stdio onto the per-container log file, init a `tracing` file subscriber, then call the core daemon `run()` stub; register the module in `crates/deacon/src/commands/mod.rs` and dispatch it from the main command match.
- [X] T008 Gate the Unix-only forwarder behind `cfg(unix)` (the `port_forward` module, `forward_daemon.rs`, and the spawn path), and make `--auto-forward` on a non-Unix build return a clear "not supported on this platform" error per Principle IV (no silent fallback). Ensure the workspace still **compiles** on Windows. Windows support itself is deferred (see Deferred Work).

**Checkpoint**: Flag parses, daemon process can be spawned and detaches on Unix, builds cleanly on Windows with a clear unsupported-platform error, but does no forwarding yet.

---

## Phase 3: User Story 1 - Reach a loopback-bound server without republishing (Priority: P1) 🎯 MVP

**Goal**: `up --auto-forward` starts a detached forwarder that makes a `127.0.0.1`-bound container server reachable on the host, and returns control to the shell.

**Independent Test**: Start a container whose server binds `127.0.0.1:PORT`, run `up --auto-forward`, assert `127.0.0.1:<reported-port>` on the host serves it — something static `-p` cannot do.

### Tests for User Story 1

- [X] T009 [P] [US1] Unit tests for the `/proc/net/tcp{,6}` parser in `crates/core/src/port_forward/detect.rs` — fixtures for IPv4 little-endian decode, IPv6, `127.0.0.1` vs `0.0.0.0` binds, v4/v6 dedup, and non-LISTEN (`st != 0A`) rows ignored.
- [X] T010 [P] [US1] Docker integration test in `crates/deacon/tests/integration_auto_forward.rs` — `up --auto-forward` against a container serving `127.0.0.1:PORT`; assert host `127.0.0.1:<host_port>` is reachable and `up` returned (no occupied terminal). Use a `TempDir` workspace (per memory: in-repo `up` chowns the workspace).
- [X] T011 [P] [US1] Docker integration regression test in `crates/deacon/tests/integration_auto_forward.rs` — `up` **without** `--auto-forward` still publishes declared ports via static `-p` and creates **no** forwarder process, marker, or registry entry (guards FR-007 / SC-006, backward compatibility).

### Implementation for User Story 1

- [X] T012 [US1] Implement `parse_proc_net_tcp(&str) -> Vec<DetectedPort>` in `crates/core/src/port_forward/detect.rs`: parse `st == 0A` rows, decode little-endian `HEXIP:HEXPORT`, collect loopback/any binds, dedup v4/v6, TCP only (FR-003).
- [X] T013 [US1] Implement the detection probe in `crates/core/src/port_forward/daemon.rs`: run `Docker::exec(id, ["cat","/proc/net/tcp","/proc/net/tcp6"], ExecConfig{silent:true,..})` and feed stdout to `parse_proc_net_tcp` (FR-018; reuses the existing exec trait).
- [X] T014 [US1] Implement `crates/core/src/port_forward/relay.rs`: bind `127.0.0.1:host_port` `TcpListener`; per accepted connection spawn `docker exec -i <id> <relay>` dialing a **configurable** dial host (default `127.0.0.1`):`container_port` and pump bytes bidirectionally; relay-program selection (embedded static binary primary → `socat`/`nc`/`/dev/tcp` fallback → fail-fast clear error if none, FR-019). Per research.md Decision 3. (The configurable dial host is generalized for `service:port` in US5/T046.)
- [X] T015 [US1] Implement minimal single-port allocation in `crates/core/src/port_forward/registry.rs`: prefer same number with a bind probe; remap privileged (<1024) container ports to a free host port ≥1024 (FR-009a). (Host-global/concurrency hardening deferred to US3.)
- [X] T016 [US1] Implement the supervisor `run()` in `crates/core/src/port_forward/daemon.rs`: eager-bind declared ports (Reserved→Active on observation), poll-detect (~1 s) and forward listening ports, write the `DaemonMarker` on start, append `RegistryEntry` per forward (FR-002, FR-024, FR-021).
- [X] T017 [US1] Implement `crates/deacon/src/commands/up/forward.rs`: spawn + detach the forwarder by re-exec'ing `std::env::current_exe()` with `__forward-daemon`, passing container id / workspace / user-data-folder / declared ports; do not await; return to the shell (FR-002). Per research.md Decision 1.
- [X] T018 [US1] Hook forwarder spawn into the up flow in `crates/deacon/src/commands/up/container.rs` after lifecycle completes (~line 673), gated on `args.auto_forward`.
- [X] T019 [US1] Implement best-effort failure handling around the spawn path in `crates/deacon/src/commands/up/forward.rs` / `container.rs`: if the forwarder cannot be spawned/detached (or no relay strategy exists), emit a clear warning to stderr and let `up` return success (exit 0) with the container running — never swallow the error, never leave a partial container (FR-025, FR-019).
- [X] T020 [US1] Route declared ports (`forwardPorts`/`appPort`/`--forward-port`) to the forwarder and **suppress** their static `-p` publish args when `auto_forward` is set, in `crates/deacon/src/commands/up/ports.rs` (FR-006).
- [X] T021 [US1] Print loopback mappings to **stderr** with explicit remap reporting (`Forwarding container P -> http://127.0.0.1:H (...)`) in `forward.rs`/`daemon.rs`; keep the `up` stdout result document unchanged (FR-010, SC-007). Per `contracts/port-events.md`.
- [X] T022 [US1] Register the `integration_auto_forward` test binary in `.config/nextest.toml` as `docker-shared` (single-container) — add to the `[profile.default]` override filter, the `[profile.dev-fast]` `default-filter` exclusion, and the `[profile.dev-fast]` override filter (3 spots per CLAUDE.md).

**Checkpoint**: US1 fully functional — loopback reach works, shell is free, mapping reported, backward-compat guarded. **This is the demoable MVP.**

---

## Phase 4: User Story 2 - Auto-detect a port that starts after the container is up (Priority: P1)

**Goal**: Ports that start listening after `up` (entrypoint/`postStart`/compose/`exec`) are forwarded within ~1–2 s with no `exec` changes; forwards are withdrawn when the port stops.

**Independent Test**: After `up --auto-forward`, open a new listening port from inside the container (via `exec`); assert it becomes reachable with no extra command/flag; stop it and assert the forward is withdrawn.

### Tests for User Story 2

- [X] T023 [P] [US2] Docker integration test in `crates/deacon/tests/integration_auto_forward.rs` — start a server AFTER `up --auto-forward` via `deacon exec` (no exec flag); assert it becomes reachable within the detection window; stop it and assert the host listener is withdrawn and the port released.

### Implementation for User Story 2

- [X] T024 [US2] Implement the reconcile loop in `crates/core/src/port_forward/daemon.rs`: diff detected vs. active each tick; **lazily** bind newly-observed undeclared ports; withdraw + release forwards whose container port stopped listening; handle rapid open/close churn without leaking host ports or relay tasks (FR-004).
- [X] T025 [US2] Ensure auto-detected (undeclared) ports use lazy binding semantics (bound only on observation, not reserved at start) in `crates/core/src/port_forward/daemon.rs` (FR-024).
- [X] T026 [US2] Emit `PORT_EVENT:` forward/unforward transitions when `--ports-events` is set, reusing `PortEvent` in `crates/core/src/ports.rs` (extend the create-time-only emission to dynamic lifetime) per `contracts/port-events.md` (FR-020).
- [X] T027 [US2] Add an integration assertion (in `integration_auto_forward.rs`) proving a port opened inside an `exec` session is forwarded with **zero** changes to `exec` (documents FR-018 transparency).

**Checkpoint**: Dynamic detection + withdrawal works on top of US1.

---

## Phase 5: User Story 3 - Multiple devcontainers forwarded concurrently without collisions (Priority: P1)

**Goal**: Concurrent `up --auto-forward` invocations allocate collision-free host ports via a host-global registry; remaps are reported; tearing one down releases its ports.

**Independent Test**: Two workspaces both serving container `3000`; assert two distinct host ports, both reachable, both in the registry; `down` one and assert its port is released while the other still works.

### Tests for User Story 3

- [X] T028 [P] [US3] Unit tests for `crates/core/src/port_forward/registry.rs` — allocation prefers same number; collision triggers next-free remap; release removes entries; stale entries (dead pid / missing container) are pruned; allocate-and-bind is serialized (simulate concurrent allocation).
- [X] T029 [P] [US3] Docker integration test in `crates/deacon/tests/integration_auto_forward.rs` — two `up --auto-forward` on two `TempDir` workspaces both serving container `3000`; assert distinct host ports, both reachable, both registry entries present; `down` one, assert its host port released and the other still works.

### Implementation for User Story 3

- [X] T030 [US3] Implement the host-global registry in `crates/core/src/port_forward/registry.rs`: load/save `{user_data_folder}/forwarded_ports.json` with atomic temp-file + `fs::rename` (reuse `cache/disk.rs::save_index` pattern), wrapping the allocate-and-bind critical section in an `fs2` advisory `flock` (FR-008, FR-011). Per research.md Decision 5.
- [X] T031 [US3] Implement collision-free allocation across ALL registry entries (host_port unique file-wide) in `crates/core/src/port_forward/registry.rs`, with remap + report when the natural port is taken (FR-008, FR-009, SC-004).
- [X] T032 [US3] Implement stale-entry pruning (dead `pid` or missing container) on daemon start and on each allocation, and release-on-shutdown that removes this container's entries (FR-016, FR-013).
- [X] T033 [US3] If multi-container tests race, reclassify the `integration_auto_forward` binary to `docker-exclusive` in `.config/nextest.toml` (update all 3 profile spots), or split multi-container cases into a `docker-exclusive` filterset while keeping single-container cases `docker-shared`.

**Checkpoint**: All three P1 stories work — the feature is fully usable for real multi-container workflows.

---

## Phase 6: User Story 4 - Clean teardown with no orphans (Priority: P2)

**Goal**: `down` / `up --remove-existing-container` reap the forwarder and release its ports; the daemon self-exits if its container vanishes; a second `up` for the same container reuses the existing forwarder.

**Independent Test**: After `up --auto-forward`, `down` and assert forwarder gone + marker removed + ports released; separately `docker rm -f` the container and assert the forwarder self-exits and cleans up.

### Tests for User Story 4

- [X] T034 [P] [US4] Unit test for marker adopt-or-reuse logic (live pid ⇒ reuse, dead/missing ⇒ spawn) in `crates/core/src/port_forward/registry.rs` or `crates/deacon/src/commands/up/forward.rs`.
- [X] T035 [P] [US4] Docker integration test in `crates/deacon/tests/integration_auto_forward.rs` — (a) `down` reaps the daemon (pid gone, marker removed, registry entries released); (b) `up --remove-existing-container` kills the old daemon before replacing; (c) `docker rm -f` ⇒ daemon self-exits and cleans up.

### Implementation for User Story 4

- [X] T036 [US4] Implement marker adopt-or-reuse in `crates/deacon/src/commands/up/forward.rs`: read `forward_daemon_<id>.pid`; if it names a live pid, reuse it (no duplicate spawn); else spawn fresh (FR-012).
- [X] T037 [US4] Implement the `down` reap hook in `crates/deacon/src/commands/down.rs`: resolve container(s) via `ContainerIdentity`/label selectors (reuse existing resolution), read each marker, `SIGTERM` via `nix::sys::signal::kill`, wait briefly, remove marker, release that container's registry entries (FR-013). Per research.md Decision 8.
- [X] T038 [US4] Reap the existing forwarder on `up --remove-existing-container` before creating the replacement, in `crates/deacon/src/commands/up/container.rs`/`forward.rs` (FR-014).
- [X] T039 [US4] Implement daemon self-exit in `crates/core/src/port_forward/daemon.rs`: detect container gone (relay execs failing / `Docker::inspect_container` returns `None`), then exit after removing the marker and releasing registry entries (FR-015).

**Checkpoint**: Lifecycle is leak-free across down/replace/out-of-band removal.

---

## Phase 7: User Story 5 - Respect declared port intent and attributes (Priority: P3)

**Goal**: `portsAttributes.onAutoForward` (`ignore`/`silent`/`notify`) is honored for declared ports; `otherPortsAttributes` is the default for auto-detected ports; compose service-qualified `"service:port"` declared ports are forwarded.

**Independent Test**: Configure one port `onAutoForward: ignore` and one `silent`; assert the ignored port is not forwarded and the silent port is forwarded without a stderr notification, while a notify port produces one.

### Tests for User Story 5

- [X] T040 [P] [US5] Unit tests for attribute resolution in `crates/core/src/port_forward/` — declared port → `portsAttributes[port]`; undeclared → `otherPortsAttributes` default else `Notify`; `ignore` excluded; `"service:port"` declared spec parses to `(service, port)`.
- [X] T041 [P] [US5] Docker integration test in `crates/deacon/tests/integration_auto_forward.rs` — `ignore` port never forwarded; `silent` port reachable with no stderr mapping line; `notify` port reachable with a mapping line.
- [X] T042 [P] [US5] Docker integration test in `crates/deacon/tests/integration_auto_forward.rs` (compose) — a `"service:port"` declared port (e.g. `"db:5432"`) on a non-primary compose service is reachable on the host while auto-detection stays scoped to the primary service (FR-023).

### Implementation for User Story 5

- [X] T043 [US5] Resolve `ResolvedPortAttributes` from `DevContainerConfig.ports_attributes` / `other_ports_attributes` in `crates/deacon/src/commands/up/forward.rs` (pass into the daemon) / `crates/core/src/port_forward/daemon.rs`.
- [X] T044 [US5] Honor `onAutoForward` in `crates/core/src/port_forward/daemon.rs`: `ignore` ⇒ no listener; `silent` ⇒ forward but suppress the human stderr line; `notify` ⇒ forward + line (FR-017).
- [X] T045 [US5] Apply `otherPortsAttributes` as the default for auto-detected, undeclared ports in `crates/core/src/port_forward/daemon.rs` (FR-017).
- [X] T046 [US5] Parse service-qualified `"service:port"` declared specs into `ForwardSpec { service: Some(..), container_port }` in `crates/deacon/src/commands/up/forward.rs` (and the declared-port parsing in `crates/deacon/src/commands/up/ports.rs`); auto-detection remains scoped to the primary service (FR-023, FR-024).
- [X] T047 [US5] Make the relay dial the service host over the compose network when `ForwardSpec.service` is `Some` (i.e. `docker exec <primary> <relay> <service> <port>`) instead of `127.0.0.1`, in `crates/core/src/port_forward/relay.rs`/`daemon.rs` (FR-023). Builds on the configurable dial host from T014.

**Checkpoint**: All five user stories independently functional.

---

## Phase 8: Polish & Cross-Cutting Concerns

**Purpose**: Documentation, security, examples, and final gates.

- [X] T048 [P] Document `--auto-forward` as a deacon consumer extension (dynamic forwarding model, declared-vs-`-p`, loopback/TCP-only, best-effort, compose `"service:port"`) in `docs/subcommand-specs/up/SPEC.md`.
- [X] T049 [P] Add the new host surface (persistent host process + bound loopback host ports) and mitigations (loopback-only bind, `docker exec` is standard consumer behavior) to `SECURITY.md` (FR-022); flag for security review whether opt-in beyond the flag is warranted.
- [X] T050 [P] Add a `--auto-forward` section to `README.md` (usage + limits), aligned with `quickstart.md`.
- [X] T051 Create the `examples/auto-forward/` canary (`README.md`, `exec.sh`, `.devcontainer/devcontainer.json`) demonstrating loopback reach + multi-container; register it in the `examples/up` aggregator; clean up all resources; pin images (Principle IX). Note the in-repo git-root mount gotcha (`--mount-workspace-git-root false`).
- [X] T052 Run `quickstart.md` validation end-to-end (loopback reach, registry entry, clean teardown release) and fix any drift.
- [X] T053 Full gate: `cargo fmt --all -- --check`, `cargo clippy --all-targets --all-features -- -D warnings`, `make test-nextest`; confirm production code is `unsafe`-free and the workspace builds on non-Unix targets.

---

## Deferred Work

Per research.md Decision 7 and the spec's Out-of-Scope/Assumptions. A spec is NOT complete while these remain; track until resolved (Constitution Principle I).

- [ ] T054 [Deferral] Windows detach/signaling path (job objects / equivalent of `setsid`+`kill`). **Decision**: research.md Decision 1/7; gated by T008. **Acceptance**: `up --auto-forward` detaches and is reaped correctly on Windows hosts.
- [ ] T055 [Deferral] Persistent in-container multiplexing relay agent (one `docker exec`, many streams). **Decision**: research.md Decision 3/7. **Acceptance**: relay throughput no longer pays a `docker exec` per connection; same external behavior.
- [ ] T056 [Deferral] Event-driven detection (netlink/inotify) replacing the ~1 s poll. **Decision**: research.md Decision 2/7. **Acceptance**: forwards appear with lower latency and lower idle CPU; SC-002 still met.
- [ ] T057 [Deferral] `0.0.0.0`/LAN exposure option. **Decision**: spec Out of Scope. **Acceptance**: an explicit opt-in binds non-loopback with documented security review.
- [ ] T058 [Deferral] UDP forwarding. **Decision**: FR-003 (TCP-only v1). **Acceptance**: UDP listeners detected and relayed.
- [ ] T059 [Deferral] `openBrowser`/`openPreview` auto-open actions. **Decision**: FR-017, spec Out of Scope. **Acceptance**: those `onAutoForward` values trigger host browser/preview.
- [ ] T060 [Deferral] Configurable poll interval / standalone `forward` subcommand / `exec --auto-forward` attach. **Decision**: FR-004, spec Out of Scope. **Acceptance**: evaluated against demand; added without breaking the v1 boolean surface.

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (Phase 1)**: no dependencies.
- **Foundational (Phase 2)**: depends on Setup — **BLOCKS all user stories**.
- **US1 (Phase 3)**: depends on Foundational. The MVP.
- **US2 (Phase 4)**: depends on US1 (extends the detect/supervisor loop).
- **US3 (Phase 5)**: depends on US1's minimal registry (replaces it with the host-global version); independent of US2.
- **US4 (Phase 6)**: depends on US1 (markers/registry written by the daemon); independent of US2/US3 but most meaningful after US3.
- **US5 (Phase 7)**: depends on US1; the `"service:port"` relay (T047) builds on T014's configurable dial host; independent of US2/US3/US4.
- **Polish (Phase 8)**: after the desired stories.

### Story independence notes

- US1 is independently demoable. US2/US3/US4/US5 each layer onto US1 and are independently testable, but US2/US3/US4 are NOT mutually dependent — they can be staffed in parallel once US1 lands.
- US3 supersedes the US1 "minimal allocation" (T015) with the host-global registry (T030–T032); keep T015's API stable so US3 is a drop-in.

### Parallel Opportunities

- Setup: T001 then T002 (keep order).
- Foundational: T003 → T004 (same file `mod.rs`, sequential); T005/T006 (`cli.rs`/`args.rs`), T007 (`forward_daemon.rs`), and T008 (cfg gating) can overlap once T003 lands.
- US1 tests T009 + T010 + T011 in parallel (T009 different file; T010/T011 share the test file — coordinate or sequence the two integration cases).
- After US1: US2, US3, US4, US5 implementation can proceed in parallel by different developers (mostly different files; coordinate on `daemon.rs`).
- Polish T048/T049/T050 in parallel (different docs).

---

## Parallel Example: User Story 1

```bash
# Tests first (different files):
Task: "T009 Unit tests for /proc/net/tcp parser in crates/core/src/port_forward/detect.rs"
Task: "T010 Docker integration: loopback reach in crates/deacon/tests/integration_auto_forward.rs"

# Then implementation, respecting daemon.rs/registry.rs/relay.rs boundaries:
Task: "T012 parse_proc_net_tcp in detect.rs"
Task: "T014 relay.rs byte relay over docker exec -i"
Task: "T015 minimal single-port allocation in registry.rs"
```

---

## Implementation Strategy

### MVP First (User Story 1 only)

1. Phase 1 Setup → Phase 2 Foundational → Phase 3 US1.
2. **STOP and VALIDATE**: `up --auto-forward` reaches a `127.0.0.1`-bound server; shell is free; mapping on stderr; `up` JSON unchanged on stdout; absent-flag path unchanged.
3. Demo the one thing static `-p` can't do (SC-001).

### Incremental Delivery

1. Foundation + US1 → MVP (loopback reach).
2. + US2 → dynamic detection / withdrawal.
3. + US3 → multi-container collision-free (the real-world unlock).
4. + US4 → leak-free lifecycle.
5. + US5 → attribute fidelity + compose `"service:port"`.
6. Polish → docs, SECURITY.md, examples canary, full gate.

### Notes

- [P] = different files, no incomplete-task dependency.
- Reuse existing helpers (`ContainerIdentity`, `Docker::exec`, `cache/disk.rs` atomic write, `trust.rs` user-data folder) — do not reimplement (Constitution Principle VIII).
- Keep production code `unsafe`-free (`nix` wrappers); run fmt+clippy after every change; `make test-nextest` before PR.
- Add the new integration binary to all 3 `.config/nextest.toml` spots (T022); resolve UNION on conflicts.
