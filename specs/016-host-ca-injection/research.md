# Research: Corporate CA (Host Trust Store) Support

**Feature**: 016-host-ca-injection
**Date**: 2026-06-11
**Input**: spec.md (with Clarifications session 2026-06-11)

This document resolves the open technical decisions from the spec's Technical Context and the
"evaluate during planning" note about the host trust-store mechanism. Each decision records
what was chosen, why, and the alternatives rejected.

---

## Decision 1: Host trust-store mechanism — `rustls-native-certs` (enumeration), not `rustls-platform-verifier`

**Decision**: Use `rustls-native-certs` as the single source for host-trusted roots, for **both**
capabilities:
- **Req 1 (deacon's own client)**: keep reqwest's `rustls-tls` feature (webpki public roots as the
  base), and additionally enumerate the host roots via `rustls-native-certs` and add each as a root
  certificate (`reqwest::Certificate::from_der` → `add_root_certificate`). `DEACON_CUSTOM_CA_BUNDLE`
  stays additive on top. The trust set becomes the **union** of webpki public roots + host roots +
  custom bundle.
- **Req 2 (discovery)**: enumerate the same host roots, parse each with `x509-parser`, keep only
  `CA:TRUE` certs, and subtract the public set (see Decision 2). The remainder is the corporate set.

**Rationale**:
- `rustls-platform-verifier` is a **verifier**, not an **enumerator** — it answers "is this chain
  valid per the OS?" but does not hand back the list of trusted root certificates. Discovery
  (Req 2) fundamentally needs the actual certificate bytes to compute the corporate delta, so a
  verifier cannot satisfy it. Adopting one mechanism (`rustls-native-certs`) for both capabilities
  is simpler and keeps the trust source identical across them.
- `rustls-native-certs` is maintained by the rustls project, is pure-Rust on the dependency axis we
  care about, and does **not** pull `aws-lc-rs` (which would break pure-Rust cross-compilation — see
  the standing memory note on reqwest 0.13 / aws-lc-rs). It uses the platform store on macOS
  (Security framework) and Windows (schannel) and reads the standard CA files on Linux.
- Keeping reqwest on `0.12` + `rustls-tls` (ring crypto provider) is mandatory per the same memory
  note. We do **not** bump reqwest and do **not** switch crypto providers.

**Alternatives considered**:
- *reqwest `rustls-tls-native-roots` feature*: loads native certs but **replaces** the webpki base
  rather than unioning, and historically errors hard when zero native roots are found (bad on minimal
  CI images). Manual `add_root_certificate` over the webpki base is more robust and gives the union.
- *`rustls-platform-verifier` for Req 1 + `rustls-native-certs` for Req 2*: two mechanisms, two
  dependency trees, and a risk of divergence between "what deacon trusts" and "what deacon discovers."
  Rejected for complexity.
- *Reading `/etc/ssl/certs` directly*: non-portable; reinvents `rustls-native-certs`.

---

## Decision 2: Public-root subtraction is by **SubjectPublicKeyInfo (SPKI) SHA-256**, not full-cert fingerprint

**Decision**: Identify "is this a public Mozilla root?" by hashing the certificate's
SubjectPublicKeyInfo (SPKI) with SHA-256 and matching against the SPKI hashes of the bundled
`webpki-roots` trust anchors. The corporate set = host `CA:TRUE` certs whose SPKI hash is **not** in
the webpki public set.

**Rationale**:
- The bundled public set we ship (`webpki-roots`) exposes trust anchors as `(subject, SPKI, name
  constraints)` — it does **not** carry full DER certificates, so a full-certificate fingerprint
  cannot be computed for the public side. SPKI is the field common to both sides.
- SPKI identity is in practice **more** robust than full-cert fingerprint: it is stable across
  re-encodings and across a CA reissuing its root with the same key. Two certs with the same SPKI are
  the same root authority.
- The spec says "by certificate fingerprint"; this decision realizes that intent as an SPKI-based
  fingerprint and documents the realization. Logged subjects still come from the certificate's actual
  Subject DN, so observability is unaffected.

**Alternatives considered**:
- *Bundle the full Mozilla PEM set ourselves*: adds a large vendored asset that must be kept in sync
  and re-introduces a second public-root source distinct from what reqwest's `rustls-tls` already
  uses. Rejected.
- *Match by Subject DN string*: not collision-safe and trivially spoofable. Rejected.

---

## Decision 3: Streaming the bundle in — new `exec_with_stdin` runtime method (default-impl delegation)

**Decision**: Add a new method to the `Docker` trait (parent of `ContainerRuntime`) for exec with
streamed stdin bytes, e.g.:

```rust
async fn exec_with_stdin(
    &self,
    container_id: &str,
    command: &[String],
    stdin: &[u8],
    config: &ExecConfig,
) -> Result<ExecResult>;
```

Provide a **default implementation that returns a clear `unsupported`/`unimplemented` domain error**
(so existing mocks and delegating runtimes compile unchanged), and override it in `CliRuntime` (the
single impl shared by Docker and Podman) by spawning `docker exec -i … sh -c 'cat > <path>'` and
writing the bundle to the child's stdin via `tokio::process` async piping. The enum wrapper
`ContainerRuntimeImpl` forwards it to the inner runtime.

**Rationale**:
- The current `Docker::exec` (`crates/core/src/docker.rs:660`) has **no stdin-bytes parameter** — its
  `interactive` flag only inherits the parent process's fds. Streaming bundle content (FR-020, "no
  bind mount, must work with remote Docker contexts") requires feeding bytes over the exec channel,
  which today's signature cannot express.
- Default-impl delegation matches the established pattern (CLAUDE.md "Misc durable patterns"; mirrors
  `exec_with_line_prefix`). Mocks and the Podman path need no change unless they exercise injection.
- A bind mount is explicitly disallowed by FR-020 because host paths may not exist on a remote Docker
  daemon. `docker exec -i` streams through the daemon, so it works for remote contexts.

**Alternatives considered**:
- *Overload `exec` with an `Option<Vec<u8>>` stdin*: churns every call site and every mock signature.
  A separate method with a default impl is lower blast-radius. Rejected.
- *`docker cp`*: also requires a host file and does not stream over the exec channel; weaker for
  remote daemons. Rejected.

---

## Decision 4: In-container install is a single idempotent shell script; distro detected in-container

**Decision**: After streaming the bundle to a fixed staging path, run one POSIX `sh -c` script that:
1. detects the distro family by sourcing `/etc/os-release` (`$ID`/`$ID_LIKE`),
2. installs into the system store with the family-appropriate tool:
   - Debian/Ubuntu → copy to `/usr/local/share/ca-certificates/deacon-host-ca.crt` (split per cert) +
     `update-ca-certificates`,
   - RHEL/Fedora → copy to `/etc/pki/ca-trust/source/anchors/` + `update-ca-trust extract`,
   - Alpine → append/copy to `/usr/local/share/ca-certificates/` + `update-ca-certificates` (Alpine
     ships it via the `ca-certificates` package) or append to the bundle if the tool is absent,
3. exits with a distinct sentinel code/string for "unsupported distro" and for "not root / no perms",
   which deacon maps to the env-var-only fallback (FR-022) with a clear warning.

The canonical installed bundle path (used by the synthesized env vars, FR-023) is a single fixed
location, e.g. `/usr/local/share/deacon/host-ca.crt` (PEM concatenation of the corporate set), written
regardless of whether the system-store update succeeds — so env-var-only fallback still has a real file
to point at.

**Rationale**:
- There is **no in-container distro detection today** (the only `/etc/os-release` reader is host-side
  in `doctor.rs:240`). Probing in-container via the exec we already need keeps it to one round trip.
- A single self-contained script minimizes exec round-trips and keeps the "no silent fallback"
  contract explicit (each degraded path has a sentinel the Rust side turns into a specific warning).
- Writing the canonical PEM file unconditionally means the six CA env vars are always valid when
  injection is enabled, even on unsupported distros / non-root (the env-var-only path).

**Alternatives considered**:
- *Multiple exec calls (detect, then install)*: more round trips, more partial-failure states.
- *Rust-side distro inference from the image name*: unreliable (image tags lie); in-container probe is
  authoritative.

---

## Decision 5: Build-time injection via a dedicated named build context + RUN step before features

**Decision**: When injection is enabled and deacon generates the feature-layering Dockerfile, write the
corporate PEM bundle into the existing feature build-context staging dir and emit one deterministic
`RUN` step **immediately after** `RUN mkdir -p /tmp/dev-container-features`
(`dockerfile_generator.rs:136` and `:190`) and **before** the first feature `install.sh` RUN-mount.
The step mounts the bundle from a named build context (mirroring
`dev_containers_feature_content_source`, e.g. `deacon_ca_source`) and runs the same idempotent
install script as runtime (Decision 4) inside the build.

Determinism (FR-017): the emitted Dockerfile text is fixed for a given CA set; the bundle file content
is the corporate certs concatenated in a **stable sorted order** (sorted by SPKI hash). Same image +
same CA set → byte-identical step → cache hit. When certs rotate, the bundle bytes change and the layer
correctly rebuilds (no long-lived cache, per FR-006).

**Rationale**:
- `crates/core/src/dockerfile_generator.rs` already emits BuildKit `RUN --mount=type=bind,from=<named
  context>` lines and the caller already passes `--build-context
  dev_containers_feature_content_source=<dir>`; a sibling CA context is the lowest-friction, in-pattern
  insertion and needs no change to user-authored Dockerfiles (FR-018).
- Placing it before the feature loop guarantees feature `install.sh` network calls see the trusted CA.
- For config shapes with **no** generated Dockerfile (image-only without features, compose,
  user-authored Dockerfile), build-time injection is **skipped with a log line** (clarification 3 /
  FR-018a); runtime injection covers them.

**Alternatives considered**:
- *Rewriting the user's Dockerfile to COPY the cert*: forbidden by FR-018. Rejected.
- *`ARG`-passing the PEM inline*: args are size-limited and leak into image history. A mounted file is
  cleaner. Rejected.

---

## Decision 6: User-level settings file (read-only in this feature; `deacon settings` command deferred to #198)

**Decision**: Introduce `{user_data_folder}/settings.json` (sibling of `trusted_workspaces.json`,
resolved by the same logic in `trust.rs:124`), a small serde struct
`{ "hostCa": "auto" | "<absolute path>" }` with unknown-field tolerance for forward compatibility.
In **this** feature deacon only **reads** the file (load + tolerate-missing + tolerate-unknown-keys).
The machine owner provisions/hand-edits it, or uses the `--inject-host-ca` flag / `DEACON_INJECT_HOST_CA`
env var per-invocation. A `deacon settings get/set` write command (atomic temp-file + `fs::rename` per
`cache/disk.rs:116`, user-data folder only) is **deferred** and tracked in **issue #198**.

**Rationale**:
- The user data folder already persists machine-level state and honors `--user-data-folder`; reusing it
  keeps one location for machine config. Reading is all `016` needs to resolve `hostCa`.
- Deferring the write CLI keeps this feature scoped to the CA capability; the flag and env var already
  give the machine owner usable activation paths without hand-editing JSON. The `Settings` struct and
  the atomic-write helper reuse land naturally with the command in #198.

**Alternatives considered**:
- *Ship `settings get/set` now (the original clarification-2 answer)*: revised at the user's request to
  reduce `016` scope; the command is consumer-side machine config and is tracked for follow-up in #198.
- *Store under a new top-level dir*: fragments machine state. Rejected.

---

## Decision 7: Activation resolution helper (CLI > env > settings) in core

**Decision**: A single core helper resolves the activation decision:
`resolve_host_ca_activation(cli: Option<Option<String>>, env: Option<String>, settings: &Settings) ->
HostCaActivation` where `HostCaActivation ∈ { Off, Auto, ExplicitPath(PathBuf) }`. Precedence: CLI flag
(present, with optional value) > `DEACON_INJECT_HOST_CA` env > `settings.hostCa` > `Off`. A present-but-
valueless CLI flag and an env/settings value of `"auto"` map to `Auto`; any other non-empty value is a
path (validated to exist + parse as PEM at use, FR-005/FR-009/edge cases). This helper is **never** fed
any workspace-sourced input (FR-015).

**Rationale**:
- Centralizing precedence prevents the per-subcommand drift the constitution forbids (Principle VIII)
  and gives one place to unit-test the precedence matrix (spec test "settings precedence").
- Mirrors the existing `core::trust::resolve_policy` shape (flags → policy enum), keeping the codebase
  consistent.

**Alternatives considered**:
- *Resolve precedence inline in `up` and `build`*: duplicates logic across subcommands; rejected.

---

## Decision 8: New dependencies (minimal, pinned, ring-compatible)

**Decision**: Add to `crates/core/Cargo.toml`:
- `rustls-native-certs` — host root enumeration (Decisions 1, 2).
- `x509-parser` — parse DER certs for BasicConstraints (`CA:TRUE`), Subject DN, and SPKI bytes.
- `sha2` — SHA-256 of SPKI for the public-set subtraction (verify if already present transitively;
  declare explicitly if used directly).
Keep `reqwest = { version = "0.12", default-features = false, features = ["json", "rustls-tls"] }`
unchanged. Do **not** add `aws-lc-rs` or bump reqwest.

**Rationale**: Smallest set that satisfies enumeration + parsing + fingerprinting while preserving the
pure-Rust/ring TLS stack. All three are widely used and actively maintained.

**Alternatives considered**:
- *`openssl` for parsing*: pulls a C dependency and breaks the pure-Rust posture. Rejected.

---

## Resolved unknowns summary

| Spec unknown | Resolution |
|---|---|
| rustls-platform-verifier vs rustls-native-certs | `rustls-native-certs` for both client trust + discovery (Decision 1) |
| How to subtract the public root set | SPKI SHA-256 against bundled `webpki-roots` (Decision 2) |
| How to stream the bundle with no bind mount | new `exec_with_stdin` runtime method, default-impl delegation (Decision 3) |
| In-container distro detection (none exists today) | in-container `/etc/os-release` probe inside one idempotent install script (Decision 4) |
| Build-context delivery of the PEM | dedicated named build context + RUN before feature layers (Decision 5) |
| Settings file location/format/writer | `{user_data_folder}/settings.json` + `deacon settings` atomic write (Decision 6) |
| Activation precedence | core helper, CLI > env > settings (Decision 7) |
