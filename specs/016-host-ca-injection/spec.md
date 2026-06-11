# Feature Specification: Corporate CA (Host Trust Store) Support

**Feature Branch**: `016-host-ca-injection`  
**Created**: 2026-06-11  
**Status**: Draft  
**Input**: User description: "Add corporate CA (TLS-interception / MITM proxy, e.g. Zscaler) support to deacon so that dev containers 'just work' on corporate machines. Two capabilities, shipped together: (1) deacon's own network calls trust the host OS trust store, and (2) an opt-in, machine-level mechanism that auto-discovers corporate-installed root CAs on the host and injects them into dev containers at build time and runtime."

## Overview

On a corporate machine behind a TLS-intercepting proxy (Zscaler, Netskope, corporate
Squid, etc.), outbound HTTPS is re-signed by a corporate root CA that the host already
trusts but that bundled/public root sets do not. Today this breaks two things in deacon:
its own OCI feature/template pulls, and any network access inside dev containers (feature
`install.sh` layers, `postCreateCommand` running `npm`/`pip`, etc.).

This feature closes both gaps. The first capability — deacon's own calls trusting the
host OS trust store — is always on and requires no configuration. The second — discovering
the corporate CA on the host and injecting it into the dev container — is a deliberate,
opt-in, **machine-side** extension beyond the containers.dev specification, modeled on the
existing workspace-trust gate. It can **never** be activated or influenced by
workspace-resident configuration (`devcontainer.json` or any file in the workspace), because
trusting and injecting a CA is a privileged act that only the machine owner may authorize.

## Clarifications

### Session 2026-06-11

- Q: How do `exec`/`run-user-commands` obtain the synthesized CA env vars after `up`? → A: `up` persists the injected bundle path and subjects in the container's deacon labels at create time; `exec`/`run-user-commands` read them back on reconnect and re-apply the env vars, with no re-discovery and no re-resolution of activation.
- Q: Who writes the user-level settings file? → A: In this feature the settings file is read-only from deacon's perspective — the machine owner / IT hand-edits or provisions it (or uses the `--inject-host-ca` flag / `DEACON_INJECT_HOST_CA` env var per-invocation). A `deacon settings get/set` write command is deferred to a follow-up (tracked in issue #198); when it ships it will use the atomic temp-file + rename pattern.
- Q: Build-time injection for config shapes with no deacon-generated Dockerfile (image-only without features, compose, user-authored Dockerfile)? → A: Build-time injection applies ONLY when deacon generates the feature-layering Dockerfile. All other shapes rely on runtime injection (US2). Consistent with the no-silent principle, deacon logs when build-time injection is skipped because no Dockerfile is generated.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Deacon's own pulls succeed behind a corporate proxy (Priority: P1)

A developer on a corporate laptop behind a TLS-intercepting proxy runs `deacon up` on a
project that uses OCI features. The corporate root CA is installed in the host OS trust
store (as corporate IT requires for the browser to work). Deacon fetches the features and
templates over HTTPS without any certificate errors and without the developer setting any
environment variable.

**Why this priority**: This is the foundational, always-on capability. Without it, deacon
cannot even fetch the features it needs to build the container, so nothing downstream works.
It requires zero configuration and benefits every corporate user immediately.

**Independent Test**: Place a corporate-style CA in the host trust store, point deacon's
registry traffic through a re-signing proxy, and run a feature pull. It succeeds with no
`DEACON_CUSTOM_CA_BUNDLE` set. Removing the CA from the host store makes it fail — proving
the host store is the source of trust.

**Acceptance Scenarios**:

1. **Given** a corporate root CA installed in the host OS trust store and an intercepting
   proxy, **When** the developer runs `deacon up` with OCI features, **Then** feature/template
   pulls succeed with no certificate error and no extra configuration.
2. **Given** the same setup **and** a `DEACON_CUSTOM_CA_BUNDLE` pointing at an additional PEM,
   **When** deacon makes HTTPS calls, **Then** both the host store roots and the bundle roots
   are trusted (the bundle is additive, not a replacement).
3. **Given** no corporate proxy at all (ordinary public internet), **When** deacon pulls from
   a public registry, **Then** behavior is unchanged from today — public roots still validate.

---

### User Story 2 - Containers "just work" at runtime on a corporate machine (Priority: P1)

The machine owner enables host-CA injection once (via settings file, environment variable,
or CLI flag). From then on, when they run `deacon up`, deacon auto-discovers the corporate
root CA(s) on the host, installs them into the container's system trust store immediately
after the container starts and **before** any lifecycle hook runs, and sets the common
tool-specific CA environment variables. A `postCreateCommand` that runs `npm install` or
`pip install` behind the proxy now succeeds.

**Why this priority**: This is the headline value — "dev containers just work on corporate
machines." Lifecycle hooks that hit the network are extremely common and are the most visible
failure for corporate users.

**Independent Test**: Enable injection, run `up` on a container whose `postCreateCommand`
reads the installed cert (or performs a proxied network call). Exec into the container and
confirm the corporate cert is present in the system trust store; confirm the hook succeeded.
With injection disabled, the cert is absent and the proxied hook fails.

**Acceptance Scenarios**:

1. **Given** injection is enabled and a corporate CA exists on the host, **When** `deacon up`
   runs on a Debian/Ubuntu-based image, **Then** the CA is installed via the distro trust-store
   updater before `onCreateCommand`/`postCreateCommand`, and those hooks can make proxied HTTPS
   calls successfully.
2. **Given** injection is enabled, **When** the container starts, **Then** `SSL_CERT_FILE`,
   `NODE_EXTRA_CA_CERTS`, `REQUESTS_CA_BUNDLE`, `PIP_CERT`, `GIT_SSL_CAINFO`, and
   `CURL_CA_BUNDLE` are set to the installed bundle path for lifecycle hooks and `exec`.
3. **Given** injection is enabled **and** the user already set `NODE_EXTRA_CA_CERTS` in
   `containerEnv`/`remoteEnv`, **When** env is composed, **Then** the user's value wins for
   that variable; the others are still synthesized.
4. **Given** injection is enabled but the target image is an unsupported distro (no recognized
   trust-store updater) **or** the container user lacks root/sudo, **When** `up` runs, **Then**
   deacon emits a clear, actionable warning and falls back to env-var-only injection — it never
   silently skips and never aborts the `up`.
5. **Given** injection is enabled but host discovery yields zero corporate certs, **When** `up`
   runs, **Then** deacon logs that zero certs were found and proceeds with no injection (not an
   error).

---

### User Story 3 - Feature installs succeed during image build (Priority: P2)

A project layers OCI features whose `install.sh` pulls packages over HTTPS. With injection
enabled, deacon installs the corporate CA into the image's trust store as a build step
inserted **before** any feature `install.sh` layer in the deacon-generated feature-layering
Dockerfile, so those network calls succeed during `docker build`.

**Why this priority**: Feature installs that hit the network are common but happen at build
time (cached), so they fail less interactively than runtime hooks; still required for parity
with the "just works" promise. Depends on the same discovery/config surface as US2.

**Independent Test**: Enable injection, run `up`/`build` on a config with a feature whose
`install.sh` performs a proxied download. The build succeeds; `docker run <image> cat <cert
path>` shows the corporate cert present in the built image.

**Acceptance Scenarios**:

1. **Given** injection is enabled, **When** deacon generates the feature-layering Dockerfile,
   **Then** a deterministic CA-install step appears before all feature `install.sh` RUN layers,
   and the resulting image contains the corporate CA in its trust store.
2. **Given** the same image content and CA set, **When** the Dockerfile is generated twice,
   **Then** the injected step is byte-stable (same content and ordering) so image-layer caching
   is preserved.
3. **Given** a user-authored Dockerfile (not the deacon-generated one), **When** `build` runs,
   **Then** deacon does **not** modify or rewrite it; the documented manual ARG/COPY convention
   is the supported path for certs inside a user's own Dockerfile build.

---

### User Story 4 - Machine owner controls activation with clear precedence (Priority: P2)

The machine owner can turn the injection capability on persistently via a user-level settings
file, per-invocation via an environment variable, or explicitly via a CLI flag — and can point
it either at auto-discovery or at a specific PEM bundle. When more than one source is present,
a single deterministic precedence applies. Nothing in the workspace can turn it on.

**Why this priority**: The configuration surface is what makes US2/US3 usable and safe; it is
the trust boundary. It is P2 because US2/US3 can be demonstrated with a single activation path
first, then generalized.

**Independent Test**: Set the capability in three places with conflicting values and confirm the
CLI flag wins over the env var, which wins over the settings file. Confirm a setting placed in
`devcontainer.json` is ignored. Confirm that with none of the three set, behavior is identical
to today.

**Acceptance Scenarios**:

1. **Given** `hostCa: "auto"` in the user-level settings file, no env var, no flag, **When**
   `deacon up` runs, **Then** auto-discovery injection is active.
2. **Given** the settings file says `"auto"`, the env var points at a PEM path, and the CLI flag
   points at a different PEM path, **When** `up` runs, **Then** the CLI flag's PEM is used (flag
   > env > settings).
3. **Given** a `devcontainer.json` (or any workspace file) that attempts to set host-CA
   injection, **When** `up` runs, **Then** that value has no effect on what CAs are trusted or
   injected.
4. **Given** none of settings file, env var, or CLI flag is set, **When** any command runs,
   **Then** no discovery and no injection occur and output is bit-for-bit unchanged from today.
5. **Given** a CLI flag or settings value pointing at an unreadable or non-PEM bundle, **When**
   `up`/`build` runs, **Then** deacon fails fast with a clear error naming the path and reason.

---

### Edge Cases

- **Discovery API failure** (host trust store cannot be enumerated): emit a clear error/warning;
  do not crash. Auto mode treats an enumeration failure as actionable, not silent.
- **Zero corporate certs discovered**: log at info and proceed without injection — not an error.
- **Multiple corporate CAs** on the host: all are discovered, logged by subject, and injected
  together as one ordered bundle.
- **Cert that is a leaf (CA:FALSE)** present in the host store: excluded from the corporate set
  (only `CA:TRUE` certificates are injected).
- **A public Mozilla root** also present in the host store: excluded by fingerprint (only the
  delta beyond the public root set is treated as "corporate").
- **Unsupported distro / no recognized trust-store updater**: warn and fall back to
  env-var-only injection.
- **Non-root container user with no sudo**: warn and fall back to env-var-only injection.
- **Remote Docker context** (host paths don't exist on the daemon): injection must stream the
  bundle over the exec channel, not via bind mount.
- **Compose and single-container** flows both honor the same injection ordering and env-var
  synthesis.
- **`exec` / `run-user-commands` after `up`**: the synthesized CA env vars are present for those
  sessions as well. `up` records the injected bundle path and subjects in the container's deacon
  labels; later sessions read them back on reconnect and re-apply the env vars — no re-discovery
  and no re-resolution of activation occurs at `exec`/`run-user-commands` time.

## Requirements *(mandatory)*

### Functional Requirements

#### Host trust store for deacon's own client (always on)

- **FR-001**: Deacon's own outbound HTTPS client MUST validate TLS using the host OS trust
  store, so corporate-installed root CAs are trusted with no configuration.
- **FR-002**: `DEACON_CUSTOM_CA_BUNDLE` MUST continue to work exactly as today, as an
  **additive** trust source layered on top of the host store — never a replacement.
- **FR-003**: With no corporate proxy and no custom bundle, public-root validation behavior
  MUST be unchanged from today.
- **FR-004**: All HTTP client implementations and test mocks MUST remain consistent with the
  trait contract after this change (no implementation is left behind).

#### Corporate CA auto-discovery

- **FR-005**: In auto mode, deacon MUST enumerate all root CAs trusted by the host, subtract the
  public Mozilla root set by certificate fingerprint (realized as a SHA-256 over each certificate's
  SubjectPublicKeyInfo — see research.md Decision 2), and keep only certificates with `CA:TRUE`
  basic constraints; the remaining delta is the corporate CA set.
- **FR-006**: Discovery MUST run on every `up`/`build` (no long-lived cache), because corporate
  certs rotate.
- **FR-007**: Deacon MUST log the subject of every discovered and every injected certificate at
  info level; discovery and injection MUST NOT be silent.
- **FR-008**: If discovery yields zero corporate certs, deacon MUST log that fact and proceed
  with no injection; this is not an error.
- **FR-009**: If the host trust store cannot be enumerated, deacon MUST surface a clear,
  actionable error/warning rather than silently proceeding as if zero certs were found.

#### Machine-level configuration surface

- **FR-010**: Deacon MUST support a user-level settings file in the user-data folder with a
  `hostCa` entry whose value is either `"auto"` or an absolute path to a PEM bundle. This is
  deacon's first user-level setting; the settings file MUST be small and extensible for future
  settings.
- **FR-011**: Deacon MUST read the settings file from the user-data folder only; it MUST tolerate a
  missing file (treated as "no setting") and unknown keys (forward compatibility). Deacon MUST NOT
  read settings from any workspace-resident file.
- **FR-011a** *(deferred — issue #198)*: In this feature the settings file is read-only from deacon's
  perspective; the machine owner provisions/hand-edits it, or uses the `--inject-host-ca` flag or
  `DEACON_INJECT_HOST_CA` env var. A `deacon settings get/set` write command (atomic temp-file +
  rename, user-data folder only, never workspace files) is tracked in issue #198 and is NOT part of
  this feature's scope.
- **FR-012**: Deacon MUST accept a CLI option on `up` and `build` that enables injection, with an
  optional value: omitted = auto-discovery; a path = use that PEM bundle.
- **FR-013**: Deacon MUST accept an environment variable equivalent (value `auto` or a path) for
  CI/one-shot use.
- **FR-014**: Activation precedence MUST be: CLI flag > environment variable > settings file.
  With none set, the capability is fully inactive.
- **FR-015**: Host-CA injection MUST NOT be configurable or influenceable by `devcontainer.json`
  or any workspace-resident file. A hostile workspace MUST NOT be able to change which CAs are
  trusted or injected.

#### Build-time injection (deacon-generated Dockerfile only)

- **FR-016**: When injection is enabled, deacon MUST insert a CA-install step into the
  deacon-generated feature-layering Dockerfile that installs the corporate bundle into the
  image trust store **before** any feature `install.sh` RUN layer.
- **FR-017**: The injected build step MUST be deterministic (stable content and ordering) so
  image-layer caching is preserved across runs with the same image and CA set.
- **FR-018**: Deacon MUST NOT modify or rewrite user-authored Dockerfiles. The gap MUST be
  documented along with a manual ARG/COPY convention for users who need certs inside their own
  Dockerfile build.
- **FR-018a**: Build-time injection applies ONLY to the deacon-generated feature-layering
  Dockerfile. For config shapes that produce no such Dockerfile (image-only without features,
  compose, user-authored Dockerfile), deacon MUST NOT perform build-time injection; those shapes
  rely on runtime injection (FR-019–FR-023). When injection is enabled but build-time injection
  is skipped because no feature-layering Dockerfile is generated, deacon MUST log that fact (not
  silent).

#### Runtime injection via exec, before lifecycle hooks

- **FR-019**: When injection is enabled, deacon MUST install the CA into the running container
  **after** create/start and **before** any lifecycle hook (ordering: create → start → inject CA
  → onCreate → postCreate → …).
- **FR-020**: The bundle content MUST be streamed into the container over the exec channel (no
  bind mount), so injection works with remote Docker contexts where host paths are absent on the
  daemon.
- **FR-021**: After writing the bundle, deacon MUST run the distro-appropriate trust-store
  update: `update-ca-certificates` (Debian/Ubuntu), `update-ca-trust` (RHEL/Fedora), bundle
  append (Alpine).
- **FR-022**: For an unsupported distro, or when the container user lacks root/sudo, deacon MUST
  emit a clear warning and fall back to env-var-only injection (FR-023). No silent fallback; the
  `up` MUST NOT abort solely because the system-store update could not run.
- **FR-023**: When injection is enabled, deacon MUST set these env vars to the installed bundle
  path via the existing container/remote env machinery: `SSL_CERT_FILE`, `NODE_EXTRA_CA_CERTS`,
  `REQUESTS_CA_BUNDLE`, `PIP_CERT`, `GIT_SSL_CAINFO`, `CURL_CA_BUNDLE`.
- **FR-024**: A user-specified value for any of those variables MUST take precedence over the
  synthesized value for that variable.
- **FR-024a**: `up` MUST record the injected bundle path and injected certificate subjects in the
  container's deacon labels at create time. `exec` and `run-user-commands` MUST read these labels
  back on reconnect and re-apply the synthesized CA env vars (subject to the same FR-024 user
  precedence), WITHOUT re-running host discovery or re-resolving activation. A later session MUST
  NOT consult workspace-resident configuration to decide CA env vars.
- **FR-025**: All container interaction for injection MUST go through the runtime abstraction so
  Docker and Podman behave consistently and test mocks stay valid (new runtime methods use
  default-impl delegation).

#### Output contract & observability

- **FR-026**: The JSON/text output stream contracts MUST be unchanged: all logs/diagnostics to
  stderr, results to stdout.
- **FR-027**: Deacon MUST emit tracing spans for discovery and injection (e.g. `ca.discover`,
  `ca.inject`).
- **FR-028**: The `up`/`build` JSON result MAY include an additive array of injected certificate
  subjects; existing result fields MUST be unchanged.

#### Safety / non-regression

- **FR-029**: With the feature unconfigured, default behavior MUST be bit-for-bit unchanged from
  today, and all existing tests MUST continue to pass.
- **FR-030**: Runtime paths MUST NOT panic; every degraded path (unsupported distro, no root,
  unreadable bundle, discovery failure) MUST emit a clear, actionable warning or error.
- **FR-031**: The extension MUST be documented in README and SECURITY.md, including the threat
  model: deacon injects a CA into every container only when the machine owner opts in, and never
  under workspace control.

### Key Entities *(include if feature involves data)*

- **Host trust store**: The operating system's set of trusted root CAs. Read-only input to
  deacon; the source of truth for both deacon's own TLS validation and corporate-CA discovery.
- **Corporate CA set**: The delta of host-trusted `CA:TRUE` certificates minus the public Mozilla
  root set, identified by SPKI SHA-256 fingerprint (research.md Decision 2). The bundle that gets injected.
- **User-level settings file**: A small, extensible JSON document in the user-data folder holding
  machine-owner preferences; first entry is `hostCa` (`"auto"` | absolute PEM path). Read-only to
  deacon in this feature (machine owner provisions/hand-edits it); a write command is deferred to
  issue #198.
- **Injection activation**: The resolved decision (off | auto | explicit-path) computed from CLI
  flag > env var > settings file. Never sourced from the workspace.
- **Injected bundle**: The PEM file materialized inside the container's trust store, referenced by
  the synthesized CA environment variables.
- **Injected-subjects record**: The list of certificate subjects discovered/injected, surfaced in
  logs and optionally in the JSON result.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: On a corporate machine behind a TLS-intercepting proxy with the corporate root CA
  in the host trust store, `deacon up` on a feature-using project completes its own
  feature/template pulls with zero certificate errors and zero extra configuration.
- **SC-002**: With injection enabled, a `postCreateCommand` that makes a proxied HTTPS call (e.g.
  `npm install`) succeeds on Debian/Ubuntu, RHEL-family, and Alpine base images.
- **SC-003**: With injection enabled, execing into the started container shows the corporate CA
  present in the system trust store before any lifecycle hook ran (verified by reading the cert,
  not by trusting the JSON outcome).
- **SC-004**: With injection enabled and a network-using feature `install.sh`, the
  feature-extended image contains the corporate CA in its trust store (verified by
  `docker run <image> cat <cert path>`).
- **SC-005**: With the feature unconfigured, output and behavior are byte-for-byte identical to
  the prior release across all existing tests.
- **SC-006**: Every discovered and injected certificate subject appears in the logs; no injection
  occurs without a corresponding log line.
- **SC-007**: A `devcontainer.json` attempting to enable or redirect host-CA injection has no
  effect on what is trusted or injected (verified by an adversarial test).
- **SC-008**: Each degraded path (unsupported distro, non-root user, unreadable/invalid bundle,
  discovery failure) produces a distinct, actionable warning or error and never a panic.

## Assumptions

- The host OS trust store is enumerable via a maintained platform API on the supported host
  platforms; the planning phase selects the specific mechanism (e.g. a platform verifier vs a
  native-certs reader) and the most maintained recommendation.
- "Public root set" means the bundled Mozilla/webpki root list deacon already ships; the
  corporate delta is computed against that list.
- The injected in-container bundle path is a single deterministic location used both for the
  system-store install and for the synthesized env vars.
- Supported in-container distro families for system-store install are Debian/Ubuntu, RHEL/Fedora,
  and Alpine; anything else is "unsupported" and falls back to env-var-only injection.
- The user-level settings file lives in the existing user-data folder (honoring
  `--user-data-folder`) alongside the existing trusted-workspaces store.
- Activation is global to the invocation (applies to all containers brought up by that command),
  not per-service.
- The capability targets Unix-like container guests; Windows containers are out of scope for the
  in-container trust-store install (consistent with the existing container feature surface).

## Out of Scope

- Modifying or rewriting user-authored Dockerfiles (documented manual convention instead).
- A long-lived cache of discovered certificates (discovery re-runs every invocation by design).
- Per-workspace or per-`devcontainer.json` control of CA trust/injection (explicitly forbidden).
- Client-certificate / mutual-TLS provisioning into containers (only root-CA trust is in scope).
- Windows container guests for in-container trust-store installation.
