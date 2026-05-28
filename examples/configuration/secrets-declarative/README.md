# Configuration: Declarative `secrets`

The spec's `secrets` top-level property lets a `devcontainer.json`
**document the secrets it expects** without storing their values. Each
entry is a `description` + `documentationUrl` keyed by the env var name.
IDEs and CI runners use this metadata to:

1. Prompt the user to supply the secret (with link to docs on how to
   obtain it).
2. Redact the supplied value from logs once it's injected via
   `--secrets-file` or `remoteEnv`.

The example declares two secrets — `GITHUB_TOKEN` and `DATABASE_URL` —
and wires them into the container's environment via
`remoteEnv: ${localEnv:...}` substitution. `exec.sh` injects fake values
locally so the assertions are reproducible.

## Files

- `devcontainer.json` — declares both secrets and a
  `postCreateCommand` that records the first 8 chars of `$GITHUB_TOKEN`
  into a canary file. The canary is what we use to verify the value
  reached the container; redaction is verified by greping the
  lifecycle stderr/logs separately.

## Scenarios exercised by `exec.sh`

1. **`read-configuration` exposes the secrets declaration.** The parsed
   JSON contains `secrets.GITHUB_TOKEN.description` and `documentationUrl`
   verbatim.
2. **Substitution wires the secret in.** With `GITHUB_TOKEN=demo-fake-token-1234`
   set in the local env, the container's `postCreateCommand` writes
   `demo-fak` (the first 8 chars) to `/tmp/redaction-canary`.
3. **Redaction proof.** Capture deacon's stderr and assert the literal
   token value does NOT appear in any log line.

## Manual usage

```sh
# 1. Declare the secrets locally.
export GITHUB_TOKEN="demo-fake-token-1234"
export DATABASE_URL="postgres://dev:dev@localhost:5432/dev"

deacon read-configuration --workspace-folder . | jq '.configuration.secrets'

# 2. Bring up the container and check the canary.
deacon up --workspace-folder . --remove-existing-container
docker exec <cid> cat /tmp/redaction-canary  # demo-fak
```

## Known deacon issues this example surfaces

- [#66](https://github.com/get2knowio/deacon/issues/66) — `read-configuration`
  rejects `--config` alone, demanding `--workspace-folder` even when the
  config already points at a complete file.
- [#67](https://github.com/get2knowio/deacon/issues/67) — `--workspace-folder`
  is replaced by the git root for *discovery*, so this example silently
  loads the deacon repo's own `.devcontainer/devcontainer.json` when run
  from inside the repo.

## Spec references

- Declarative secrets:
  <https://github.com/devcontainers/spec/blob/main/docs/specs/declarative-secrets.md>
- Secrets injection + redaction:
  <https://github.com/devcontainers/spec/blob/main/docs/specs/secrets-support.md>
