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
  hand-converting it to JSON. (`crates/core/src/secrets.rs`.) Recorded in the
  conformance registry as extension `ext-secrets-file-env-format` (behavior
  `bhv-secrets-dotenv-superset`).

- **Config validation that fails fast on *your* mistakes, but never loses *your* data.**
  Deacon applies one consistent rule the reference CLI does not: **fail fast and
  precisely where the developer made a mistake; preserve silently where deacon
  doesn't model the field.**
  - *Precise early errors instead of confusing late ones.* The reference's
    `read-configuration` is a lenient parse-and-echo: it recovers from malformed
    JSONC by silently dropping the broken property, and accepts type errors like
    `forwardPorts: "3000"` or `features: "<ref>"` verbatim. Those mistakes don't
    vanish — they resurface much later as a misleading downstream failure (e.g. a
    dropped `"image":` becomes *"No image information specified"* at build time,
    pointing nowhere near the typo). Deacon rejects them up front with the exact
    cause and location (`JSONC parsing error: … line 6`, `expected an object (map),
    found a string`), and does so *consistently* across modeled fields — typed
    fields (`forwardPorts`) and object-shaped fields (`features`, `customizations`)
    are all held to their spec shape, so there's no "deacon caught this one but not
    that one" surprise.
  - *Forward-compatible field preservation.* For fields deacon does **not** model
    (new spec properties, editor-specific keys), it neither rejects (which would
    break a config that's valid in VS Code / the reference) nor silently drops them
    — it passes them through verbatim, matching the reference's fidelity. The spec's
    extensibility model assumes tools tolerate unknown fields; deacon honors that.
    (`crates/core/src/config.rs` — strict `deserialize_object_value` for
    `features`/`customizations`; `#[serde(flatten)] extra` round-trips unknown
    top-level fields. Differential coverage: `fixtures/parity-corpus/errors/`.)

  Recorded in the conformance registry under the `read-configuration` area: the
  strict-early-error cases as `intentional-divergence` behaviors
  (`bhv-readconfig-malformed-jsonc-rejected`,
  `bhv-readconfig-wrong-type-forwardports-rejected`,
  `bhv-readconfig-wrong-type-features-rejected`) and the preservation case as the
  `follow-spec` behavior `bhv-readconfig-unknown-field-preserved` — each with its
  three-axis disposition and migrated waiver. See the conformance registry for the
  full per-case detail rather than duplicating it here.

## Capability

- **`extends` is resolved for `up` and `read-configuration --include-merged-configuration`.**
  The reference CLI v0.87.0 errors on a config that uses `extends`
  (`up`: "missing one of image/dockerFile/dockerComposeFile"; merged read-config:
  exit 1, empty stdout). Deacon resolves the full extends chain — base `image`,
  merged `containerEnv`, merged `forwardPorts`, etc. — so multi-file config
  composition just works.

  This is an **ahead-of-spec capability**, tracking the in-flight proposal
  [devcontainers/spec#22](https://github.com/devcontainers/spec/issues/22), and
  is therefore an *intentional divergence* — deacon deliberately does more than
  the reference here, so `extends` behavior is **not** covered by our
  reference-parity claims. Recorded in the conformance registry as extension
  `ext-extends-resolution` (behaviors `bhv-readconfig-extends-merged`,
  `bhv-readconfig-extends-missing-rejected`, `bhv-readconfig-extends-cycle-rejected`;
  waivers `wvr-extends-child-merged`, `wvr-extends-missing`, `wvr-extends-cycle`);
  also characterized in `fixtures/parity-corpus/errors/README.md`. (Issue #297.)

## Robustness

- **Valid compose project names where the reference fails.** Both derive the compose
  project name from the workspace folder; the reference emits it verbatim, so a folder
  like `-myproj` produces an invalid `--project-name` that `docker compose` rejects
  (exit 1). Deacon trims leading separators / falls back to a safe stem, so the same
  folder still comes up. (Normal folders produce the identical `<folder>_devcontainer`
  name as the reference.) (`crates/core/src/compose.rs`.) Recorded in the conformance
  registry as behavior `bhv-compose-project-name-robust`.

## Security

- **Workspace-trust gate for host-side lifecycle hooks.** `initializeCommand` (and
  future workspace-resident host hooks) run on the developer's host *before* any
  container sandboxing. Deacon gates these behind an explicit trust opt-in
  (`--trust-workspace[-persist]`, a persisted allowlist, or `DEACON_NO_PROMPT=1` to
  fail closed in CI) — a protection the upstream spec does not mandate. See
  `SECURITY.md` and `crates/core/src/trust.rs`. Recorded in the conformance registry
  as extension `ext-workspace-trust-gate` (behavior `bhv-trust-host-hook-gate`).

## Performance & deployment

- **Single static Rust binary, no Node.js runtime.** Deacon ships as one native
  executable — nothing to `npm install`, no Node version to manage — which makes it
  cheap to drop into CI images and constrained environments.
- **Container environment-probe caching.** The shared `resolve_env_and_user()` path
  caches the per-container user-env probe (`{cache_folder}/env_probe_*`), giving a
  10–50× speedup (90–98% latency reduction) on repeat invocations across
  `up`/`exec`/`run-user-commands`. See `docs/ARCHITECTURE.md`.
