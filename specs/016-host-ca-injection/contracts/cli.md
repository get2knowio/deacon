# CLI & Interface Contracts: Corporate CA (Host Trust Store) Support

**Feature**: 016-host-ca-injection
**Date**: 2026-06-11

These are the observable contracts (CLI surface, env vars, settings file, JSON result, runtime trait,
in-container script) for this feature. Behavior MUST match these exactly.

---

## 1. `--inject-host-ca [PATH]` flag (on `up` and `build`)

```
--inject-host-ca [<PATH>]    Enable host-CA injection into the dev container.
                             Omit the value for auto-discovery of corporate root CAs.
                             Provide an absolute PEM path to inject that bundle verbatim.
```

- clap: `num_args = 0..=1`, `default_missing_value = "auto"`, `value_name = "PATH"`.
- Present without value ⇒ `Auto`. Present with `auto` ⇒ `Auto`. Present with a path ⇒ `ExplicitPath`.
- Absent ⇒ defer to env var, then settings, then `Off`.
- **Not** added to `exec` or `run-user-commands` (they read persisted labels — FR-024a).

## 2. `DEACON_INJECT_HOST_CA` environment variable

- Value `auto` ⇒ `Auto`. Absolute path ⇒ `ExplicitPath`. Unset/empty ⇒ no effect.
- Lower precedence than the CLI flag, higher than the settings file.
- Defined as a named constant per Constitution V (env-var-constants rule).

## 3. `DEACON_CUSTOM_CA_BUNDLE` (unchanged, additive)

- Continues to add a PEM to **deacon's own** HTTP client trust set (`oci/client.rs`), now layered on
  top of the host roots and webpki public roots. No behavior change vs today (FR-002).

## 4. Settings file (read-only in this feature)

`{user_data_folder}/settings.json` — deacon **reads** this to resolve `hostCa`:

```jsonc
{ "hostCa": "auto" }        // or "/absolute/path/to/bundle.pem"
```

- Honors `--user-data-folder`. Read only from the user-data folder, never from the workspace.
- Missing file ⇒ "no setting"; unknown keys ⇒ tolerated (forward compatibility).
- **Deferred (issue #198)**: a `deacon settings get/set` command to inspect/write this file (atomic
  write, validated keys) is out of scope here. For now the machine owner hand-edits/provisions the file
  or uses `--inject-host-ca` / `DEACON_INJECT_HOST_CA`.

## 5. JSON result additions (additive only)

`up` / `build` JSON result MAY gain:

```jsonc
{
  // ... existing fields unchanged ...
  "injectedCaSubjects": ["CN=ACME Corp Root CA, O=ACME, C=US", "..."]  // present only when injection ran
}
```

- Field is **omitted** when injection is `Off` or yielded zero certs (keeps default output byte-stable,
  FR-005/FR-029). Existing fields and ordering are untouched.

## 6. Runtime trait contract — `exec_with_stdin`

New method on the `Docker` trait (`crates/core/src/docker.rs`):

```rust
/// Exec a command in the container, streaming `stdin` bytes to its standard input.
/// Default impl returns an `unsupported` domain error so existing mocks/runtimes
/// compile unchanged; real runtimes override it.
async fn exec_with_stdin(
    &self,
    container_id: &str,
    command: &[String],
    stdin: &[u8],
    config: &ExecConfig,
) -> Result<ExecResult> {
    // default: Err(DeaconError::Unsupported { capability: "exec_with_stdin" })
}
```

- `CliRuntime` overrides it (covers Docker + Podman). `ContainerRuntimeImpl` forwards to the inner
  runtime. Mocks inherit the default unless a test exercises injection.
- No bind mount; bytes travel over `docker exec -i`. Works with remote Docker contexts (FR-020).

## 7. In-container install script contract

Run via `exec_with_stdin` (runtime) and as the generated `RUN` step (build). Single POSIX `sh -c`.

**Inputs**: bundle PEM on stdin (runtime) or mounted file (build); target canonical path
`/usr/local/share/deacon/host-ca.crt`.

**Behavior**:
1. Write the bundle to the canonical path (always — so env-var-only fallback has a real file).
2. Detect distro via `/etc/os-release` (`$ID`, `$ID_LIKE`).
3. Install into the system store:
   - debian/ubuntu ⇒ split into `/usr/local/share/ca-certificates/` + `update-ca-certificates`
   - rhel/fedora/centos ⇒ copy to `/etc/pki/ca-trust/source/anchors/` + `update-ca-trust extract`
   - alpine ⇒ `/usr/local/share/ca-certificates/` + `update-ca-certificates`

**Exit contract** (mapped to Rust outcomes — no silent fallback, FR-022/FR-030):

| Exit code | Meaning | Deacon action |
|---|---|---|
| `0` | system store updated | `InjectionOutcome.mode = SystemStore` |
| `10` | unsupported distro (no recognized updater) | warn + `EnvVarOnly` |
| `11` | not root / insufficient permissions | warn + `EnvVarOnly` |
| other non-zero | unexpected failure | warn with captured stderr + `EnvVarOnly` |

## 8. Tracing spans (observability)

- `ca.discover` — wraps host enumeration + subtraction; fields: `host_total`, `corporate_count`,
  `mode` (`auto`/`explicit`).
- `ca.inject` — wraps runtime + build injection; fields: `bundle_path`, `subject_count`, `outcome`
  (`system_store`/`env_var_only`), `distro`.
- All logs on stderr; results on stdout (Constitution VI). Every discovered/injected subject logged at
  info (FR-007).

## 9. Exit-code / default-behavior invariants

- With no activation source set, **no** discovery span, **no** new env vars, **no** new labels, **no**
  JSON field — output is byte-for-byte unchanged (FR-029, SC-005).
- Invalid/unreadable explicit bundle ⇒ fail-fast non-zero exit with a message naming the path and reason
  (FR edge case, SC-008). Discovery enumeration failure in `auto` ⇒ actionable error/warning (FR-009).
