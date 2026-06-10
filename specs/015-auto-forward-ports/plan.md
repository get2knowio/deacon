# Implementation Plan: Dynamic User-Space Port Forwarding (`up --auto-forward`)

**Branch**: `015-auto-forward-ports` | **Date**: 2026-06-08 | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification from `/specs/015-auto-forward-ports/spec.md`

## Summary

Add a boolean `deacon up --auto-forward` flag that starts a **detached, host-side forwarder process** for a running devcontainer. The forwarder polls the container's TCP LISTEN sockets (`/proc/net/tcp{,6}` via `docker exec`), and for each detected (or declared) port opens a `127.0.0.1:<host-port>` listener on the host that relays bytes into the container's network namespace over `docker exec -i`. Declared ports (`forwardPorts`/`appPort`/`--forward-port`) are forwarded **eagerly** (host port reserved at `up` time) and are NOT statically `-p` published when `--auto-forward` is set; auto-detected ports are forwarded **lazily**. A **host-global registry file** under the user-data folder allocates collision-free host ports across concurrent devcontainers, guarded by an advisory file lock. The forwarder is single-owner per container (adopt-or-reuse via a pid marker), is reaped on `down` / `up --remove-existing-container`, and self-exits when its container vanishes. Forwarding is **best-effort**: failures warn loudly but never fail `up`. TCP only; loopback only; v1 targets Unix.

## Technical Context

**Language/Version**: Rust, Edition 2024, MSRV 1.95 (`workspace.package` in root `Cargo.toml`); `unsafe_code = "deny"` workspace-wide.  
**Primary Dependencies**: `tokio` (rt/process/fs/io-util/net), `clap` (CLI), `serde`/`serde_json` (registry + marker JSON), `tracing` (daemon logging), `thiserror` (core domain errors), `anyhow` (binary boundary), `directories-next` (user-data folder), `libc` (already in core). **New (Unix-only):** `nix` (features `process`, `signal` — safe `setsid()`, `kill()`, `Pid`, process-liveness checks without raw `unsafe`); `fs2` (advisory `flock` on the registry, auto-released on process death).  
**Storage**: Two host-side JSON files under the user-data folder (default `~/.deacon/`): a host-global `forwarded_ports.json` registry and per-container `forward_daemon_<container_id>.pid` markers; per-container `forward_daemon_<container_id>.log` log files. All writes use the temp-file + `fs::rename` atomic pattern (`crates/core/src/cache/disk.rs::save_index`).  
**Testing**: `cargo nextest` (unit + docker integration); new docker integration test binary registered in all three `.config/nextest.toml` profile spots per CLAUDE.md.  
**Target Platform**: Linux/macOS hosts with Docker; container side requires only a way to read `/proc/net/*` and a relay path. **Windows detach/signaling is deferred** (tracked, not a v1 gate — per spec Assumptions).  
**Project Type**: Single Rust workspace (CLI binary `crates/deacon` + library `crates/core`).  
**Performance Goals**: Detect a newly-listening port and make it reachable within ≤~2 s (SC-002); poll interval fixed ~1 s (FR-004). 0% host-port collisions across concurrent forwarders (SC-004).  
**Constraints**: Loopback-only host binds (FR-005); never require host root — privileged container ports (<1024) always remap to ≥1024 host ports (FR-009a); production code stays `unsafe`-free; no blocking IO in async paths (Principle V).  
**Scale/Scope**: Tens of forwarded ports across a handful of concurrent devcontainers on a developer machine / CI agent; one forwarder process per container.

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

| Principle | Status | Notes |
|-----------|--------|-------|
| I. Spec-Parity as Source of Truth | ✅ PASS (with note) | Dynamic forwarding is **not** mandated by upstream devcontainers/spec — it is a deacon consumer extension modeled on VS Code, and the issue explicitly notes this. It reuses spec vocabulary (`portsAttributes.onAutoForward`, `forwardPorts`, `appPort`, `"service:port"`). Behavior will be documented in `docs/subcommand-specs/up/SPEC.md` as a deacon-specific addition. No conflict with existing spec-defined behavior; static `-p` path is unchanged when the flag is absent (FR-007). |
| II. Consumer-Only Scope | ✅ PASS | Forwarding serves devcontainer *consumers*; `docker exec` into the container is standard consumer behavior (the `exec` subcommand already does it). No authoring surface. |
| III. Keep the Build Green | ✅ PASS | fmt/clippy/fast-tests after each change; full `make test-nextest` before PR; new tests added to nextest profiles. |
| IV. No Silent Fallbacks — Fail Fast | ✅ PASS (with rationale) | Relay-unavailable and daemon-start failures emit a **clear, non-silent** error to stderr + the forwarder log (FR-019). Per the clarified FR-025, forwarding is an *additive, best-effort* capability layered on a container that genuinely came up, so the error does not abort `up`. This is not a silent noop or a mock substitution — nothing is faked, and the error is loud. Documented as Decision 4 in research.md. |
| V. Idiomatic, Safe Rust | ✅ PASS | Daemon is a separate re-exec'd process (an in-process task cannot outlive `up`). Detachment/signaling use `nix` safe wrappers — production stays `unsafe`-free under the `deny` policy. New module decomposed into `detect`/`registry`/`relay`/`daemon` (Principle V modular boundaries). `thiserror` in core, async via `tokio`, no blocking IO in async. |
| VI. Observability & Output Contracts | ✅ PASS | Human-readable mappings → **stderr**; `up` result JSON on **stdout** unchanged (FR-010). Integrates the existing `PORT_EVENT:` channel for forward/unforward (FR-020). Daemon logs via `tracing` to a per-container file (FR-021). |
| VII. Testing Completeness | ✅ PASS | Unit: `/proc/net/tcp{,6}` parser (v4/v6/loopback fixtures), registry allocation/collision/release/stale-reap, marker adopt-or-reuse. Integration (docker): loopback reach, post-`up` detection, multi-container collision-free, `down`/replace reaping, self-exit. Nextest groups planned (see Structure). |
| VIII. Subcommand Consistency & Shared Abstractions | ✅ PASS | Reuses `ContainerIdentity`/labels for container resolution, the `Docker::exec` trait for socket scan + relay, the atomic temp+rename write pattern, the user-data-folder resolution that the trust store uses, and `down`'s identity/label selectors for reaping. New cross-cutting logic lives once in `crates/core/src/port_forward/`. |
| IX. Executable & Self-Verifying Examples | ✅ PASS | Adds an `examples/auto-forward/` canary with `exec.sh` demonstrating loopback reach + multi-container, registered in the `examples/up` aggregator; cleans up all resources. |

**Initial gate: PASS.** Two new dependencies (`nix`, `fs2`) are justified in Complexity Tracking and research.md; both are Unix-only, minimal, and chosen specifically to keep production code `unsafe`-free.

**Post-Design re-check (after Phase 1): PASS.** The data model (registry/marker/mapping) and contracts introduce no new constitution tension; all writes are atomic, all container resolution reuses `ContainerIdentity`, and the daemon entrypoint is a hidden internal subcommand that does not expand the user-facing surface beyond the single `--auto-forward` flag.

## Project Structure

### Documentation (this feature)

```text
specs/015-auto-forward-ports/
├── plan.md              # This file (/speckit.plan command output)
├── research.md          # Phase 0 output (/speckit.plan command)
├── data-model.md        # Phase 1 output (/speckit.plan command)
├── quickstart.md        # Phase 1 output (/speckit.plan command)
├── contracts/           # Phase 1 output (/speckit.plan command)
│   ├── cli.md           #   --auto-forward flag + hidden daemon subcommand contract
│   ├── registry.schema.json   # forwarded_ports.json entry schema
│   ├── marker.schema.json     # forward_daemon_<id>.pid marker schema
│   └── port-events.md   #   PORT_EVENT forward/unforward contract + stderr mapping format
├── checklists/
│   └── requirements.md  # Spec quality checklist (already complete)
└── tasks.md             # Phase 2 output (/speckit.tasks command - NOT created by /speckit.plan)
```

### Source Code (repository root)

```text
crates/core/src/
├── port_forward/                 # NEW module — pure logic + IO, unit-testable
│   ├── mod.rs                    #   public API surface (re-exports), shared types
│   ├── detect.rs                 #   parse /proc/net/tcp{,6} LISTEN rows → Vec<DetectedPort> (pure)
│   ├── registry.rs               #   host-global allocation registry: alloc/release/prune, flock, atomic write
│   ├── relay.rs                  #   per-connection byte relay over `docker exec -i`; relay-program selection
│   └── daemon.rs                 #   supervisor loop: detect → reconcile → relay; self-exit; per-container log
├── ports.rs                      # EXTEND: reuse PortEvent/OnAutoForward; add forward/unforward event emit
├── container.rs                  # REUSE: ContainerIdentity, labels(), label_selector()
├── cache/disk.rs                 # REUSE pattern: atomic temp+rename write
└── trust.rs                      # REUSE pattern: user_data_folder resolution (~/.deacon default)

crates/deacon/src/
├── cli.rs                        # EXTEND: add `--auto-forward` bool to Commands::Up; add hidden daemon subcommand
└── commands/
    ├── up/
    │   ├── args.rs               # EXTEND: add `auto_forward: bool` to UpArgs
    │   ├── container.rs          # HOOK: after lifecycle completes (~line 673), spawn/adopt forwarder; suppress -p for declared ports when set
    │   ├── ports.rs              # EXTEND: route declared ports to daemon vs static publish
    │   └── forward.rs            # NEW: spawn+detach forwarder (re-exec), marker adopt-or-reuse, declared-port wiring
    ├── forward_daemon.rs         # NEW: hidden `__forward-daemon` subcommand entrypoint (setsid, reopen stdio→log, run daemon loop)
    └── down.rs                   # EXTEND: reap forwarder(s) by pid marker, release registry ports

crates/deacon/tests/
└── integration_auto_forward.rs  # NEW docker integration test binary (registered in all 3 nextest profile spots)

examples/auto-forward/            # NEW canary: README + exec.sh (loopback reach + multi-container)

Cargo.toml (workspace)            # add nix (unix), fs2 to workspace.dependencies
crates/core/Cargo.toml            # add nix (cfg(unix)), fs2
.config/nextest.toml              # register integration_auto_forward in default + dev-fast(default-filter + override)
```

**Structure Decision**: Single Rust workspace. The reusable, runtime-agnostic logic (socket detection, host-port registry, relay, supervisor loop) lives in `crates/core/src/port_forward/` as a focused module decomposed per Principle V. The CLI binary owns only orchestration: the `--auto-forward` flag, the spawn/detach of the forwarder process, the hidden daemon entrypoint subcommand, and the `down` reap hook. This mirrors the existing split (e.g. `up` `{args,compose,lifecycle}`, `oci` `{auth,client,fetcher}`).

## Complexity Tracking

| Violation / Addition | Why Needed | Simpler Alternative Rejected Because |
|----------------------|------------|--------------------------------------|
| New dependency `nix` (Unix, `process`+`signal`) | Detached daemon needs `setsid()` (leave the controlling terminal/process group) and `kill()` / liveness checks for reaping and self-exit. | Raw `libc` calls require `unsafe`, and the workspace sets `unsafe_code = "deny"`. `nix` wraps these syscalls in safe APIs, keeping production code `unsafe`-free (the policy's preferred state). A scoped `#[allow(unsafe_code)]` was rejected to avoid normalizing `unsafe` in a long-lived daemon. |
| New dependency `fs2` | The host-global registry's allocate-and-bind critical section must be serialized across concurrent `up --auto-forward` processes; an advisory `flock` is auto-released when the holder dies. | An `O_EXCL` lockfile mutex (pure std) leaks on crash and needs hand-rolled stale-pid detection and backoff. `flock` releases on process death automatically — exactly the crash-safety the multi-container requirement needs. |
| Separate re-exec'd daemon process (not an in-process task) | FR-002/FR-021: forwarding must outlive `up` returning to the shell, with no attached terminal. | An in-process `tokio::spawn` task dies when the `up` command process exits — it cannot satisfy "returns control to the shell while forwarding stays active." A separate process is mandatory. |
| Hidden `__forward-daemon` subcommand | The re-exec'd process needs an entrypoint that runs the supervisor loop. | A separate helper binary would complicate packaging/distribution (single static binary is the deacon distribution model); re-exec'ing `current_exe()` with a hidden subcommand reuses the shipped binary. |
