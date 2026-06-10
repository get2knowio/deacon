# Phase 0 Research: Dynamic User-Space Port Forwarding (`up --auto-forward`)

All spec-level ambiguities were resolved in the spec's Clarifications section (two `/speckit.specify` + `/speckit.clarify` sessions on 2026-06-08). This document records the **technical** decisions that turn those clarified requirements into an implementable design, plus the deferrals required by the constitution's phased-implementation pattern.

No `NEEDS CLARIFICATION` markers remain in Technical Context.

---

## Decision 1 — Daemon is a separate, re-exec'd process (not an in-process task)

**Decision**: When `--auto-forward` is set and the container is healthy, `up` spawns a child process by re-exec'ing the current deacon binary (`std::env::current_exe()`) with a hidden `__forward-daemon` subcommand, passing the container id, workspace path, user-data folder, and the resolved declared-port set. The child detaches (`setsid()`, reopen stdio to the per-container log), records its pid in a marker, and runs the supervisor loop. `up` does **not** await the child and returns to the shell.

**Rationale**: FR-002 and FR-021 require forwarding to outlive `up` returning to the shell with no attached terminal. A `tokio::spawn` task lives inside the `up` process and dies when `up` exits, so it structurally cannot satisfy this. A separate OS process is mandatory. Re-exec'ing the shipped single binary (rather than a second helper binary) preserves deacon's static-binary distribution model.

**Detachment without `unsafe`**: The workspace sets `unsafe_code = "deny"`. Raw `libc::fork`/`setsid` need `unsafe`. The child therefore calls `nix::unistd::setsid()` (a safe wrapper) at the *start of its own `main`* (a normal function call, not a `pre_exec` closure), then reopens its standard fds onto the log file. Orphan reparenting to init happens naturally when the parent exits; `setsid` additionally divorces the daemon from the terminal session so a closing terminal (SIGHUP) does not kill it.

**Alternatives considered**:
- *In-process tokio task* — rejected: cannot outlive `up`.
- *Raw `libc` double-fork with scoped `#[allow(unsafe_code)]`* — rejected: normalizes `unsafe` in a long-lived component; `nix` gives the same result safely.
- *Separate helper binary* — rejected: complicates packaging vs. the single-binary model.

---

## Decision 2 — Port detection via `docker exec … cat /proc/net/tcp{,6}`, polled ~1 s

**Decision**: The supervisor polls every ~1 s (fixed constant, FR-004) by running `Docker::exec(container_id, ["cat","/proc/net/tcp","/proc/net/tcp6"], ExecConfig{ silent: true, .. })` and parsing the captured stdout. `detect.rs` parses rows where the state field `st == 0A` (TCP_LISTEN), decodes the `local_address` `HEXIP:HEXPORT`, and collects ports bound to `127.0.0.1`, `0.0.0.0`, `::`, `::1`. IPv4 addresses are little-endian hex (`0100007F` → `127.0.0.1`); the port is plain hex (`1F90` → 8080); tcp6 addresses are 32 hex chars in per-word little-endian order. The same logical port on v4 and v6 is deduplicated to one forward (FR-003).

**Rationale**: `/proc/net/tcp{,6}` is the same mechanism VS Code uses; it is process-agnostic (catches entrypoint/`postStart`/compose/`exec`-started servers, so `exec` needs no changes — FR-018) and requires nothing installed in the container to *detect*. `silent: true` already pipes and captures stdout in the existing exec impl (`docker.rs` ~1554). The format is stable and well-documented.

**Alternatives considered**:
- *`ss`/`netstat` inside the container* — rejected: not present on `alpine`/`distroless`; `/proc/net/*` is always present on Linux containers.
- *netlink/inotify event-driven detection* — deferred (Decision 7): poll is simplest for v1 and meets the ≤~2 s target (SC-002).

**Reference**: Linux kernel `Documentation/networking/proc_net_tcp.txt`; the `st=0A` LISTEN state and little-endian address encoding are confirmed in the sources cited at the bottom of this file.

---

## Decision 3 — Relay: embedded static relay binary primary; `socat`/`nc`/`/dev/tcp` fallback; fail-fast if none

**Decision**: For each forwarded `(container_port → host_port)`, the daemon binds a `tokio::net::TcpListener` on `127.0.0.1:host_port`. On each accepted host connection it spawns `docker exec -i <id> <relay>` that dials `127.0.0.1:<container_port>` inside the container, and pumps bytes bidirectionally (`tokio::io::copy` both ways). Relay-program selection order: (1) a tiny embedded static relay binary `docker cp`'d into the container on first use (works on `alpine`/`distroless`, no container deps); (2) fall back to `socat`, then `nc`, then bash `/dev/tcp` **only if detected present**; (3) if no relay strategy is available, surface a clear error per FR-019. Per-connection relay for v1.

**Rationale**: `docker exec` shares the container network namespace, which is *exactly* why this reaches `127.0.0.1`-bound servers that `-p` cannot (the headline capability, SC-001). Not assuming `socat` exists honors Principle IV (no silent fallback). The embedded-binary-first strategy keeps it working on minimal images.

**Alternatives considered**:
- *Assume `socat` present* — rejected: violates no-silent-fallback; absent on many images.
- *Persistent in-container multiplexing agent (one exec, many streams)* — deferred (Decision 7): a per-connection exec is simplest and correct for v1; the multiplexer is a throughput optimization.

**Open sub-decision deferred to implementation PR**: the exact embedded relay (a ~few-KB statically-linked helper vs. a shell one-liner when a shell exists). Tracked as a deferral; v1 acceptance only requires *a* working relay with fail-fast when none exists.

---

## Decision 4 — Forwarding is best-effort: failures warn loudly, `up` still succeeds (exit 0)

**Decision**: If the forwarder cannot be spawned/detached, the registry lock cannot be taken, or no relay mechanism exists, `up` emits a clear warning to stderr (and, where a daemon exists, its log) and returns success with the container running (FR-025). The error is never swallowed (FR-019).

**Rationale**: The container genuinely came up and is usable; forwarding is additive. Failing `up` would regress the non-`--auto-forward` path and block workflows where forwarding is a convenience. This is consistent with Principle IV because nothing is faked or silently downgraded — the failure is surfaced loudly; only the *exit code* treats forwarding as non-fatal. (Clarified with the user during `/speckit.clarify`.)

---

## Decision 5 — Host-global registry under the user-data folder, flock-guarded, atomic writes

**Decision**: Active allocations live in `{user_data_folder}/forwarded_ports.json` (user-data folder resolved exactly as the trust store: `--user-data-folder` else `~/.deacon`, per `trust.rs::trust_store_path`). The allocate-and-bind critical section is wrapped in an advisory `fs2` `flock` on a sibling lock file; entries are written with the temp-file + `fs::rename` atomic pattern (`cache/disk.rs::save_index`). Allocation policy: prefer the same number (container 3000 → host 3000) when free in both the registry and an actual bind probe; otherwise next free port. Privileged container ports (<1024) skip the same-number attempt and allocate ≥1024 (FR-009a). On daemon start and on each allocation, prune entries whose `pid` is dead or whose container no longer exists (FR-016). On shutdown/reap, remove that container's entries (FR-013).

**Rationale**: Host ports are a host-wide shared resource, so the registry must be host-global (not the per-workspace `--container-data-folder`). `flock` auto-releases on holder death — the crash-safety the multi-container requirement (User Story 3) needs. Atomic writes prevent the "trailing characters" JSON corruption noted in CLAUDE.md.

**Alternatives considered**:
- *`--container-data-folder` for the registry* — rejected: it may be per-workspace, defeating host-global collision avoidance.
- *`O_EXCL` lockfile mutex* — rejected: leaks on crash; needs hand-rolled stale detection.

---

## Decision 6 — Single-owner-per-container marker; declared ports eager, auto-detected lazy

**Decision**: A `{user_data_folder}/forward_daemon_<container_id>.pid` marker records the live forwarder for a container. `up --auto-forward` adopts-or-reuses: if the marker names a live pid, it does not spawn a duplicate (FR-012). Declared ports (`forwardPorts`/`appPort`/`--forward-port`, including `"service:port"`) are passed to the daemon and their host listeners opened **eagerly** at startup with the natural host port reserved; the relay connects lazily once the container port is observed listening (FR-024). Auto-detected, undeclared ports are bound **lazily** only once observed and torn down when they stop. When `--auto-forward` is set, declared ports are routed to the daemon and **suppressed** from the static `-p` publish args (FR-006) so Docker and the daemon never contend for the same host port.

**Rationale**: The container id is the natural single-owner key (each `up` yields a distinct `ContainerIdentity`). Eager declared-port forwarding matches VS Code (declared ports appear immediately and their natural host port is reserved against theft by a second container); lazy auto-detection avoids reserving ports for servers that never start. Mirrors the env-probe marker key pattern (`container_env_probe.rs:155`).

---

## Decision 7 — Phased scope: explicit v1 deferrals

Per Constitution Principle I (phased implementation) and the spec's Out-of-Scope/Assumptions, the following are intentionally deferred. Each has a corresponding entry to be carried into `tasks.md` "## Deferred Work" by `/speckit.tasks`:

1. **Windows detach/signaling** — Unix `setsid`/`kill` path only in v1; Windows job-object/detach path tracked separately (spec Assumptions).
2. **Persistent in-container multiplexing relay agent** — per-connection `docker exec` relay for v1; single-exec multiplexer is a throughput optimization (Decision 3).
3. **Event-driven detection (netlink/inotify)** — fixed ~1 s poll for v1 (Decision 2).
4. **`0.0.0.0`/LAN exposure** — loopback-only in v1 (FR-005).
5. **UDP forwarding** — TCP only in v1 (FR-003).
6. **`openBrowser`/`openPreview` auto-open** — honor `ignore`/`silent`/`notify` only; browser/preview opening optional (FR-017, spec Out of Scope).
7. **Configurable poll interval / standalone `forward` subcommand / `exec --auto-forward`** — not in v1 surface (FR-004, spec Out of Scope).

---

## Decision 8 — Reaping on `down` and replace reuses `ContainerIdentity`

**Decision**: `down` resolves the container(s) exactly as today (`ContainerIdentity` → state manager → `label_selector()`/`workspace_label_selector()` for `--all`), then for each resolved container reads the pid marker, sends `SIGTERM` (via `nix::sys::signal::kill`), waits briefly, removes the marker, and releases that container's registry entries (FR-013). `up --remove-existing-container` performs the same reap for the existing container before creating the replacement (FR-014). The daemon independently self-exits if `docker inspect` / relay execs show the container is gone (FR-015).

**Rationale**: Reuses existing container-resolution machinery (Principle VIII) rather than inventing a parallel lookup. SIGTERM (not SIGKILL) lets the daemon run its own cleanup (release ports, remove marker) for belt-and-suspenders correctness.

---

## Sources

- Linux kernel docs — `/proc/net/tcp` interface (state field, address encoding): <https://www.kernel.org/doc/Documentation/networking/proc_net_tcp.txt>
- Reading `/proc/net/tcp` (LISTEN `st=0A`, little-endian address/port decode): <https://blog.arkey.fr/2020/10/23/read-network-addresses-in-procfs/>
- VS Code Dev Containers port forwarding model & compose `"service:port"`: <https://code.visualstudio.com/docs/devcontainers/containers>
