# CLI Contract: `--auto-forward`

## User-facing flag

Added to `Commands::Up` (`crates/deacon/src/cli.rs`) and threaded into `UpArgs` (`crates/deacon/src/commands/up/args.rs`) as `auto_forward: bool`.

```
deacon up --auto-forward
```

| Property | Value |
|----------|-------|
| Name | `--auto-forward` (NOT `--forward`; deliberately distinct from `--forward-port`) |
| Type | boolean flag (`#[arg(long)]`, no value) |
| Default | `false` (absent ⇒ today's static `-p` behavior, FR-007) |
| Scope | `up` subcommand only (no `exec`/standalone form in v1) |

### Behavior contract

- **Absent** (`false`): declared ports (`forwardPorts`/`appPort`/`--forward-port`) → `docker run -p` static publish; no forwarder. Byte-for-byte unchanged (FR-007, SC-006).
- **Present** (`true`):
  - Container is created/started normally, but declared ports are **suppressed from `-p`** and routed to the forwarder (FR-006).
  - After the container is healthy (post-lifecycle, `up/container.rs` ~line 673), `up` spawns-or-adopts the forwarder, then returns to the shell (FR-002).
  - Per-port loopback mappings are printed to **stderr**; the `up` result document on **stdout** is unchanged (FR-010).
  - If the forwarder cannot start, a clear warning is printed and `up` still exits `0` (FR-025).
  - On a **non-Unix** build (Windows), `--auto-forward` returns a clear "not supported on this platform" error (Unix-only in v1; no silent fallback). The rest of `up` is unaffected.

### Interaction with existing flags

| Flag | Interaction |
|------|-------------|
| `--forward-port <spec>` | Treated as a declared port; joins the daemon set (eager) when `--auto-forward` is set. |
| `--ports-events` | Forwarder emits `PORT_EVENT:` forward/unforward lines as ports come/go (FR-020; see `port-events.md`). |
| `--remove-existing-container` | Reaps the existing container's forwarder before replacing (FR-014). |
| `--user-data-folder` | Locates the host-global registry and markers (default `~/.deacon`). |

## Hidden daemon subcommand (internal)

The forwarder process is the deacon binary re-exec'd with a hidden subcommand. It is **not** part of the user-facing surface (hidden from `--help`); it exists only so `up` can spawn a detached forwarder from the shipped single binary.

```
deacon __forward-daemon \
  --container-id <id> \
  --workspace <canonical-path> \
  --user-data-folder <dir> \
  --declared-port <spec> [--declared-port <spec> ...] \
  --config <path>
```

| Property | Value |
|----------|-------|
| Visibility | `#[command(hide = true)]` — never shown in help/completions |
| Stability | Internal; arguments may change between releases without notice |
| Entry behavior | `setsid()`, reopen stdio → per-container log, then run the supervisor loop |
| Exit | Self-exits (0) when the container is gone or on SIGTERM after releasing ports/marker (FR-015) |

**Contract note**: end users MUST NOT invoke `__forward-daemon` directly; it is an implementation detail of `up --auto-forward`. Documented here only for completeness.
