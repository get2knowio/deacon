# Data Model: Corporate CA (Host Trust Store) Support

**Feature**: 016-host-ca-injection
**Date**: 2026-06-11

Entities are derived from the spec's Key Entities section and the research decisions. Types are Rust
shapes in `crates/core` unless noted. No structures replace spec-defined shapes; all additions are new.

---

## `Settings` (user-level machine settings)

Serde struct persisted at `{user_data_folder}/settings.json`. First and only field today is `hostCa`.
Tolerates unknown fields for forward compatibility.

| Field | Type | Notes |
|---|---|---|
| `host_ca` | `Option<String>` | serde rename `"hostCa"`. `"auto"` or an absolute PEM path. Absent ⇒ no setting. |

- **Validation**: when present and not `"auto"`, MUST be an absolute path (validated when used, not at
  load, so a stale path doesn't break unrelated commands). Unknown keys are tolerated on read (forward
  compatibility).
- **Persistence**: **read-only in this feature** — deacon loads the file (tolerating missing file +
  unknown keys) but does not write it. A write path (`deacon settings set`, atomic temp-file +
  `fs::rename` per `cache/disk.rs::save_index`) is deferred to issue #198.
- **Source boundary**: read only from the user-data folder; never from the workspace.

---

## `HostCaActivation` (resolved decision)

Enum produced by the precedence helper (research Decision 7). Drives all downstream behavior.

```
enum HostCaActivation {
    Off,                    // no discovery, no injection — default
    Auto,                   // discover corporate certs from the host store
    ExplicitPath(PathBuf),  // use this PEM bundle verbatim as the corporate set
}
```

- **Derivation**: `CLI flag > DEACON_INJECT_HOST_CA env > Settings.host_ca > Off`.
- A valueless CLI flag, or value `"auto"`, ⇒ `Auto`. Any other non-empty value ⇒ `ExplicitPath`.
- **Never** derived from workspace-resident config (FR-015).

---

## `HostCertificate` (a discovered/parsed root)

Internal representation of one parsed host-store certificate during discovery.

| Field | Type | Notes |
|---|---|---|
| `der` | `Vec<u8>` (`CertificateDer`) | raw certificate bytes from `rustls-native-certs` |
| `subject` | `String` | Subject DN, for info logging (FR-007) |
| `spki_sha256` | `[u8; 32]` | SHA-256 of SubjectPublicKeyInfo — identity key for subtraction (Decision 2) |
| `is_ca` | `bool` | BasicConstraints `CA:TRUE` (only `true` are kept) |

- **Lifecycle**: built transiently per `up`/`build` invocation; never cached (FR-006).

---

## `CorporateCaSet` (the injection payload)

The computed delta and its serialized bundle.

| Field | Type | Notes |
|---|---|---|
| `certs` | `Vec<HostCertificate>` | host `CA:TRUE` certs minus the public set, sorted by `spki_sha256` (deterministic ordering, FR-017) |
| `subjects` | `Vec<String>` | derived Subject DNs, for logs + JSON result + label persistence |
| `pem_bundle` | `String` | PEM concatenation in sorted order; the bytes streamed in / mounted at build |

- **Empty case**: `certs` empty ⇒ log "zero corporate certs" and proceed without injection (FR-008). Not
  an error.
- **ExplicitPath case**: `pem_bundle` is the file's contents (validated PEM); `subjects` parsed from it
  for logging; no host enumeration runs.

---

## `InjectionOutcome` (per-container result)

Result of the runtime injection attempt, surfaced in logs/labels/JSON.

| Field | Type | Notes |
|---|---|---|
| `mode` | enum `{ SystemStore, EnvVarOnly }` | `SystemStore` when the distro updater ran; `EnvVarOnly` on unsupported distro / non-root fallback (FR-022) |
| `bundle_path` | `String` | canonical in-container PEM path (e.g. `/usr/local/share/deacon/host-ca.crt`) |
| `injected_subjects` | `Vec<String>` | subjects actually injected (FR-028) |
| `warning` | `Option<String>` | populated on the fallback path with an actionable message |

---

## Container label additions

Added to `ContainerIdentity::labels()` output (`crates/core/src/container.rs:207`) at `up` create time,
read back on reconnect by `exec`/`run-user-commands` (FR-024a).

| Label key | Value | Notes |
|---|---|---|
| `devcontainer.deacon.hostCaBundlePath` | in-container PEM path | empty/absent when injection was off |
| `devcontainer.deacon.hostCaSubjects` | newline- or comma-joined subject DNs | for observability + re-apply context |

- **Identity neutrality**: these labels are written **after** identity hashing and MUST NOT feed
  `workspace_hash`/`config_hash` (they are informational, like `local_folder`/`config_file`). Reconnect
  selection still matches on the existing `source + workspaceHash + configHash` selector — the new
  labels are read from the matched container, not used to match.

---

## Synthesized CA environment variables

Set into `container_env` (at create) and re-applied for `exec`/`run-user-commands` from the label-read
bundle path. User-provided values win (FR-024).

| Variable | Value |
|---|---|
| `SSL_CERT_FILE` | `bundle_path` |
| `NODE_EXTRA_CA_CERTS` | `bundle_path` |
| `REQUESTS_CA_BUNDLE` | `bundle_path` |
| `PIP_CERT` | `bundle_path` |
| `GIT_SSL_CAINFO` | `bundle_path` |
| `CURL_CA_BUNDLE` | `bundle_path` |

- **Precedence rule**: synthesize with "insert only if absent" semantics over the user's
  `containerEnv`/`remoteEnv` + CLI `--remote-env` (mirrors the secrets `or_insert_with` merge at
  `up/mod.rs:329`).

---

## Relationships

```
Settings.host_ca ┐
DEACON_INJECT_HOST_CA env ┼─► HostCaActivation ─► (Auto) ─► enumerate host store ─► [HostCertificate]
--inject-host-ca flag ┘                          │                                      │
                                                  │                              subtract public set
                                                  │                                      ▼
                                                  └─ (ExplicitPath) ──────────────► CorporateCaSet
                                                                                         │
                                   build-time: named build context + RUN step ◄──────────┤
                                   runtime: exec_with_stdin → install script ─► InjectionOutcome
                                                                                         │
                                                            container labels + CA env vars ◄┘
```
