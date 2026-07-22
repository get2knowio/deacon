# deacon

**The DevContainer CLI, minus the parts you don't use.**

A fast, focused Rust CLI for developers who use [dev containers](https://containers.dev) and CI pipelines — not for feature authors.

<!-- Badges -->
<p>
  <a href="https://github.com/get2knowio/deacon/releases/latest">
    <img alt="Latest Release" src="https://img.shields.io/github/v/release/get2knowio/deacon" />
  </a>
  <a href="https://github.com/get2knowio/deacon/actions/workflows/ci.yml?query=branch%3Amain">
    <img alt="CI" src="https://github.com/get2knowio/deacon/actions/workflows/ci.yml/badge.svg?branch=main" />
  </a>
  <a href="https://github.com/get2knowio/deacon/actions/workflows/codeql.yml?query=branch%3Amain">
    <img alt="CodeQL" src="https://github.com/get2knowio/deacon/actions/workflows/codeql.yml/badge.svg?branch=main" />
  </a>
  <a href="https://coveralls.io/github/get2knowio/deacon?branch=main">
    <img alt="Coverage" src="https://img.shields.io/coverallsCoverage/github/get2knowio/deacon?branch=main" />
  </a>
  <img alt="MSRV" src="https://img.shields.io/badge/MSRV-1.82-blue.svg" />
  <a href="https://github.com/get2knowio/deacon/security/policy">
    <img alt="Security Policy" src="https://img.shields.io/badge/security-policy-blue.svg" />
  </a>
  <a href="https://github.com/get2knowio/deacon/blob/main/LICENSE">
    <img alt="License: MIT" src="https://img.shields.io/badge/License-MIT-green.svg" />
  </a>
</p>

## Install

```bash
curl -fsSL https://get2knowio.github.io/deacon/install.sh | bash
```

While only pre-release builds are published, install the latest pre-release with:

```bash
curl -fsSL https://get2knowio.github.io/deacon/install.sh | bash -s -- --prerelease
```

<details>
<summary>Other installation methods</summary>

### macOS (Homebrew) - Coming Soon
```bash
brew install get2knowio/tap/deacon
```

### Manual Download
Download from [releases](https://github.com/get2knowio/deacon/releases/latest):

| Platform | Architecture | Download |
|----------|--------------|----------|
| Linux | x86_64 | [deacon-linux-x86_64.tar.gz](https://github.com/get2knowio/deacon/releases/latest/download/deacon-v0.2.0-x86_64-unknown-linux-gnu.tar.gz) |
| Linux | ARM64 | [deacon-linux-arm64.tar.gz](https://github.com/get2knowio/deacon/releases/latest/download/deacon-v0.2.0-aarch64-unknown-linux-gnu.tar.gz) |
| Linux (musl) | x86_64 | [deacon-linux-musl-x86_64.tar.gz](https://github.com/get2knowio/deacon/releases/latest/download/deacon-v0.2.0-x86_64-unknown-linux-musl.tar.gz) |
| macOS | x86_64 | [deacon-darwin-x86_64.tar.gz](https://github.com/get2knowio/deacon/releases/latest/download/deacon-v0.2.0-x86_64-apple-darwin.tar.gz) |
| macOS | ARM64 (Apple Silicon) | [deacon-darwin-arm64.tar.gz](https://github.com/get2knowio/deacon/releases/latest/download/deacon-v0.2.0-aarch64-apple-darwin.tar.gz) |
| Windows | x86_64 | [deacon-windows-x86_64.zip](https://github.com/get2knowio/deacon/releases/latest/download/deacon-v0.2.0-x86_64-pc-windows-msvc.zip) |
| Windows | ARM64 | [deacon-windows-arm64.zip](https://github.com/get2knowio/deacon/releases/latest/download/deacon-v0.2.0-aarch64-pc-windows-msvc.zip) |

### From Source
```bash
git clone https://github.com/get2knowio/deacon.git
cd deacon
cargo build --release
./target/release/deacon --help
```

### Installer Options
The install script supports a `--prerelease` flag and these environment variables:
- `--prerelease` (or `DEACON_PRERELEASE=true`) — Install the latest release including pre-releases
- `DEACON_VERSION` — Specific version (default: latest stable)
- `DEACON_INSTALL_DIR` — Install directory (default: `~/.local/bin`)
- `DEACON_FORCE=true` — Overwrite existing binary without prompt

```bash
# Install a specific version
curl -fsSL https://get2knowio.github.io/deacon/install.sh | DEACON_VERSION=0.2.0-rc.15 bash

# Install the latest pre-release
curl -fsSL https://get2knowio.github.io/deacon/install.sh | bash -s -- --prerelease
```

</details>

## Quick Start

```bash
# Start a dev container
deacon up

# Run a command in the container
deacon exec -- npm install

# Stop and remove the container
deacon down
```

Verify installation:
```bash
deacon --version
```

## Auto-forward ports (`up --auto-forward`)

A deacon extension (modeled on VS Code Dev Containers) that dynamically forwards
container TCP ports to **loopback** host ports — including `127.0.0.1`-bound
servers that static `-p` publishing cannot reach.

```bash
# Start the container and a detached forwarder, then return to the shell.
deacon up --auto-forward
#   stderr: Forwarding container 3000 -> http://127.0.0.1:3000 (web)
curl http://127.0.0.1:3000        # reaches a loopback-only server in the container

# Ports that start later (entrypoint / postStart / exec) are auto-detected
# within ~1-2s; no extra flags needed. Multiple devcontainers get collision-free
# host ports (the actual port is always reported, e.g. "(remapped; host 3000 in use)").

deacon down                        # reaps the forwarder and releases its host ports
```

How it works: the forwarder polls the container's listening sockets and relays
bytes via `docker exec` into the container's network namespace. Declared ports
(`forwardPorts`/`appPort`/`--forward-port`) forward eagerly and are not also
`-p` published; `portsAttributes.onAutoForward`
(`ignore`/`silent`/`notify`/`openBrowser`/`openBrowserOnce`/`openPreview`) is
honored; compose `"service:port"` declared ports are forwarded too.

**Auto-open a browser.** A port whose `onAutoForward` is `openBrowser` (every
time it forwards) or `openBrowserOnce` (the first time per forwarder session)
opens your browser at the forwarded loopback URL. `openPreview` has no CLI
analog and is treated as `notify`. Which browser is a **machine-owner** choice,
resolved with precedence:

```bash
DEACON_BROWSER=firefox deacon up --auto-forward     # env var (highest)
# or persist it: ~/.deacon/settings.json  ->  { "browser": "firefox" }
# else the OS default opener (xdg-open / open / start) is used.
```

The value is a bare program (the URL is appended as the final argument — no
shell). Auto-open is **skipped in CI / non-TTY sessions** unless a browser is
explicitly configured. Nothing in `devcontainer.json` can choose the program;
the workspace can only *request* an open via `onAutoForward` (a loopback URL).

**Limits (v1):** loopback-only (never `0.0.0.0`/LAN), TCP only, Unix hosts only,
and best-effort — if forwarding (or the browser launch) can't start you get a
clear warning but `up` still succeeds. Forwarder logs:
`~/.deacon/forward_daemon_<container_id>.log`.

## Corporate CA injection (`--inject-host-ca`)

On a machine behind a TLS-intercepting corporate proxy, dev containers need the
corporate root CA to validate HTTPS. Two capabilities (opt-in for the second):

- **Always on:** deacon's own feature/template pulls trust the host OS trust
  store automatically — if your browser works behind the proxy, `deacon up` can
  fetch features with no configuration. `DEACON_CUSTOM_CA_BUNDLE=/path.pem` is
  still additive on top.
- **Opt-in, machine-side:** inject the corporate CA into the container at build
  and runtime so feature installs and `postCreateCommand` network calls succeed.

```bash
deacon up --inject-host-ca                      # auto-discover the corporate root CA delta
deacon up --inject-host-ca /etc/corp/root.pem   # or inject a specific PEM bundle verbatim
deacon build --inject-host-ca                    # build-time injection into the feature Dockerfile
DEACON_INJECT_HOST_CA=auto deacon up             # env var (CI / one-shot)
```

Persist it for every `up`/`build` via `~/.deacon/settings.json`
(`{ "hostCa": "auto" }`). Precedence: `--inject-host-ca` flag >
`DEACON_INJECT_HOST_CA` > `settings.json` > off. Activation is **machine-owner
controlled only** — nothing in `devcontainer.json` or the workspace can enable
or redirect it (see [SECURITY.md](SECURITY.md#corporate-ca-injection---inject-host-ca)).

When enabled, deacon installs the CA into the container trust store **before**
any lifecycle hook and sets the standard CA env vars (`SSL_CERT_FILE`,
`NODE_EXTRA_CA_CERTS`, `REQUESTS_CA_BUNDLE`, `PIP_CERT`, `GIT_SSL_CAINFO`,
`CURL_CA_BUNDLE`). On an unsupported distro or non-root container it warns and
falls back to env-var-only. deacon never rewrites user-authored Dockerfiles —
see SECURITY.md for the manual `ARG`/`COPY` convention.

## User profiles (`--profile`)

Profiles are named, mutually-exclusive startup configurations kept in the
**machine-owner's** `~/.deacon/settings.json` — never in a project. A repo cannot
define or select a profile. The file is **read-only** here (hand-edit it; a
`deacon settings` write command is tracked as [#198](https://github.com/get2knowio/deacon/issues/198)).

Select one with the global `--profile <NAME>` flag (or `DEACON_PROFILE` env var),
honored by `up`, `read-configuration`, `build`, and `outdated` (not `set-up`).
Selection precedence: `--profile` > `DEACON_PROFILE` > `defaultProfile` (in
settings) > none. With no profiles — or no `defaultProfile` — a bare command
behaves exactly as it does today.

```json
{
  "browser": "firefox",
  "defaultProfile": "dev",
  "profiles": {
    "dev":   { "mergeConfig": "overrides/dotfiles.json" },
    "agent": { "mergeConfig": "overrides/agent.json", "browser": "none" }
  }
}
```

Each profile's `mergeConfig` is a `devcontainer.json` fragment **deep-overlaid**
on the base config. Fragment paths resolve relative to `~/.deacon/` and may be a
single string or an ordered array (later entries win). The full layering
precedence, low→high, is:

```
base config                          (discovered devcontainer.json + extends,
                                      OR the --override-config file if given)
  <  root "mergeConfig"              (settings.json, always applied — optional)
      <  selected profile "mergeConfig"
          <  --merge-config          (CLI, repeatable, highest merge layer)
```

Two distinct CLI flags act on configuration:

- **`--override-config <path>`** — **replaces** the discovered base config with
  this file (resolved through its own `extends` chain), matching the reference
  devcontainer CLI. Also `DEACON_OVERRIDE_CONFIG`.
- **`--merge-config <path>`** — **deep-overlays** this fragment onto the base
  (repeatable; later wins). This is the CLI analogue of a profile `mergeConfig`.
  A single path is also settable via `DEACON_MERGE_CONFIG`.

A profile may also override the root scalars `browser` / `hostCa` (profile value
wins over root; a CLI flag or env var still wins over both). The reserved value
`browser: "none"` (case-insensitive) **disables** port auto-open rather than
naming a program.

Naming an undefined profile — via `--profile` or a dangling `defaultProfile` —
errors and lists the available profiles. When a profile is applied, deacon logs a
diagnostic to **stderr** naming it; stdout / `--output json` documents are
unchanged.

## Shipped Commands

| Command | Description |
|---------|-------------|
| `up` | Create and start a dev container (features installed at build time via BuildKit) |
| `down` | Stop and remove a dev container or compose project |
| `exec` | Execute a command in a running container |
| `build` | Build a dev container image (with feature layering for Dockerfile configs) |
| `read-configuration` | Resolve and output `devcontainer.json` (with extends + variable substitution) |
| `run-user-commands` | Run lifecycle commands in an existing container |
| `set-up` | Convert an already-running container into a DevContainer (lifecycle + dotfiles + `/etc` patches) |
| `upgrade` | Regenerate the lockfile from the currently resolved feature set |
| `outdated` | Report current / wanted / latest feature versions |
| `templates apply` | Scaffold a project from a template |
| `config` | Configuration management subcommands |
| `doctor` | Environment diagnostics and support bundle creation |

## Known limitations

| Limitation | Notes |
|---|---|
| **Podman runtime** | **Supported.** A required CI lane runs the integration suite against rootless Podman, and `up`/`exec`/`down`/`run-user-commands`/`set-up` handle Podman's image-ref qualification, `ps`-JSON shape, SELinux `label=disable`, and rootless `--userns=keep-id`. Remaining gap: **GPU passthrough is not wired for Podman** (`--gpu detect` cleanly reports no GPU; CDI/`--device` support is a follow-up). See [#30](https://github.com/get2knowio/deacon/issues/30). |
| **`build` features** | Feature installation during `build` is supported for **Dockerfile-based** configs only. Compose-build and image-reference configs still error out with features (different integration patterns; tracked as a post-1.0 follow-up). |

For the full 1.0 roadmap, see [docs/ROADMAP_TO_1.0.md](docs/ROADMAP_TO_1.0.md). Post-1.0 hardening landed in May 2026 — redaction wiring into the tracing pipeline, a [workspace-trust gate](#workspace-trust) for host-side lifecycle hooks, async I/O conversion across `crates/core`, typed errors throughout `crates/core`, and the `json5`→`jsonc-parser` migration (see closed [#52](https://github.com/get2knowio/deacon/issues/52)).

## How deacon verifies correctness

deacon is a reimplementation, so "is it correct?" is really two questions: does it
match the pinned [containers.dev spec](https://containers.dev), and does it match the
pinned reference CLI (`@devcontainers/cli` v0.87.0)? Those can disagree — and
sometimes deacon differs from both on purpose.

**[docs/PARITY_AND_CONFORMANCE.md](docs/PARITY_AND_CONFORMANCE.md)** explains the
machinery that keeps those questions separate and answerable: the parity harness
(which *finds* differences) versus the conformance registry (which *explains*
them), what **divergence**, **gap**, **waiver**, and **out of scope** each mean,
which of the two CI gates actually blocks a release, and what to do when you find a
difference.

Start there if you've seen those words in a PR and weren't sure whether they meant
the same thing.

## Filesystem Artifacts

Like the reference DevContainers CLI, deacon keeps its machine-level state (the
OCI cache, lifecycle-resume markers, build cache, feature entrypoint wrappers,
trust store, port-forward registry) in the host **user-data folder** (`~/.deacon/`
by default, override with `--user-data-folder`) — never inside your project. The
**only** file written into the workspace is the spec-mandated feature lockfile
(`devcontainer-lock.json`), which is meant to be committed.

For the full enumeration (what each path is for, a parity comparison with the
reference CLI, and a `.gitignore` snippet), see
[docs/FILESYSTEM_ARTIFACTS.md](docs/FILESYSTEM_ARTIFACTS.md).

## Examples

Self-contained categorized examples live under [`examples/`](examples/README.md):

- Configuration: variable substitution, lifecycle commands basics (`examples/configuration/`)
- Container Lifecycle: lifecycle command execution, ordering, and variables (`examples/container-lifecycle/`)
- Feature System: dependencies, parallelism, caching, and lockfile support (`examples/features/`)
- Template Management: template application with options (`examples/template-management/`)

See the full details and additional commands in `examples/README.md`.

## Runtime Selection

Deacon supports **Docker** (default) and **Podman**. Podman is exercised by a
required CI lane that runs the integration suite against rootless Podman, and
the consumer commands handle Podman's specifics automatically: local image-ref
qualification (`localhost/…`), the `podman ps` JSON shape, SELinux relabeling
(`--security-opt label=disable`), and rootless UID mapping (`--userns=keep-id`
for non-root users). The one remaining gap is GPU passthrough, which is not yet
wired for Podman (tracked in [#30](https://github.com/get2knowio/deacon/issues/30)).

```bash
# Explicitly select Docker (optional, it's the default)
deacon --runtime docker up

# Select Podman
deacon --runtime podman up

# Or via environment variable
DEACON_CONTAINER_RUNTIME=podman deacon up
```

## Runtime Configuration

### Logging
Deacon supports both human-readable text and structured JSON logging formats.

**Text Logging (Default):**
```bash
deacon --help  # Standard text output
```

**JSON Logging:**
```bash
export DEACON_LOG_FORMAT=json
deacon doctor  # Structured JSON logs for machine parsing
```

The JSON format is useful for CI/CD systems and log aggregation tools that need structured data.

### Quiet mode and spinner (TTY only)

The default log level is `warn`, so routine `info` progress noise stays out of your way unless you ask for it. When running in a real terminal (stderr is a TTY) and using the default text output, deacon also shows a small spinner during long-running operations like `up` and `down`. JSON mode or non‑TTY environments (CI, redirections) do not render a spinner.

Verbosity is a single axis (`error < warn < info < debug < trace`). Use the `-v`/`-q` shortcuts to shift it, or `--log-level` to set the baseline directly:

- `-v`/`--verbose` raises verbosity one step (`-v`=info, `-vv`=debug, `-vvv`=trace); repeatable.
- `-q`/`--quiet` lowers it (`-q`=error, silencing warnings).
- `--log-level <level>` sets the baseline that `-v`/`-q` shift from.
- `DEACON_LOG`/`RUST_LOG` take precedence over all of the above.

Tips:
- Want details? Use `-v` (or `--log-level info`); `-vv` for debug.
- Prefer structured logs? Use `--log-format json` (no spinner) and parse stderr.

Color and accessibility:
- Help/usage output uses automatic color when writing to a terminal. Spinner/status messages also use subtle colors (yellow for in‑progress, green for success, red for failures).
- Respecting your environment, color is disabled when not writing to a TTY and when `NO_COLOR` is set (see https://no-color.org/). To force-disable colors, export `NO_COLOR=1`.

### Build output

When `deacon up` or `deacon build` runs `docker build` (for a Dockerfile, a
feature-extended image, or a compose service), the way that build's output is
presented follows the same verbosity/TTY signals as logging:

| Situation | What you see |
|-----------|--------------|
| Interactive terminal, default verbosity | **Compact**: one collapsing line per build step (feature-install steps are named), with a live spinner for the current step. On failure, only the **failing step's log tail** is shown — not the whole BuildKit firehose. |
| Interactive terminal, `-v`/`--verbose` | **Inherit**: the terminal is handed to BuildKit so you get its full native, collapsing progress UI. |
| Non‑TTY (CI, redirection), `--log-format json`, or an explicit `--progress` | **Plain**: build output is streamed verbatim to stderr as it arrives (stdout stays reserved for the command result). |

In all modes stdout stays reserved for the command's result (so `--output json`
/ piped output remain parseable); build progress and diagnostics go to stderr.

### Lifecycle command output

Lifecycle commands (`onCreate`, `postCreate`, `postStart`, …) follow the same
signals:

| Situation | What you see |
|-----------|--------------|
| Interactive terminal, default verbosity | **Compact**: output is buffered behind the per‑phase spinner (`Running postCreate…` → `postCreate completed in N ms`). On failure the captured output is replayed (redacted, tail‑capped to the last 100 lines) so you still see why it failed. |
| `-v`/`--verbose`, non‑TTY (CI, redirection), or `--log-format json` | **Streamed**: each command's output is forwarded to stderr verbatim as it runs, exactly as before. |

This keeps a verbose `postCreateCommand` (e.g. an installer that downloads
hundreds of MB) from burying the progress spinner, while a failing hook still
surfaces its log. It applies to string and array (`exec`) form commands; parallel
(object‑form) and dotfiles installation still stream so their interleaved output
stays attributable.

### PTY Allocation for JSON Log Mode

When using JSON logging format (`--log-format json`), lifecycle commands (onCreate, postCreate, etc.) run without PTY (pseudo-terminal) allocation by default. This is ideal for non-interactive scripts and automated environments.

However, if your lifecycle commands need interactive terminal behavior while maintaining structured JSON logs, you can force PTY allocation:

**Via CLI Flag:**
```bash
deacon up --log-format json --force-tty-if-json
```

**Via Environment Variable:**
```bash
# Enable PTY allocation
export DEACON_FORCE_TTY_IF_JSON=true
deacon up --log-format json

# Disable PTY allocation (default)
export DEACON_FORCE_TTY_IF_JSON=false
deacon up --log-format json
```

**Truthy values** (case-insensitive): `true`, `1`, `yes`
**Falsey values**: `false`, `0`, `no`, or unset

**Precedence:**
1. CLI flag (`--force-tty-if-json`)
2. Environment variable (`DEACON_FORCE_TTY_IF_JSON`)
3. Default (no PTY allocation)

**Important Notes:**
- This setting only applies when `--log-format json` is active
- With PTY allocation enabled, interactive commands work correctly while JSON logs remain structured on stderr and machine-readable output stays on stdout
- Without PTY allocation (default), lifecycle commands run in non-interactive mode

## Output Streams

Deacon follows a strict stdout/stderr separation contract to ensure reliable machine-readable output:

### Stream Usage Contract

1. **JSON Output Modes** (`--output json`, `--json` flags):
   - **stdout**: Single JSON document (newline terminated), nothing else
   - **stderr**: All logs, diagnostics, and progress messages via `tracing`
   - **Guarantee**: Scripts parsing stdout will receive only valid JSON

2. **Text Output Modes** (default):
   - **stdout**: User-facing result summaries and human-readable reports only
   - **stderr**: All logs, diagnostics, and progress messages via `tracing`
   - **Note**: Text format content may evolve; use JSON modes for stable parsing

3. **Error Conditions**:
   - **Non-zero exit**: stdout may be empty unless partial results are explicitly supported
   - **All errors**: Logged to stderr, never stdout

### Examples

```bash
# JSON mode - stdout contains only JSON, logs go to stderr
deacon read-configuration --output json > config.json 2> logs.txt

# Text mode - stdout contains human-readable results, logs to stderr
deacon doctor > diagnosis.txt 2> logs.txt

# Parsing JSON output safely
OUTPUT=$(deacon read-configuration 2>/dev/null)
echo "$OUTPUT" | jq '.configFilePath'
```

### Integration Guidelines

- **Automation/CI**: Always use JSON output modes (`--json`, `--output json`) for reliable parsing
- **Human Use**: Default text modes provide better readability and context
- **Logging**: Use `--log-level` and `--log-format` to control stderr verbosity and format

### Docker Integration
All Docker functionality is available when Docker is installed and running. If the Docker daemon isn’t available, deacon emits a clear runtime error with guidance to install or start Docker.

## Workspace Trust

`deacon up` runs `initializeCommand` and any custom dotfiles install command on the **host** — outside the container sandbox. To prevent arbitrary host-side execution when cloning hostile repos, deacon gates these hooks behind a workspace-trust check that fails closed by default.

```bash
# Untrusted workspaces error out:
$ deacon up
Error: workspace is not trusted: /path/to/workspace
       Re-run with --trust-workspace (or --trust-workspace-persist to remember).

# Opt in for this run only:
deacon --trust-workspace up

# Opt in and remember (persisted to ~/.local/share/deacon/trusted_workspaces.json):
deacon --trust-workspace-persist up

# CI fail-closed mode — never auto-trust, error immediately on first untrusted run:
DEACON_NO_PROMPT=1 deacon up
```

The check only fires when host-side hooks are actually configured. Containers without `initializeCommand` or a host-side dotfiles install are unaffected. See [SECURITY.md](./SECURITY.md#workspace-trust-model-host-side-lifecycle-hooks) for the full threat model.

## Usage

### Reading DevContainer Configuration
The `read-configuration` command loads, processes, and outputs your devcontainer.json:

```bash
# In a directory with .devcontainer/devcontainer.json or .devcontainer.json
deacon read-configuration

# With explicit config path
deacon read-configuration --config /path/to/devcontainer.json

# Include merged configuration (with extends resolution)
deacon read-configuration --include-merged-configuration

# With debug logging
deacon read-configuration --log-level debug
```

**Example output:**
```json
{
  "name": "my-dev-container",
  "image": "mcr.microsoft.com/devcontainers/base:ubuntu",
  "workspaceFolder": "/workspaces/my-project",
  "features": {
    "ghcr.io/devcontainers/features/docker-in-docker:2": {}
  },
  "customizations": {
    "vscode": {
      "extensions": ["ms-python.python"]
    }
  }
}
```

**Variable Substitution:**
Variables like `${localWorkspaceFolder}` are automatically replaced:
```json
{
  "workspaceFolder": "${localWorkspaceFolder}/src",
  "containerEnv": {
    "PROJECT_ROOT": "${localWorkspaceFolder}"
  }
}
```
becomes:
```json
{
  "workspaceFolder": "/home/user/project/src",
  "containerEnv": {
    "PROJECT_ROOT": "/home/user/project"
  }
}
```

### Logging

Deacon uses the `tracing` ecosystem for structured logging.

- Default log level: `warn`
- Default log format: text (human-readable)
- CLI flag overrides: `--log-level` (`error|warn|info|debug|trace`), `-v`/`--verbose` and `-q`/`--quiet` (repeatable shortcuts that shift the baseline), `--log-format` (`text|json`)
- Environment overrides (take precedence before CLI flag processing sets `RUST_LOG`):
  - `DEACON_LOG`: Full filter specification (e.g. `DEACON_LOG=deacon=debug,deacon_core=debug`)
  - `RUST_LOG`: Standard Rust filter fallback if `DEACON_LOG` unset

When you specify `--log-level`, the CLI sets `RUST_LOG` internally to `deacon=<level>,deacon_core=<level>` prior to initializing the subscriber. Use `DEACON_LOG` for advanced per-module filtering; it will be honored as-is (and will emit a warning and fall back to `info` if invalid).

Guidance on levels:
- `info`: High-level milestones and user‑visible state changes (container start/stop, template application summary, configuration load boundaries)
- `debug`: Detailed decision points (config discovery paths, feature resolution steps, variable substitution reports)
- `trace`: Very fine‑grained internals (iteration over collections, per-file copy decisions) – typically for deep troubleshooting only
- `warn`: Recoverable issues or unexpected states deviating from normal expectations
- `error`: Failures causing command termination or skipped critical workflow phases

Examples:
```bash
# Increase verbosity for troubleshooting configuration issues
deacon read-configuration --log-level debug

# Use JSON logs (machine parsing / CI ingestion)
deacon up --log-format json

# Advanced module filtering (show trace for config, keep others at info)
DEACON_LOG=deacon_core::config=trace,deacon=info deacon read-configuration
```

If you disable secret redaction (`--no-redact`), secret values may appear in logs—avoid in shared terminals or CI unless strictly necessary.

### Development Build
```bash
cargo run -- --help
cargo test
```

### Running Tests

The project supports both traditional `cargo test` and parallel execution via [cargo-nextest](https://nexte.st/).

#### Standard Test Commands (Serial Execution)
```bash
# Run all tests serially (default)
make test

# Fast feedback loop: unit + bins + examples + doctests (no integration)
make test-fast

# Development fast loop: fmt-check + clippy + fast tests
make dev-fast
```

#### Parallel Test Execution with cargo-nextest

For faster feedback, install [cargo-nextest](https://nexte.st/):
```bash
cargo install cargo-nextest --locked
# Or follow: https://nexte.st/book/pre-built-binaries.html
```

Then use the nextest targets:
```bash
# Fast parallel tests (excludes smoke/parity tests)
make test-nextest-fast

# Full test suite with parallel execution and test grouping
make test-nextest

# CI-aligned conservative profile
make test-nextest-ci
```

**Test Groups**: The project organizes tests into groups based on resource requirements:
- **docker-exclusive**: Tests requiring exclusive Docker daemon access (serial)
- **docker-shared**: Tests that can share Docker daemon (limited parallelism)
- **fs-heavy**: Filesystem-intensive tests (limited parallelism)
- **unit-default**: Fast unit tests with high parallelism
- **smoke**: High-level integration tests (serial)
- **parity**: Upstream CLI comparison tests (serial)

Timing data is automatically captured in `artifacts/nextest/` for performance analysis.

**Fallback Behavior**: If cargo-nextest is not installed, the Make targets will fail with clear installation instructions. Always keep the standard `make test` working as a fallback.

#### Test Classification Checklist

When adding new tests, classify them into the appropriate group:

1. **Does the test use Docker?**
   - No → `unit-default` or `fs-heavy`
   - Yes → Continue to step 2

2. **Does it require exclusive Docker daemon access?**
   - Yes (manipulates daemon state) → `docker-exclusive`
   - No (just runs containers) → `docker-shared`

3. **Does it perform heavy filesystem operations?**
   - Yes (large files, many I/O ops) → `fs-heavy`

4. **Is it an end-to-end integration test?**
   - Yes (validates complete workflow) → `smoke`
   - Yes (compares with upstream CLI) → `parity`

**Audit test assignments:**
```bash
# List all tests with their group classifications
make test-nextest-audit
```

**Example test naming for automatic classification:**
```rust
// Docker-exclusive tests
#[test]
fn integration_lifecycle_full_up_down() { ... }

// Docker-shared tests  
#[test]
fn integration_build_with_cache() { ... }

// Smoke tests
#[test]
fn smoke_basic_workflow() { ... }
```

#### Troubleshooting Test Issues

**Flaky tests in parallel execution:**
- Test passes with `make test` but fails with `make test-nextest`
- **Solution**: Reclassify to more conservative group (e.g., `docker-shared` → `docker-exclusive`)
- Update test name or `.config/nextest.toml` filter
- Verify with `make test-nextest-audit`
- Validate with multiple runs of `make test-nextest`

**Tests requiring specific order:**
- **Preferred**: Refactor test to be independent
- **If unavoidable**: Move to `smoke` group (serial execution)
- Document the dependency in test comments

**Slow tests:**
- Profile to identify bottleneck
- Consider splitting into smaller tests
- Move inherently slow end-to-end tests to `smoke` group
- Use `#[ignore]` for very slow tests: `cargo nextest run -- --ignored`

For comprehensive troubleshooting, classification workflows, and remediation steps, see [docs/testing/nextest.md](docs/testing/nextest.md).

## Roadmap

Deacon focuses on consuming dev containers — building, running, and managing them — rather than authoring reusable features or publishing to registries. Coverage continues to expand across these specification domains:

- Configuration resolution and parsing (`devcontainer.json`)
- Feature consumption: installing and resolving community features during container builds
- Template system for scaffolding new projects
- Container lifecycle management
- Docker/OCI integration
- Cross-platform support

See the [containers.dev specification](https://containers.dev) and the [reference CLI](https://github.com/devcontainers/cli) for authoritative behavior.

### Binary Authenticity & Code Signing

Current release artifacts (tar.gz / zip) are **not yet code signed**. Integrity is provided via `SHA256SUMS` published with every release. You should always:

```bash
# Download archive and checksum listing
curl -LO https://github.com/get2knowio/deacon/releases/download/<version>/SHA256SUMS
grep '<archive-filename>' SHA256SUMS | sha256sum -c -
```

Planned enhancements (no tracking issue yet — file one if you need any of these prioritized):
- GPG detached signature for `SHA256SUMS` (`SHA256SUMS.asc`)
- macOS codesign + notarization
- Windows Authenticode signature
- Supply chain provenance (SLSA build attestation)

Until signatures are in place, rely on checksum verification and the GitHub release provenance. If you need reproducible build parity, the workflow captures the exact `rustc -Vv` used in each release (`RUSTC_VERSION.txt` asset). Deterministic builds for comparison can be performed via:

```bash
rustup toolchain install $(grep '^release:' RUSTC_VERSION.txt | cut -d':' -f2 | xargs)
cargo build --release --locked --all-features
```

If you have requirements around signed binaries and would like to help accelerate this, comment on the tracking issue once it is opened.

## Contributing

See [CONTRIBUTING.md](./CONTRIBUTING.md) for development workflow, testing guidelines, and contribution requirements.

## Continuous Integration

CI runs via GitHub Actions and uses the Makefile + cargo-nextest:

- Lint: rustfmt check, cargo check, clippy, and doctests
- Test (Ubuntu): `make test-nextest-fast` with Docker available
- Smoke (Ubuntu): `make test-smoke` (serial, Docker required)
- Nextest CI (Ubuntu/macOS): `make test-nextest-ci` with timing artifact `artifacts/nextest/ci-timing.json`
- Other OS (macOS/Windows): runs unit + non‑smoke integration tests and separate smoke tests; macOS uses Colima for Docker

Notes:
- Networked integration tests run only in selected jobs (smoke and nextest-ci) via `DEACON_NETWORK_TESTS=1` to keep the fast test job hermetic.
- Test grouping and concurrency are configured in `.config/nextest.toml`. See docs/testing/nextest.md for details.

## Test Coverage

We use cargo-llvm-cov (LLVM source-based coverage) locally and in CI.

- Install toolchain addon and helper:
	- rustup component add llvm-tools-preview
	- cargo install cargo-llvm-cov

- Run coverage locally and open HTML report:
	- cargo llvm-cov --workspace --open

- Generate LCOV for external services:
	- cargo llvm-cov --workspace --lcov --output-path lcov.info

CI enforces a minimum line coverage threshold (see MIN_COVERAGE in `.github/workflows/ci.yml`). To try the same locally:

- cargo llvm-cov --workspace --fail-under-lines 80

Coverage reporting is published to Coveralls for the `main` branch and PRs: https://coveralls.io/github/get2knowio/deacon
