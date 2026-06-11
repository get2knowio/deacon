# Implementation Plan: Corporate CA (Host Trust Store) Support

**Branch**: `016-host-ca-injection` | **Date**: 2026-06-11 | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification from `/specs/016-host-ca-injection/spec.md`

## Summary

Make dev containers "just work" on corporate machines behind a TLS-intercepting proxy via two
capabilities shipped together: (1) **always-on** — deacon's own HTTPS client trusts the host OS trust
store (union of webpki public roots + host roots via `rustls-native-certs`, plus the existing additive
`DEACON_CUSTOM_CA_BUNDLE`); and (2) **opt-in, machine-side** — auto-discover the corporate root CA delta
on the host and inject it into the container at build time (a deterministic `RUN` step in the
deacon-generated feature-layering Dockerfile, before any feature `install.sh`) and at runtime (streamed
over a new `exec_with_stdin` runtime method, installed into the distro trust store before any lifecycle
hook, plus six synthesized CA env vars). Activation resolves CLI flag > env var > `settings.json`, never
from workspace config. Default behavior with the feature unconfigured is bit-for-bit unchanged.

## Technical Context

**Language/Version**: Rust, Edition 2024, MSRV 1.95 (`unsafe_code = "deny"` workspace-wide)
**Primary Dependencies**: `reqwest` 0.12 (rustls-tls / ring — **unchanged**), `rustls-native-certs` (new,
host-root enumeration), `x509-parser` (new, DER parse / BasicConstraints / SPKI), `sha2` (SPKI
fingerprint), `webpki-roots` (existing transitive — public-root set), `tokio` (async exec streaming),
`clap`, `serde`/`serde_json`, `tracing`, `thiserror`/`anyhow`, `directories-next` (user-data folder)
**Storage**: `{user_data_folder}/settings.json` (read-only in this feature, sibling of
`trusted_workspaces.json`; write command deferred to #198); in-container PEM at
`/usr/local/share/deacon/host-ca.crt`; container labels for reconnect re-apply. No long-lived cert
cache (discovery re-runs every invocation).
**Testing**: `cargo nextest` (unit + docker integration), doctests; distro matrix
debian:bookworm-slim / RHEL-family / alpine; examples canary with `exec.sh`
**Target Platform**: Linux/macOS/Windows host; Unix-like container guests for in-container install
**Project Type**: CLI (Rust workspace: `crates/deacon` binary + `crates/core` library)
**Performance Goals**: Discovery adds one host-store enumeration per `up`/`build` (cached OS read,
negligible); runtime injection adds one `exec` round trip before lifecycle. No regression to the
unconfigured path.
**Constraints**: No silent fallbacks; no panics in runtime paths; no blocking IO in async; stay on
reqwest 0.12 + ring (no aws-lc-rs); do not rewrite user-authored Dockerfiles; additive output only.
**Scale/Scope**: ~8 task groups; touches `oci/client.rs`, a new `host_ca` core module, `docker.rs`
(runtime trait + CliRuntime), `dockerfile_generator.rs`, `up`/`build`/`exec`/`run-user-commands`,
`container.rs` labels, a read-only core `settings.rs` module, README + SECURITY.md, examples.
(`deacon settings get/set` write command deferred to issue #198.)

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

| Principle | Status | Notes |
|---|---|---|
| I. Spec-Parity as Source of Truth | ✅ PASS (documented extension) | This is a deliberate extension **beyond** containers.dev, exactly like the workspace-trust gate (`trust.rs`, SECURITY.md). It changes **no** spec-defined behavior; the unconfigured path is byte-for-byte identical (FR-029). Documented in SECURITY.md per FR-031. |
| II. Consumer-Only Scope | ✅ PASS | Touches consumer commands `up`/`build`/`exec`/`run-user-commands`. Settings file is read-only here; the `deacon settings` write command (consumer-side **machine** configuration, not feature authoring) is deferred to issue #198. |
| III. Keep the Build Green | ✅ PASS | fmt+clippy+fast loop each change; full `make test-nextest` + distro-matrix docker tests before PR. |
| IV. No Silent Fallbacks | ✅ PASS | Every degraded path (unsupported distro, non-root, unreadable bundle, discovery failure, zero certs) emits a distinct warning/error (FR-008/009/022/030). CLI value sets validated at parse. |
| V. Idiomatic, Safe Rust | ✅ PASS | `unsafe_code=deny` honored (no new unsafe; new deps are safe-API). thiserror in core, anyhow at boundary. New `exec_with_stdin` uses default-impl delegation. New core module `host_ca/` keeps boundaries small. Env vars as constants. |
| VI. Observability & Output Contracts | ✅ PASS | `ca.discover`/`ca.inject` spans; logs→stderr, results→stdout; `injectedCaSubjects` additive and omitted when off. |
| VII. Testing Completeness | ✅ PASS | Unit (discovery diff, precedence, PEM errors), docker integration (runtime present-in-store, ordering, build-time), distro matrix, adversarial workspace test, canary. Nextest groups registered in all profile spots. |
| VIII. Subcommand Consistency & Shared Abstractions | ✅ PASS | Reuses `ContainerIdentity::labels()`, `resolve_env_and_user`, the atomic-write helper, the `ContainerRuntime` trait, and the `resolve_policy`-style activation helper. One precedence helper consumed by `up`+`build`. |
| IX. Executable & Self-Verifying Examples | ✅ PASS | New `examples/up/host-ca/` with `exec.sh`+README in lockstep, pinned images, full cleanup; registered in `examples/up` aggregator + `CANARY_STATUS.md`. |

**Result**: No violations. No entries in Complexity Tracking.

## Project Structure

### Documentation (this feature)

```text
specs/016-host-ca-injection/
├── plan.md              # This file
├── spec.md              # Feature spec (+ Clarifications)
├── research.md          # Phase 0 — 8 decisions
├── data-model.md        # Phase 1 — entities
├── quickstart.md        # Phase 1 — machine-owner workflow
├── contracts/
│   └── cli.md           # Phase 1 — CLI/env/settings/runtime/script contracts
└── tasks.md             # Phase 2 — created by /speckit.tasks (NOT here)
```

### Source Code (repository root)

```text
crates/core/src/
├── oci/client.rs                 # MODIFY: union host roots (rustls-native-certs) into ReqwestClient; keep DEACON_CUSTOM_CA_BUNDLE additive
├── host_ca/                      # NEW module: corporate CA capability
│   ├── mod.rs                    #   public surface (activation, discovery, bundle, inject orchestration)
│   ├── discover.rs               #   enumerate host roots, parse (x509-parser), subtract public set by SPKI sha256
│   ├── activation.rs             #   resolve_host_ca_activation (CLI > env > settings)
│   ├── inject.rs                 #   runtime injection orchestration + install script + InjectionOutcome
│   └── env.rs                    #   the six CA env vars + insert-if-absent merge
├── settings.rs                   # NEW: Settings struct + read-only load() + settings_path() (write/`deacon settings` deferred to #198)
├── trust.rs                      # REFERENCE: user-data-folder resolution reused by settings_path()
├── docker.rs                     # MODIFY: Docker::exec_with_stdin (default-impl) + CliRuntime override
├── runtime.rs                    # MODIFY: ContainerRuntimeImpl forwards exec_with_stdin
├── dockerfile_generator.rs       # MODIFY: emit CA RUN step after mkdir, before feature loop (both generate paths)
├── container.rs                  # MODIFY: add hostCaBundlePath/hostCaSubjects labels (post-identity, informational)
└── cache/disk.rs                 # REFERENCE: atomic write pattern (reused when settings write lands, #198)

crates/deacon/src/commands/
├── up/{mod.rs,container.rs,compose.rs,features_build.rs}  # MODIFY: resolve activation, discover, inject before hooks, synth env, write labels, build-context
├── build/mod.rs                  # MODIFY: resolve activation, pass CA into generated Dockerfile + build context
├── exec.rs                       # MODIFY: read CA labels, re-apply env vars (no re-discovery)
└── run_user_commands.rs          # MODIFY: same label-read + env re-apply
# NOTE: `deacon settings get/set` command deferred to issue #198 (settings.json is read-only here)

crates/deacon/tests/              # NEW docker-gated integration binaries (distro matrix) + unit tests
examples/up/host-ca/             # NEW canary (devcontainer.json, exec.sh, README)
README.md, SECURITY.md            # MODIFY: document the extension + threat model (FR-031)
.config/nextest.toml              # MODIFY: register new docker test binaries in all profile spots
```

**Structure Decision**: Standard deacon workspace layout. New domain logic is isolated in a focused
`crates/core/src/host_ca/` module (Constitution V modular boundaries) plus a small `settings.rs`; the
binary crate wires activation/orchestration into existing command flows. No new crates. (The
`deacon settings get/set` subcommand is deferred to issue #198.)

## Phase 0 — Research

Complete. See [research.md](./research.md). All NEEDS CLARIFICATION resolved: host-store mechanism
(`rustls-native-certs`, Decision 1), public-set subtraction by SPKI sha256 (Decision 2), stdin streaming
via new `exec_with_stdin` (Decision 3), in-container distro probe + idempotent install script
(Decision 4), build-context delivery (Decision 5), read-only settings file — write command deferred to
issue #198 (Decision 6), activation precedence helper (Decision 7), dependency set (Decision 8).

## Phase 1 — Design & Contracts

Complete. Artifacts: [data-model.md](./data-model.md) (Settings, HostCaActivation, HostCertificate,
CorporateCaSet, InjectionOutcome, label + env additions), [contracts/cli.md](./contracts/cli.md)
(flag, env var, read-only settings file, JSON additions, `exec_with_stdin` trait contract, install-script exit
contract, spans, invariants), [quickstart.md](./quickstart.md).

## Phase 2 — Task planning approach (preview only; tasks.md created by /speckit.tasks)

Tasks will be grouped by user story for independent testability:
1. **US1 (P1)** — host-trust-store union in `ReqwestClient` + additive bundle preserved; unit tests for
   trust set; keep all HttpClient mocks consistent.
2. **Discovery core** — `host_ca::discover` + SPKI subtraction + unit tests with fixture certs (public
   excluded, corporate detected, non-CA leaf excluded) — shared by US2/US3.
3. **Activation + settings** (US4) — `settings.rs` (read-only load), `resolve_host_ca_activation`;
   precedence + PEM-validation unit tests; adversarial workspace-cannot-enable test. (The
   `deacon settings get/set` write command is deferred to issue #198.)
4. **Runtime injection** (US2) — `exec_with_stdin` (trait default + CliRuntime + enum forward), install
   script, inject orchestration before lifecycle hooks (single-container + compose), synth env vars,
   label write; docker integration verifying cert-in-store + ordering.
5. **Reconnect** — label read + env re-apply in `exec`/`run-user-commands`; integration test.
6. **Build-time injection** (US3) — `dockerfile_generator` RUN step + build context wiring in
   `build`/`features_build`; docker test asserting cert in feature-extended image (`docker run … cat`).
7. **Observability/JSON** — spans + `injectedCaSubjects`; unconfigured-path byte-stability test.
8. **Docs + examples + nextest** — README/SECURITY.md, `examples/up/host-ca/`, nextest registration in
   all profile spots; full `make test-nextest`.

## Complexity Tracking

No constitution violations; no justifications required.
