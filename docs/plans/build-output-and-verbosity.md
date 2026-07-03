# Plan: Feature `containerEnv` fix, verbosity flags, and build-output UX

## Context

Running `deacon up` on a real project (`../hola`, a Node/agentic devcontainer with the
`node` + several custom features) surfaced three distinct problems:

1. **A real spec-parity bug:** the `node` feature installs Node via nvm and exposes it
   only through `containerEnv.PATH = "/usr/local/share/nvm/current/bin:${PATH}"`. deacon
   applied feature `containerEnv` only to the **final runtime container**, never as
   `ENV` in the generated feature-install Dockerfile — so a later feature's `install.sh`
   that calls `npm` (e.g. `ai-clis` installing Gemini CLI) failed with
   `npm: command not found` (exit 127). The upstream devcontainer CLI emits each
   feature's `containerEnv` as `ENV` lines between install steps.
2. **Noisy default logging:** routine, non-actionable lines were logged at WARN
   (`installsAfter` targets not in the set; `GPU mode 'detect'` finding no runtime).
3. **Ugly build output:** deacon buffers `docker build` via `.output()` and never sets
   `--progress`, so BuildKit dumps its raw `plain` firehose — and only surfaces it (the
   whole buffer) on failure. There is no live progress and no compact view.

The intended outcome: correct feature env propagation, a quiet-by-default single
verbosity axis with `-v/-q`, and a build UX that is **compact per-feature by default**
and **full live BuildKit output under `--verbose`**.

This work spans two repos:
- `deacon` (`/Users/paul/GitHub/deacon`) — all code changes below.
- `devcontainer-features` (`/Users/paul/GitHub/devcontainer-features`) — one optional
  defensive change to `ai-clis`.

Execution happens in a devcontainer that has the Rust toolchain (the authoring machine
has Docker via OrbStack but no `cargo`/`rustc`, so nothing below has been compiled yet —
all verification is deferred to that environment).

---

## Status of already-open PRs (verify + merge first)

These are pushed and CI-pending; treat them as **Part 0**. In the devcontainer: build,
run `cargo fmt --all -- --check`, `cargo clippy --all-targets --all-features -- -D warnings`,
`make test-nextest`, then merge (squash).

- **PR #251** — `refactor(logging): downgrade expected feature/GPU detect noise`
  (`installsAfter` unresolved → `debug!`; GPU `detect` no-runtime → `info!`; probe-error
  stays `warn!`). Files: `crates/core/src/features.rs`, `crates/deacon/src/commands/up/mod.rs`.
- **PR #252** — `fix(features): propagate feature containerEnv to build-time via ENV`.
  File: `crates/core/src/dockerfile_generator.rs`. **This is the `npm: command not found`
  fix.** Verify end-to-end (see Verification §A) before merging.
- **PR #253** — `feat(cli): add -v/-q verbosity flags; default log level to warn`.
  Files: `crates/deacon/src/cli.rs`, `README.md`. Foundation for Part 1 gating.

> Merge order suggestion: #251, then #252, then #253 (independent, but #253's default-WARN
> change is the switch Part 1 reads). Rebase Part 1 branches on `main` after these land.

---

## Part 1: Build-output rendering (Phase B — new work)

**Behavior matrix**, gated on the verbosity resolved by `resolve_log_level` (#253) plus
TTY/JSON detection:

| Mode | Condition | Behavior |
|------|-----------|----------|
| **Compact** | default (warn), stderr is TTY, not JSON | Parse `--progress=plain`; render one collapsing line per feature via `indicatif::MultiProgress`. On failure: show only the failing step's log tail. |
| **Inherit** | `-v`/verbose (info+), stderr is TTY | Hand the terminal to buildx (`--progress=auto`, child stdio inherited) → native BuildKit collapsing UI. deacon does **not** capture (no retry/trim in this mode — decision confirmed). |
| **Plain** | non-TTY, CI, `--log-format json`, or `--progress json` | Stream lines straight to stderr verbatim, capture for uniform error handling. |

Applies to **both** `deacon up` (image, feature, compose paths) and `deacon build`.

### B1 — BuildKit plain-output parser (pure, unit-tested; land first)

New module `crates/deacon/src/ui/build_progress.rs` (deacon crate — it already depends on
`indicatif = "0.18"`; `crates/core` does not).

- A stateful `BuildProgress` model consuming lines via `push_line(&str)`:
  - Parse the BuildKit plain grammar: `#<N> [<stage> <k>/<M>] <op...>`,
    `#<N> <elapsed> <msg>`, `#<N> DONE <t>s`, `#<N> CACHED`, `#<N> ERROR: ...`, and the
    trailing `------` / `Dockerfile:NN` error block.
  - Recognize feature-install steps by the mount marker already emitted by
    `generate_feature_install_command` (`crates/core/src/dockerfile_generator.rs:278`):
    `--mount=...,source=<sanitized_id>_<level>,...`. Map `<sanitized_id>` back to a
    friendly feature name (reuse the sanitize scheme from
    `DockerfileGenerator::sanitize_feature_id`, `dockerfile_generator.rs:331`).
- Public surface:
  - `steps()` / iterable current state (id, label, status: running/done/cached/error,
    elapsed) — consumed by the MultiProgress renderer.
  - `failing_step_log() -> Option<String>` — the failing `#N` step's captured lines
    (tail-capped) for the failure-trim (#1).
- Unit tests over a captured fixture: save a real `--progress=plain` build log from
  `hola` (success + a forced failure) under `crates/deacon/tests/fixtures/buildkit/` and
  assert step extraction, feature mapping, and `failing_step_log()`. **Fully verifiable
  with `cargo test`, no Docker.**

### B2 — Streaming build execution + `--iidfile`

- **Stream instead of buffer.** Refactor `run_build_with_retry`
  (`crates/core/src/docker_retry.rs:262`, currently `.output()` at `:279`) to accept an
  optional per-line sink and stream merged stdout+stderr line-by-line (tokio piped
  child + `BufReader::lines`) while still accumulating a captured buffer for retry
  classification and error text. Preserve the existing `retry_async`/`RetryConfig::network()`
  wrapper; on a retry, reset the sink.
- **Inherit mode** bypasses the sink: spawn with inherited stdio (no capture) — used only
  when the resolved mode is Inherit.
- **`--iidfile`** replaces stdout image-ID parsing:
  - Inject `--iidfile <tmp>` in `generate_build_args`
    (`crates/core/src/dockerfile_generator.rs:420`) and in the two inline arg builders:
    `build_image_from_config` (`crates/deacon/src/commands/up/image_build.rs`, drop the
    `-q` reliance at `:124`/`:150`) and `deacon build`'s inline path
    (`crates/deacon/src/commands/build/mod.rs:~1706`, read iidfile instead of stdout at
    `~1976`).
  - Feature builds via `build_image` (`crates/core/src/docker.rs:2539`) already **discard**
    the parsed ID (`features_build.rs:183`/`:376` use a pre-chosen tag), so they only need
    the arg injected. The fragile multi-pattern `extract_sha256` parser
    (`docker.rs:2560-2610`) can then be retired or reduced to an iidfile read.
- **Compose:** `ComposeCommand::execute` (`crates/core/src/compose.rs:~271`) already pipes;
  thread the same per-line sink so `build_service` (`compose.rs:1080`) can render. Compose
  build has no arg-injection seam for `--iidfile` (not needed — output is discarded).

### B3 — Mode resolution + threading + renderer wiring

- **Resolve the mode once** in `crates/deacon/src/cli.rs` (where `self.verbose`,
  `resolve_log_level`, `stderr.is_terminal()`, and `is_json_log_format()` are all known —
  same block as `spinner_eligible`, `cli.rs:~1167`). Produce a
  `BuildOutputMode { Compact, Inherit, Plain }`.
- **Thread it** to the build call sites. Cleanest carrier is `BuildOptions`
  (`crates/core/src/build/mod.rs:35`) because it is the one value reaching
  `generate_build_args`. Add an `output_mode` field (default `Plain`) and:
  - Populate it in `build_options_from_args` (`crates/deacon/src/commands/up/args.rs:555`)
    and the `deacon build` equivalent — which means passing the resolved mode into
    `UpArgs`/`BuildArgs` (add a field; both already carry the `ProgressTracker` Arc, so the
    dispatch sites at `cli.rs:1341`/`1505` are the place to set it).
  - **Caveat:** `to_docker_args()` is only emitted when `!is_default()`
    (`generate_build_args:432`). Inject `--progress`/`--iidfile` **outside** the
    `is_default()` guard, and make `is_default()` ignore `output_mode`, so setting a mode
    does not accidentally start emitting cache args.
- **Renderer.** New `MultiProgress`-based renderer in the deacon crate (reuse the styling
  idiom from `crates/deacon/src/ui/spinner.rs`) driven by `BuildProgress` (B1). In Compact
  mode the streaming sink (B2) feeds `push_line` then repaints the MultiProgress; on
  non-zero exit, print `failing_step_log()` and return an error carrying that trim. In
  Plain mode the sink writes lines to stderr. Inherit mode uses no sink.

### B4 — Tests, nextest wiring, docs

- Docker integration test (new binary under `crates/deacon/tests/`, e.g.
  `integration_build_output.rs`): assert a feature build in Compact mode succeeds and that
  a forced failure prints a trimmed failing-step message (not the whole log). Follow the
  marker-assert convention (`docker run <tag> cat <marker>`), not just JSON outcome
  (per `integration_compose_features_build.rs`).
- Add the new binary to **all** nextest profiles in `.config/nextest.toml` (docker group +
  `dev-fast` default-filter exclusion + `dev-fast` override), per CLAUDE.md. Group:
  `docker-slow-shared` (mirrors `integration_build`).
- Update `README.md` build/logging sections and `--help` (the verbosity flags from #253
  plus the new build-output behavior).

### PR structure (Part 1)

Staged, each independently CI-verifiable (user has no preference; this keeps reviews small):
1. **B1** — parser module + fixtures + unit tests (no Docker).
2. **B2** — streaming `run_build_with_retry` + `--iidfile` across all build arg builders
   (retire stdout ID parsing). Docker-verified.
3. **B3 + B4** — `BuildOutputMode` resolution/threading, MultiProgress renderer, failure
   trim, integration tests, docs. Docker-verified end-to-end on `hola`.

---

## Part 2 (optional): `ai-clis` feature defensiveness

Once PR #252 is merged and a new deacon is in use, `ai-clis` works unchanged. Independently
of deacon, make the feature robust (also correct against any CLI) by sourcing nvm before
using `npm` in `/Users/paul/GitHub/devcontainer-features/src/ai-clis/install.sh` (before
line 78, the Gemini CLI step):

```sh
export NVM_DIR="/usr/local/share/nvm"
[ -s "$NVM_DIR/nvm.sh" ] && . "$NVM_DIR/nvm.sh"
```

This is belt-and-suspenders, not required for the deacon fix. Skip if you prefer the CLI
to be solely responsible.

---

## Execution bookkeeping

- After each code change: `cargo fmt --all && cargo fmt --all -- --check`,
  `cargo clippy --all-targets --all-features -- -D warnings`.
- Keep PR titles Conventional-Commits (`feat`/`fix`/`refactor`; not `test`/`style`).

---

## Verification

### A. containerEnv fix (#252) — the headline bug
1. Build the branch: `cargo build --release`.
2. `cd /Users/paul/GitHub/hola && /path/to/target/release/deacon up` (Docker available).
3. **Expect:** the `ai-clis` feature step reaches "Installing Gemini CLI…" and
   `npm install -g @google/gemini-cli` **succeeds** (previously exit 127). Container comes
   up. Optionally `deacon exec -- bash -lc 'command -v npm && gemini --version'`.
4. Inspect the generated Dockerfile (or unit test `test_container_env_emitted_as_env_between_features`)
   to confirm `ENV PATH="/usr/local/share/nvm/current/bin:${PATH}"` appears after the node
   step and before `ai-clis`.

### B. Verbosity (#253)
- `deacon up` (no flag) → only warnings/errors on stderr; no INFO chatter.
- `deacon -v up` → INFO visible; `-vv` → debug. `deacon -q ...` → errors only.
- `DEACON_LOG=deacon=trace deacon up` → env wins over `-v/-q`.
- `cargo test -p deacon resolve_log_level` and the clap parse tests pass.

### C. Build output (Part 1)
- Default `deacon up` on `hola` → compact one-line-per-feature progress; no apt/dpkg
  firehose. Force a failure (e.g. a feature that exits non-zero) → only the failing step's
  tail is shown.
- `deacon -v up` on a terminal → BuildKit's native collapsing UI (inherited).
- `deacon --log-format json up` and non-TTY (piped) → plain streamed lines on stderr,
  stdout JSON stays pure (cross-check `json_output_purity.rs`).
- `cargo test -p deacon build_progress` (parser unit tests) pass.
- `make test-nextest` green (including the new `integration_build_output` binary and the
  existing `integration_build*` / `integration_compose_features_build` suites, confirming
  the `--iidfile` switch preserved image tags).
