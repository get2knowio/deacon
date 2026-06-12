# What makes Deacon unique (net-positive differentiators)

A living list of places where Deacon **intentionally does more (or better)** than the
reference Dev Containers CLI (`@devcontainers/cli`) while staying spec-compliant — the
kind of thing worth calling out in a blog post or on the project page.

> Scope rule: entries here must be *net-positive* differences a user would be glad
> about (better DX, security, performance, robustness) — **not** spec divergences
> that are bugs. Bugs/divergences-to-fix live in `fixtures/parity-corpus/REPORT.md`.
> When you land a change that makes Deacon distinctively better, add it here.

## Developer experience

- **`--secrets-file` accepts both JSON *and* `.env`.** The reference CLI (and the
  spec) require a flat JSON object and reject a `KEY=VALUE` file with
  `Error: Invalid json data`. Deacon auto-detects the format (leading `{` → JSON,
  otherwise `KEY=VALUE`) and accepts either. JSON support is a strict superset, so a
  user can drop in the `.env` file they already have for `docker`/`compose` instead of
  hand-converting it to JSON. (`crates/core/src/secrets.rs`.)

## Capability

- **`extends` is resolved for `up` and `read-configuration --include-merged-configuration`.**
  The reference CLI v0.87.0 errors on a config that uses `extends`
  (`up`: "missing one of image/dockerFile/dockerComposeFile"; merged read-config:
  exit 1, empty stdout). Deacon resolves the full extends chain — base `image`,
  merged `containerEnv`, merged `forwardPorts`, etc. — so multi-file config
  composition just works.

## Robustness

- **Valid compose project names where the reference fails.** Both derive the compose
  project name from the workspace folder; the reference emits it verbatim, so a folder
  like `-myproj` produces an invalid `--project-name` that `docker compose` rejects
  (exit 1). Deacon trims leading separators / falls back to a safe stem, so the same
  folder still comes up. (Normal folders produce the identical `<folder>_devcontainer`
  name as the reference.) (`crates/core/src/compose.rs`.)

## Security

- **Workspace-trust gate for host-side lifecycle hooks.** `initializeCommand` (and
  future workspace-resident host hooks) run on the developer's host *before* any
  container sandboxing. Deacon gates these behind an explicit trust opt-in
  (`--trust-workspace[-persist]`, a persisted allowlist, or `DEACON_NO_PROMPT=1` to
  fail closed in CI) — a protection the upstream spec does not mandate. See
  `SECURITY.md` and `crates/core/src/trust.rs`.

## Performance & deployment

- **Single static Rust binary, no Node.js runtime.** Deacon ships as one native
  executable — nothing to `npm install`, no Node version to manage — which makes it
  cheap to drop into CI images and constrained environments.
- **Container environment-probe caching.** The shared `resolve_env_and_user()` path
  caches the per-container user-env probe (`{cache_folder}/env_probe_*`), giving a
  10–50× speedup (90–98% latency reduction) on repeat invocations across
  `up`/`exec`/`run-user-commands`. See `docs/ARCHITECTURE.md`.
