# Security Policy

## Supported versions

| Version | Supported |
|---------|-----------|
| `main`  | ✅ (active development) |
| `1.0.x` | ✅ (once cut) |
| `< 1.0` | ❌ |

## Reporting a vulnerability

If you believe you've found a security vulnerability in deacon, **please do
not file a public issue.** Instead, use one of the private channels below:

- **GitHub Security Advisory** (preferred): open a private report at
  <https://github.com/get2knowio/deacon/security/advisories/new>.
- **Email**: send details to `security@get2know.io` with the subject line
  starting `[deacon]`.

Please include:

- A description of the vulnerability and its impact.
- Reproduction steps or a proof-of-concept, if available.
- The affected version(s) (commit hash or release tag).
- Your name/handle for credit (or "anonymous" if you prefer).

We aim to acknowledge reports within **3 business days** and to provide a
remediation plan within **14 days** of acknowledgement. Critical issues
affecting users of released versions will be prioritized.

## Scope

deacon is a command-line tool that manages DevContainers. The following
attack surfaces are in scope for reports:

- **Command injection** via untrusted workspace inputs (lifecycle commands,
  dotfiles install commands, feature option strings, etc.).
- **Path traversal** in feature/template extraction, lockfile derivation, or
  configuration file resolution.
- **Secret leakage** through logs, JSON output, or persisted state — the
  redaction layer (`crates/core/src/redaction.rs`) is the trust boundary.
- **TLS / OCI authentication** in the registry client
  (`crates/core/src/oci/`).
- **Container runtime privilege escalation** via mount, capability, or
  security-option handling.

The following are **out of scope**:

- Vulnerabilities in upstream Docker / Podman / BuildKit. Please report
  those to the respective projects.
- Vulnerabilities in features authored by third parties. The
  feature-installer trust boundary is documented in `CLAUDE.md`.
- Sandboxed compromise of feature install scripts running inside the
  containers deacon launches — by design these run with whatever
  privileges the container/Dockerfile grant.
- Denial-of-service against the local CLI process (e.g. crafted
  `devcontainer.json` that triggers an OOM).

## Workspace-trust model (host-side lifecycle hooks)

`initializeCommand` (and any other host-side hook that runs unsandboxed
shell on the developer's machine, e.g. a workspace-sourced dotfiles
install command) is gated by a workspace-trust check. The threat the
gate addresses: `git clone <hostile-repo> && deacon up` would otherwise
execute arbitrary shell from `devcontainer.json` on the host before any
container sandboxing.

**Policy resolution** (see `crates/core/src/trust.rs`):

- `--trust-workspace` — one-shot trust for the current invocation.
- `--trust-workspace-persist` — one-shot trust plus appends the
  canonicalized workspace path to
  `{user_data_folder}/trusted_workspaces.json` so future invocations
  pass the gate without a flag.
- `DEACON_NO_PROMPT=1` — switches the default from "allowlist-then-fail"
  to a hard `Deny`. Set this in CI so untrusted workspaces fail loudly
  instead of silently looking like a normal run failure.
- Default (no flag, no env) — consult the persisted allowlist; if the
  current canonical workspace path is present, allow; otherwise refuse.

The trust file is written atomically (write-temp-then-rename) so a
mid-write crash leaves either the previous file or the new file on
disk, never a partial one.

This trust check is **deacon-specific**: the upstream
[containers.dev spec](https://containers.dev) does not mandate it.

## Dynamic port forwarding (`up --auto-forward`)

`up --auto-forward` (a deacon-specific extension, not part of the
containers.dev spec) introduces two new host-side surfaces, both opt-in
behind the flag:

- **A persistent host process** — a detached forwarder
  (`__forward-daemon`) survives `up` and runs until the container is torn
  down or vanishes. It is single-owner per container (pid marker), reaped
  on `down`/replace, and self-exits when its container is gone, so it does
  not silently accumulate. Its only privileged action is `docker exec`
  into the user's own container — the same capability the `exec`
  subcommand already exposes — to read `/proc/net/tcp{,6}` and run a relay
  program. It never executes workspace-sourced host shell, so it is not a
  new host-trust vector.
- **Bound host ports** — for each forward the daemon binds a TCP listener.
  These binds are **always `127.0.0.1` (loopback) only** — never
  `0.0.0.0`/LAN — so forwarded container services are reachable only from
  the local host, not the network. Privileged container ports (<1024)
  always remap to an unprivileged host port, so the forwarder never needs
  host root. Allocations are recorded in a host-global
  `{user_data_folder}/forwarded_ports.json` registry written atomically
  under an advisory file lock; every host port is unique file-wide.

Mitigations: loopback-only binds, no host root, `docker exec` is standard
consumer behavior, and the whole feature is off unless `--auto-forward` is
passed. LAN/`0.0.0.0` exposure is explicitly out of scope for v1 and would
require a separate, documented opt-in. Reviewers evaluating whether
forwarding warrants opt-in beyond the flag itself should weigh that it
only ever exposes the user's own container to the user's own loopback.

## Corporate CA injection (`--inject-host-ca`)

Behind a TLS-intercepting corporate proxy, dev containers need the corporate
root CA to validate HTTPS. deacon ships two capabilities for this (016):

- **Always-on (deacon's own client).** deacon's own OCI pulls trust the host
  OS trust store (union of the bundled public roots + host roots, plus the
  additive `DEACON_CUSTOM_CA_BUNDLE`). This changes only what *deacon* trusts
  for its own requests; it injects nothing into containers.
- **Opt-in, machine-side injection.** When enabled, deacon discovers the
  corporate root CA delta on the host and installs it into the container — at
  build time (a deterministic `RUN` step in the deacon-generated
  feature-layering Dockerfile, before any feature `install.sh`) and at runtime
  (streamed over `docker exec -i` into the distro trust store before any
  lifecycle hook, plus six synthesized CA env vars: `SSL_CERT_FILE`,
  `NODE_EXTRA_CA_CERTS`, `REQUESTS_CA_BUNDLE`, `PIP_CERT`, `GIT_SSL_CAINFO`,
  `CURL_CA_BUNDLE`).

**Threat model — machine-owner controlled only.** Injection activation is
read **only** from machine-owner sources, in precedence order:

1. the `--inject-host-ca [PATH]` flag (on `up` and `build`),
2. the `DEACON_INJECT_HOST_CA` environment variable,
3. `{user_data_folder}/settings.json` (`{ "hostCa": "auto" | "<abs path>" }`).

It is **never** read from the workspace — nothing in `devcontainer.json` or any
workspace-resident file can enable, disable, or redirect injection (FR-015,
SC-007). This mirrors the [workspace-trust gate](#workspace-trust-model-host-side-lifecycle-hooks):
a hostile workspace cloned onto a corporate machine cannot trick deacon into
trusting or installing an attacker-chosen CA. The CLI tier resolves activation
via `resolve_host_ca_activation_cli`, which by contract receives only the three
sources above; adding a workspace-sourced input there is a security regression
and must be rejected in review.

**Degraded paths never silent.** Unsupported distro / non-root → warn + the
six CA env vars still point at the written bundle (env-var-only). Unreadable or
non-PEM explicit bundle → fail fast naming the path. Zero corporate certs
discovered → log and proceed without injection.

**User-authored Dockerfiles are not rewritten.** deacon never edits a
user-authored `Dockerfile`. If your own Dockerfile needs the CA at build time,
mirror deacon's convention manually: declare a build context and copy the
bundle before your network steps, e.g.

```dockerfile
# docker build --build-context corp_ca=/etc/corp ...
RUN --mount=type=bind,from=corp_ca,target=/tmp/corp \
    cp /tmp/corp/root.pem /usr/local/share/ca-certificates/corp.crt \
    && update-ca-certificates
```

The settings file is **read-only** in this release; a `deacon settings get/set`
command to manage it is tracked in issue #198.

## Security-relevant CI gates

- `cargo-deny` (advisories + bans + licenses + sources) runs on every PR
  and on a daily schedule. See `.github/workflows/ci.yml` `security` job
  and `deny.toml`.
- CodeQL scanning runs on every PR and weekly. See
  `.github/workflows/codeql.yml`.
- Release artifacts are attested via SLSA provenance. See
  `.github/workflows/release.yml`.

## Coordinated disclosure

We follow a 90-day coordinated-disclosure window. Once a fix is released
and users have had a reasonable upgrade period, the advisory is
published with credit to the reporter.
