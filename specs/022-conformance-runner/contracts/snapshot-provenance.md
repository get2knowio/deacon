# Contract: Snapshot & Provenance

Committed under `conformance/snapshots/<os>-<arch>/<case-id>/`. Three files; raw and normalized evidence are **separate** (FR-016). Written atomically (temp file + `fs::rename`) by the reviewed refresh only; never during ordinary runs (FR-021).

## Layout

```text
conformance/snapshots/
└── linux-x86_64/
    └── case-up-postcreate-env/
        ├── provenance.json     # the 13 required elements
        ├── raw.json            # verbatim per-channel evidence
        └── normalized.json     # rule-normalized per-channel evidence
```

## `provenance.json`

```json
{
  "oracleVersion": "0.80.0",
  "sourceRevision": "113500f4",
  "caseHash": "9f2a…",
  "fixtureHash": "3c71…",
  "argv": ["up", "--workspace-folder", "<WORKSPACE>"],
  "platform": "linux",
  "arch": "x86_64",
  "nodeVersion": "20.11.1",
  "dockerVersion": "27.1.1",
  "composeVersion": "2.29.1",
  "imageDigests": { "mcr.microsoft.com/devcontainers/base:bookworm": "sha256:…" },
  "normalizerVersion": "2",
  "capturedAt": "2026-07-24T00:00:00Z"
}
```

The twelve fields above are the FR-017 identity/environment elements; the thirteenth FR-017 element — the captured observables — is the sibling `raw.json`/`normalized.json` (below), NOT `provenance.json`. `capturedAt` is an informational provenance-file field, not one of the thirteen and excluded from staleness. `argv` is the complete argument vector with temporary paths tokenized (workspace path → `<WORKSPACE>`) for portability; `raw.json` retains the verbatim argv. `imageDigests` covers every pinned image the operations used.

## `raw.json` / `normalized.json`

Array of channel evidence objects:

```json
[
  { "channel": "chan-exit-code", "operation": "op-exec-env", "present": true, "value": 0 },
  { "channel": "chan-injected-process", "operation": "op-exec-env", "present": true,
    "value": { "env": { "MY_VAR": "hello", "TZ": "<HOST-TZ>" }, "user": "vscode", "cwd": "<WORKSPACE>",
               "path": ["/usr/local/bin", "/usr/bin", "/bin"] } }
]
```

`present:false` ⇒ channel not captured; `value:null`/`""`/`[]` are captured-but-empty and remain distinct (FR-018/FR-025). `raw.json` preserves temp paths verbatim; `normalized.json` shows the `<WORKSPACE>`/`<HOST-TZ>` tokens (FR-024). Nothing is blanket-removed (FR-029).

## Staleness (replay gate — FR-020, SC-003)

On replay, recompute the evidence-determining inputs. Compare to `provenance.json`. **Fail as stale**, naming the FIRST mismatched field, if any differ:

`caseHash · fixtureHash · oracleVersion · sourceRevision · imageDigests · normalizerVersion`

`capturedAt` is informational (never triggers staleness). `platform`/`arch` are **selectors**, not staleness fields:
- match → use that snapshot;
- no snapshot for the current `<os>-<arch>` → **`no-reference-for-platform`** verdict (a coverage gap, distinct from stale and from a silent skip — FR-016a).

The host tool versions `nodeVersion`/`dockerVersion`/`composeVersion` are recorded for reproducibility but are **NOT** staleness fields. The reference CLI's output is independent of the host Node runtime, and any Docker/Compose-version effect on recorded evidence surfaces as a real *divergence* on replay — not a false stale. Gating on host tool versions would make every committed snapshot stale on every machine but the recorder's, defeating cross-machine CI replay (SC-003) — e.g. a snapshot recorded under Node 22 would falsely fail the parity lane's Node 20.

## Refresh (reviewed action only — FR-022)

`cargo run -p parity-harness --bin conformance-snapshot -- refresh [--case <id>] [--platform <os-arch>]`:
1. Requires the verified pinned oracle + Docker/Node (fail-loud otherwise).
2. Runs each case's operations against the reference, captures + normalizes, writes the three files atomically.
3. Emits a summary diff (raw + normalized) for the reviewer; the git diff is the review surface.

Ordinary test runs call only the **read/compare** path (`conformance snapshot check`, hermetic) — they never write (FR-021). CI asserts this: `snapshot check` in the PR lane fails if committed snapshots are stale, but never rewrites them.

## Hermetic vs live split

| Operation | Tool | Lane |
|-----------|------|------|
| `snapshot check` (staleness compare, hermetic) | `deacon-conformance` bin | dev-fast / PR |
| `snapshot diff <old> <new>` (deterministic drift) | `deacon-conformance` bin | dev-fast |
| `refresh` (live re-record, writes files) | `parity-harness` `conformance-snapshot` bin | manual / reviewed |
