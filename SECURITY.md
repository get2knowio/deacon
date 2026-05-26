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
