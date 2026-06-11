# Host CA Injection (`up --inject-host-ca`)

Demonstrates injecting a corporate root CA into a dev container so builds and
lifecycle hooks trust a TLS-intercepting proxy. Machine-owner controlled — the
activation never comes from the workspace (see
[SECURITY.md](../../../SECURITY.md#corporate-ca-injection---inject-host-ca)).

## What it shows

- `deacon up --inject-host-ca <bundle.pem>` installs the CA into the container
  **before** any lifecycle hook. The `postCreateCommand` asserts the canonical
  bundle (`/usr/local/share/deacon/host-ca.crt`) exists at hook time, so the run
  fails if injection were ordered after hooks.
- The six CA env vars (`SSL_CERT_FILE`, `NODE_EXTRA_CA_CERTS`, …) are set in the
  container and re-applied to `exec` sessions from the container label.
- The `up` JSON result gains an additive `injectedCaSubjects` array.

## Run it

```bash
./exec.sh
```

The script generates a throwaway self-signed "corporate" root CA with `openssl`,
injects it as an explicit bundle (so the demo doesn't depend on your host trust
store), verifies the cert + env var inside the container, and cleans everything
up.

> The base image (`debian:bookworm-slim`) ships without the `ca-certificates`
> package, so deacon installs the bundle file and the CA env vars but logs an
> env-var-only fallback for the system trust store. Images that include the
> updater (most language/dev images) get the cert in the system store too. Auto
> mode (`--inject-host-ca` with no value) discovers the corporate delta from
> the host trust store instead of using an explicit bundle.

## Cleanup

`exec.sh` removes the container and the temporary CA directory automatically.
