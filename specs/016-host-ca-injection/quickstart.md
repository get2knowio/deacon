# Quickstart: Corporate CA (Host Trust Store) Support

**Feature**: 016-host-ca-injection

This shows the end-to-end machine-owner workflow on a corporate laptop behind a TLS-intercepting proxy.

## 0. Nothing to do for deacon's own pulls (always on)

Once this feature ships, deacon's own OCI feature/template pulls trust the **host OS trust store**
automatically. If your browser works behind the corporate proxy, `deacon up` can fetch features with no
configuration:

```bash
deacon up            # feature pulls succeed; corporate root CA already trusted via the host store
```

`DEACON_CUSTOM_CA_BUNDLE=/path/extra-roots.pem` still works and is **additive** on top of the host store.

## 1. Turn on container injection persistently (settings file)

Provision `~/.deacon/settings.json` (or `{--user-data-folder}/settings.json`) — hand-edited or pushed by
IT. Deacon reads it on every `up`/`build`:

```bash
mkdir -p ~/.deacon
printf '{ "hostCa": "auto" }\n' > ~/.deacon/settings.json     # auto-discover corporate roots
# or point at a specific bundle:
printf '{ "hostCa": "/etc/corp/zscaler-root.pem" }\n' > ~/.deacon/settings.json
```

> A `deacon settings get/set` command to manage this file is planned (tracked in issue #198). Until
> then, edit the file directly or use the flag / env var in step 3.

## 2. Bring up a container — CA is injected before lifecycle hooks

```bash
deacon up
# logs (stderr) show, e.g.:
#   ca.discover: 1 corporate certificate(s): CN=ACME Corp Root CA, O=ACME, C=US
#   ca.inject:   installed into system store (debian) at /usr/local/share/deacon/host-ca.crt
```

A `postCreateCommand` that runs `npm install` / `pip install` over the proxy now succeeds, because the
CA was installed **before** the hook ran and the CA env vars are set.

## 3. One-shot / CI without persisting a setting

```bash
DEACON_INJECT_HOST_CA=auto deacon up          # env var
# or:
deacon up --inject-host-ca                     # flag, auto-discovery
deacon up --inject-host-ca /etc/corp/root.pem  # flag, explicit bundle
deacon build --inject-host-ca                  # build-time injection into generated feature Dockerfile
```

Precedence: `--inject-host-ca` flag > `DEACON_INJECT_HOST_CA` > `settings.json` > off.

## 4. exec / run-user-commands after up

```bash
deacon exec env | grep -E 'SSL_CERT_FILE|NODE_EXTRA_CA_CERTS'
# -> SSL_CERT_FILE=/usr/local/share/deacon/host-ca.crt   (read back from container labels; no re-discovery)
```

## 5. Verify the cert landed in the container

```bash
deacon exec cat /usr/local/share/deacon/host-ca.crt | head -1   # -----BEGIN CERTIFICATE-----
# debian/ubuntu:
deacon exec ls /usr/local/share/ca-certificates/ | grep deacon
```

## Degraded paths (never silent)

- **Unsupported distro / non-root user**: deacon warns and falls back to **env-var-only** injection —
  the six CA env vars still point at the written bundle; the system store just isn't updated.
- **Zero corporate certs discovered**: deacon logs "0 corporate certs" and proceeds with no injection.
- **Unreadable / non-PEM explicit bundle**: deacon fails fast naming the path and reason.

## Not done for you

- deacon does **not** modify user-authored Dockerfiles. If your own `Dockerfile` needs the cert at build
  time, use the documented manual ARG/COPY convention (README / SECURITY.md).
- Injection is **machine-owner controlled only** — nothing in `devcontainer.json` or the workspace can
  enable or redirect it.
