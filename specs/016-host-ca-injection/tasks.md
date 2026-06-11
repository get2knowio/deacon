---
description: "Task list for 016-host-ca-injection"
---

# Tasks: Corporate CA (Host Trust Store) Support

**Input**: Design documents from `/specs/016-host-ca-injection/`
**Prerequisites**: plan.md, spec.md, research.md, data-model.md, contracts/cli.md, quickstart.md

**Tests**: INCLUDED — the spec's Requirement 8 ("Testing — spec-mandated") and Constitution VII make
the listed unit/integration/distro-matrix/canary tests acceptance criteria, not optional.

**Organization**: Tasks are grouped by user story. Foundational (Phase 2) is the shared CA engine that
US2/US3/US4 build on; US1 builds only on the host-root enumeration helper.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependencies on incomplete tasks)
- **[Story]**: US1, US2, US3, US4 (maps to spec.md user stories)

## Path Conventions

Rust workspace: `crates/core/src/` (library/domain), `crates/deacon/src/commands/` (CLI),
`crates/deacon/tests/` (integration), `examples/` (canaries).

---

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: Dependencies and module skeletons

- [X] T001 Add `rustls-native-certs`, `x509-parser`, `sha2` to `crates/core/Cargo.toml` (workspace-hoist if shared); confirm `reqwest` stays `{ version = "0.12", default-features = false, features = ["json", "rustls-tls"] }` (no aws-lc-rs, no bump); `cargo build -p deacon-core`
- [X] T002 [P] Create `crates/core/src/host_ca/` module skeleton (`mod.rs`, `discover.rs`, `activation.rs`, `inject.rs`, `env.rs`) and register `pub mod host_ca;` in `crates/core/src/lib.rs`
- [X] T003 [P] Create `crates/core/src/settings.rs` skeleton and register `pub mod settings;` in `crates/core/src/lib.rs`

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: The shared CA engine + activation plumbing. US2/US3/US4 cannot function until this is done.

**⚠️ CRITICAL**: No user-story work begins until this phase is complete.

- [X] T004 [P] Implement `enumerate_host_roots() -> Result<Vec<CertificateDer>>` in `crates/core/src/host_ca/discover.rs` using `rustls-native-certs`; surface enumeration failure as an actionable error (FR-009); shared by US1 + discovery
- [X] T005 [P] Define env-var name constants (`DEACON_INJECT_HOST_CA` and the six CA vars `SSL_CERT_FILE`/`NODE_EXTRA_CA_CERTS`/`REQUESTS_CA_BUNDLE`/`PIP_CERT`/`GIT_SSL_CAINFO`/`CURL_CA_BUNDLE`) + canonical in-container bundle path const `/usr/local/share/deacon/host-ca.crt` in `crates/core/src/host_ca/env.rs` (Constitution V env-var-constants)
- [X] T006 Implement cert parsing into `HostCertificate { der, subject, spki_sha256, is_ca }` in `crates/core/src/host_ca/discover.rs` using `x509-parser` (Subject DN, BasicConstraints CA:TRUE) + `sha2` (SHA-256 over SPKI) — depends T004
- [X] T007 Implement `discover_corporate_set(activation) -> Result<CorporateCaSet>` in `crates/core/src/host_ca/discover.rs`: auto mode subtracts `webpki-roots` SPKI hashes and keeps only CA:TRUE; explicit-path mode reads+validates the PEM; deterministic sort by `spki_sha256`; build `pem_bundle` + `subjects`; empty set is OK (FR-008) — depends T006
- [X] T008 [P] Implement `Settings { host_ca }` + read-only `load(user_data_folder) -> Result<Settings>` (tolerate missing file + unknown keys) + `settings_path()` reusing the user-data-folder resolution from `crates/core/src/trust.rs`, in `crates/core/src/settings.rs`
- [X] T009 Implement `HostCaActivation { Off, Auto, ExplicitPath }` + `resolve_host_ca_activation(cli, env, settings)` precedence helper (CLI > env > settings > Off; never workspace-sourced) in `crates/core/src/host_ca/activation.rs` — depends T008
- [X] T010 [P] Implement install-script builder (POSIX `sh -c`: write canonical bundle always, detect distro via `/etc/os-release`, run debian/rhel/alpine updater, sentinel exits 10=unsupported 11=no-root) + `InjectionOutcome { mode, bundle_path, injected_subjects, warning }` + exit-code→outcome mapping in `crates/core/src/host_ca/inject.rs` — depends T005
- [X] T011 [P] Implement synthesized-CA-env helper (insert-if-absent over user `containerEnv`/`remoteEnv`, so user values win — FR-024) in `crates/core/src/host_ca/env.rs` — depends T005
- [X] T012 [P] Add `devcontainer.deacon.hostCaBundlePath` / `devcontainer.deacon.hostCaSubjects` label key constants + optional fields on `ContainerIdentity` and informational emission in `labels()` (MUST NOT feed `workspace_hash`/`config_hash`) in `crates/core/src/container.rs`
- [ ] T013 Add `--inject-host-ca [PATH]` flag (`num_args = 0..=1`, `default_missing_value = "auto"`) to `up` and `build` arg structs, and resolve `HostCaActivation` at the CLI tier (flag → `DEACON_INJECT_HOST_CA` → `Settings::load`) feeding T009, wired in `crates/deacon/src/commands/up/args.rs` (or up/mod.rs) and `crates/deacon/src/commands/build/mod.rs` — depends T009

**Checkpoint**: CA engine + activation resolution exist; user stories can begin.

---

## Phase 3: User Story 1 - Deacon's own pulls succeed behind a corporate proxy (Priority: P1) 🎯 MVP

**Goal**: Deacon's own HTTPS client trusts the host OS trust store (union with webpki public roots),
with `DEACON_CUSTOM_CA_BUNDLE` still additive.

**Independent Test**: A corporate-style CA in the host store lets deacon's feature pulls validate with
no `DEACON_CUSTOM_CA_BUNDLE`; removing it from the host store makes validation fail.

### Tests for User Story 1

- [X] T014 [P] [US1] Unit test in `crates/core/src/oci/client.rs` (mod tests): `ReqwestClient::new` builds with host roots added (count > 0 when present) and a `DEACON_CUSTOM_CA_BUNDLE` temp PEM is still added additively (use `temp-env` for the env var, not `set_var`)

### Implementation for User Story 1

- [X] T015 [US1] Add a shared `apply_host_and_custom_roots(builder)` helper in `crates/core/src/oci/client.rs` that calls `enumerate_host_roots()` (T004) and `add_root_certificate(reqwest::Certificate::from_der(..))` for each over the webpki base AND applies the existing `DEACON_CUSTOM_CA_BUNDLE` additively; invoke it from ALL constructors (`with_timeout`, `with_no_timeout`, `with_auth_config`) so trust is consistent everywhere; `debug!` the host-root count — depends T004
- [X] T016 [US1] Confirm no `HttpClient` trait change (constructor-only) and run `cargo nextest run -p deacon-core oci` to verify all mocks (`MockHttpClient`, `AuthMockHttpClient`, `SlowMockHttpClient`, `FailingMockClient`, `AlwaysFailingClient`) compile/pass unchanged

**Checkpoint**: US1 fully functional and independently testable — deacon's own pulls trust the host store.

---

## Phase 4: User Story 2 - Containers "just work" at runtime (Priority: P1)

**Goal**: When injection is enabled, discover corporate CAs and inject them into the running container
(system store + the six CA env vars) BEFORE any lifecycle hook; persist for later exec sessions.

**Independent Test**: Enable injection, `up` a debian container whose `postCreateCommand` reads the
injected cert (or hits a proxied endpoint) — it succeeds; `exec cat <bundle path>` shows the cert; a
subsequent `exec` shows the CA env vars (read from labels, no re-discovery).

### Tests for User Story 2

- [X] T017 [P] [US2] Unit test discovery diff in `crates/core/src/host_ca/discover.rs` tests with fixture certs: public root excluded (SPKI match), corporate CA detected, non-CA leaf (CA:FALSE) excluded, AND the all-public-roots case yields an empty corporate set (the "zero certs → proceed without injection, not an error" path, FR-008) (Requirement 8 unit)
- [ ] T018 [P] [US2] Docker integration test (debian:bookworm-slim) in new `crates/deacon/tests/integration_host_ca_runtime.rs`: after `up --inject-host-ca <fixture.pem>`, `exec` shows the cert in the system store (`docker exec … cat` marker pattern, not just JSON)
- [ ] T019 [P] [US2] Docker integration test in `crates/deacon/tests/integration_host_ca_runtime.rs`: a `postCreateCommand` that reads the injected cert succeeds — proves inject ordering (start → inject → hooks)
- [ ] T020 [P] [US2] Docker distro-matrix integration tests in `crates/deacon/tests/integration_host_ca_runtime.rs` (pinned `rockylinux:9` RHEL-family + pinned `alpine:3.20`) asserting cert-in-store via the distro-appropriate location (Constitution Fixture Hygiene — pin versions)
- [ ] T021 [P] [US2] Integration test: unsupported distro (e.g. scratch/busybox) and non-root user each → env-var-only fallback with a clear warning and NO `up` abort (FR-022)

### Implementation for User Story 2

- [X] T022 [US2] Add `exec_with_stdin(container_id, command, stdin, config)` to the `Docker` trait in `crates/core/src/docker.rs` with a default impl returning an `unsupported` domain error (mocks/runtimes compile unchanged)
- [X] T023 [US2] Implement `CliRuntime::exec_with_stdin` (spawn `docker exec -i … sh -c 'cat > <path>'`, write bytes via `tokio::process` async stdin pipe) in `crates/core/src/docker.rs` — depends T022
- [X] T024 [US2] Forward `exec_with_stdin` in the `ContainerRuntimeImpl` enum wrapper in `crates/core/src/runtime.rs` — depends T022
- [X] T025 [US2] Implement runtime inject orchestration `inject_runtime(runtime, container_id, corporate_set) -> InjectionOutcome` in `crates/core/src/host_ca/inject.rs`: stream `pem_bundle` via `exec_with_stdin`, run install script, map exit → outcome, info-log every injected subject under a `ca.inject` span — depends T010, T023, T007
- [ ] T026 [US2] Wire injection into the single-container `up` flow AFTER `start_container` and BEFORE the first lifecycle hook in `crates/deacon/src/commands/up/{mod.rs,container.rs}`; skip cleanly when activation is `Off` — depends T025, T013
- [ ] T027 [US2] Wire injection into the compose `up` path before lifecycle in `crates/deacon/src/commands/up/compose.rs` — depends T025, T013
- [ ] T028 [US2] Synthesize the six CA env vars (insert-if-absent, T011) into `container_env` at create in `crates/deacon/src/commands/up/{mod.rs,compose.rs}` for both single-container and compose — depends T011, T026, T027
- [ ] T029 [US2] Populate `hostCaBundlePath`/`hostCaSubjects` labels at create from `InjectionOutcome` in `crates/deacon/src/commands/up/mod.rs` — depends T012, T025
- [ ] T030 [US2] Emit the `ca.discover` span around discovery in `crates/deacon/src/commands/up/mod.rs` (fields: host_total, corporate_count, mode) — depends T007
- [ ] T031 [P] [US2] Docker integration test: `up --inject-host-ca` then `exec` shows the six CA env vars sourced from labels (no re-discovery log) in `integration_host_ca_runtime.rs`
- [ ] T032 [US2] Read CA labels on reconnect and re-apply the six env vars (no re-discovery / no activation re-resolve) in `crates/deacon/src/commands/exec.rs` — depends T029, T011
- [ ] T033 [US2] Same label-read + env re-apply in `crates/deacon/src/commands/run_user_commands.rs` — depends T029, T011

**Checkpoint**: US2 works independently — containers get the CA before hooks; exec/run-user-commands inherit it.

---

## Phase 5: User Story 3 - Feature installs succeed during image build (Priority: P2)

**Goal**: When injection is enabled and deacon generates the feature-layering Dockerfile, install the CA
before any feature `install.sh` RUN layer, deterministically.

**Independent Test**: `build --inject-host-ca <fixture.pem>` on a config with a network-using feature →
build succeeds and `docker run <image> cat <cert path>` shows the cert.

### Tests for User Story 3

- [ ] T034 [P] [US3] Unit test in `crates/core/src/dockerfile_generator.rs` tests: the generated Dockerfile contains the CA-install RUN step after `RUN mkdir -p /tmp/dev-container-features` and before the first feature RUN-mount, and is byte-stable for the same CA set (FR-017)
- [ ] T035 [P] [US3] Test in `crates/deacon/tests/integration_host_ca_build.rs`: image-only (no features) and compose shapes SKIP build-time injection and emit the skip log line (FR-018a)
- [ ] T036 [P] [US3] Docker integration test in new `crates/deacon/tests/integration_host_ca_build.rs`: feature-extended image built with injection contains the cert (`docker run <tag> cat`), distro debian:bookworm-slim

### Implementation for User Story 3

- [ ] T037 [US3] Emit the deterministic CA-install RUN step (mount from a named build context, run the shared install script from T010) immediately after `RUN mkdir -p /tmp/dev-container-features` in BOTH `generate()` (:136) and `generate_install_stage_from()` (:190) in `crates/core/src/dockerfile_generator.rs` — depends T010
- [ ] T038 [US3] Stage the `pem_bundle` into the feature build context and pass `--build-context deacon_ca_source=<dir>` in `crates/deacon/src/commands/up/features_build.rs` (+ the build-args assembly); thread an `Option<CorporateCaSet>` parameter through `build_image_with_features` so BOTH callers (`up`'s feature build and `build`'s `apply_features_and_lockfile`) supply it — depends T037, T007
- [ ] T039 [US3] Resolve activation in BOTH the `up` feature-build path (`crates/deacon/src/commands/up/mod.rs`) and the `build` flow (`crates/deacon/src/commands/build/mod.rs`), pass the resolved `CorporateCaSet` into the generated-Dockerfile path (T038), and skip-with-log when no feature-layering Dockerfile is generated (FR-018a) — depends T013, T007, T037, T038

**Checkpoint**: US3 works independently — feature installs trust the CA at build time.

---

## Phase 6: User Story 4 - Machine owner controls activation with clear precedence (Priority: P2)

**Goal**: Activation resolves CLI > env > settings file, never from the workspace; invalid bundles fail
fast. (Resolution plumbing lands in Foundational T009/T013; this story delivers + verifies the guarantees.)

**Independent Test**: Set the capability in all three sources with conflicting values → flag wins, then
env, then settings; a setting in `devcontainer.json` has no effect; an unreadable/non-PEM explicit bundle
fails fast.

### Tests for User Story 4

- [X] T040 [P] [US4] Unit test of the precedence matrix (CLI > env > settings > Off; valueless flag & "auto" → Auto) in `crates/core/src/host_ca/activation.rs` tests (Requirement 8)
- [X] T041 [P] [US4] Unit test: missing settings file → Off; `hostCa:"auto"` → Auto; unknown keys tolerated; non-absolute path rejected at use, in `crates/core/src/settings.rs` tests
- [X] T042 [P] [US4] Unit test in `crates/core/src/host_ca/discover.rs` tests: unreadable and non-PEM explicit bundle each fail fast with a message naming the path + reason (Requirement 8; SC-008)
- [ ] T043 [P] [US4] Adversarial integration test: a `devcontainer.json` attempting to set host-CA injection has NO effect on trust/injection (SC-007)

### Implementation for User Story 4

- [X] T044 [US4] Validate the explicit bundle path (exists + parses as PEM) at use and fail fast with path + reason; reuse in both up & build activation paths in `crates/core/src/host_ca/discover.rs` (+ call sites) — depends T007
- [ ] T045 [US4] Add an explicit guard/assertion (and code comment) that no workspace-resident value is ever fed into `resolve_host_ca_activation` (FR-015), in `crates/deacon/src/commands/up/mod.rs` and `crates/deacon/src/commands/build/mod.rs` — depends T013

**Checkpoint**: Activation behavior is correct, bounded, and never workspace-driven.

---

## Phase 7: Polish & Cross-Cutting Concerns

- [ ] T046 [P] Add additive `injectedCaSubjects` array to the `up`/`build` JSON result (omitted when injection is Off or zero certs) in the result structs + serializers; assert unconfigured output is byte-stable (FR-028, FR-029, SC-005)
- [ ] T047 Register the new docker test binaries (`integration_host_ca_runtime`, `integration_host_ca_build`) in `.config/nextest.toml` in ALL profile spots (the `[profile.default]` override filter, the `[profile.dev-fast]` default-filter exclusion, and the `[profile.dev-fast]` override filter; add to `mvp-integration` if these gate PRs; verify the `full`/`ci` profiles also classify them since those execute docker tests) — mirror `run_user_commands_prebuild`
- [ ] T048 [P] Document the extension in `README.md` and `SECURITY.md`: opt-in machine-side CA injection, threat model (machine-owner only, never workspace-driven), and the manual ARG/COPY convention for user-authored Dockerfiles (FR-031, FR-018)
- [ ] T049 Create `examples/up/host-ca/` canary (`.devcontainer/devcontainer.json` pinned `debian:bookworm-slim` + a `postCreateCommand` that reads the injected cert, `exec.sh`, `README.md`) with full resource cleanup; register in `examples/up/exec.sh` aggregator, `examples/up/README.md` table, and `examples/CANARY_STATUS.md` (Constitution IX)
- [ ] T050 [P] Run `quickstart.md` end-to-end against a local fixture CA to validate the documented workflow
- [ ] T051 Full gate: `cargo fmt --all -- --check`, `cargo clippy --all-targets --all-features -- -D warnings`, `make test-nextest`; fix ALL failures (including any pre-existing/unrelated, per CLAUDE.md)

---

## Deferred Work

- [ ] T052 [Deferral] `deacon settings get/set` command — tracked in **issue #198**
  - **Decision**: research.md Decision 6 (settings file is read-only in this feature; the `Settings`
    struct + atomic-write helper reuse land with the command in #198)
  - **Rationale**: Keeps 016 scoped to the CA capability; the `--inject-host-ca` flag and
    `DEACON_INJECT_HOST_CA` env var already give the machine owner usable activation paths.
  - **Acceptance** (in #198): `deacon settings get <key>` / `set <key> <value>` for `hostCa`, atomic
    temp-file + rename write, user-data folder only (never workspace), unknown-key fail-fast, stdout/stderr
    contract honored.

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (Phase 1)**: no dependencies.
- **Foundational (Phase 2)**: depends on Setup; BLOCKS all user stories.
- **US1 (Phase 3, P1)**: depends only on T004 (host-root enumeration) — the leanest MVP.
- **US2 (Phase 4, P1)**: depends on Foundational (T007 discovery, T009/T013 activation, T010 script, T011 env, T012 labels).
- **US3 (Phase 5, P2)**: depends on Foundational (T007, T010, T013).
- **US4 (Phase 6, P2)**: depends on Foundational (T007, T009, T013).
- **Polish (Phase 7)**: depends on the user stories it touches (T046 after US2/US3; T047/T049 after US2/US3 tests exist).

### User Story Dependencies

- **US1** is fully independent (constructor-only change).
- **US2, US3, US4** are independent of each other; all consume the Foundational engine. US2 reconnect
  (T032/T033) depends on US2 label write (T029).

### Within Each Story

- Tests are written to fail first, then implementation makes them pass.
- Core (core crate) before wiring (commands), wiring before integration tests pass.

### Parallel Opportunities

- Setup: T002, T003 in parallel.
- Foundational: T004, T005, T008, T010, T011, T012 in parallel (different files); T006→T007 and T008→T009 are serial chains.
- Once Foundational completes, US1/US2/US3/US4 can proceed in parallel (different developers).
- All `[P]` test tasks within a story run in parallel.

---

## Parallel Example: Foundational

```bash
# Independent foundational leaves (different files):
Task: "T004 enumerate_host_roots in host_ca/discover.rs"
Task: "T005 env-var constants in host_ca/env.rs"
Task: "T008 Settings load in settings.rs"
Task: "T010 install-script builder in host_ca/inject.rs"
Task: "T012 CA labels in container.rs"
```

## Parallel Example: User Story 2 tests

```bash
Task: "T017 discovery-diff unit test"
Task: "T018 debian cert-in-store integration test"
Task: "T019 postCreate-reads-cert ordering test"
Task: "T020 RHEL+alpine distro matrix tests"
Task: "T021 unsupported-distro / non-root fallback test"
```

---

## Implementation Strategy

### MVP First

1. Phase 1 Setup → Phase 2 Foundational → Phase 3 **US1**.
2. **STOP & VALIDATE**: deacon's own pulls trust the host store (the always-on win). Shippable alone.

### Incremental Delivery

1. Foundational ready.
2. US1 (P1) → deacon's own calls work behind the proxy → ship.
3. US2 (P1) → containers "just work" at runtime → ship.
4. US3 (P2) → feature installs work at build time → ship.
5. US4 (P2) → activation precedence/boundary guarantees verified → ship.
6. Polish: JSON field, nextest registration, docs, canary, full gate.

### Notes

- `[P]` = different files, no incomplete-task dependency.
- Orchestration files (`up/mod.rs`, `build/mod.rs`) are touched by several phases — those tasks are NOT
  marked `[P]` against each other.
- Stay on reqwest 0.12 + ring (no aws-lc-rs). No `unsafe`. thiserror in core, anyhow at the boundary.
- Commit after each task or logical group; run `make test-nextest-fast` between groups, full gate at T051.
