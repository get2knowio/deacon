# Contract: Observer Channel

Each observable channel is captured by one module in `parity-harness/src/observe/` implementing a small `ChannelObserver` contract. The runner invokes only the observers a case's `expected`/`fsAllowlist` declares (a pure `read-configuration` CLI case pays nothing for Docker inspection).

## `ChannelObserver` (conceptual)

```text
capture(ctx: &RunContext, op: &Operation) -> Result<RawChannelEvidence, HarnessError>
```

`RawChannelEvidence = { channel, operation, present: bool, value: serde_json::Value }`.
`present:false` when the channel could not be observed for this op (distinct from a captured empty value — FR-018).

## Channels, capture, assertion shape, and named normalization rules

| Channel | Captures | `assertion` shape (spec-expectation) | Normalization rules applied |
|---------|----------|--------------------------------------|-----------------------------|
| `chan-exit-code` | process exit status | `{ equals: <int> }` / `{ nonZero: true }` | none |
| `chan-stdout` | raw stdout bytes | `{ equals } / { contains } / { matches }` | `path_token` |
| `chan-stderr` | raw stderr bytes | `{ contains } / { matches }` | `path_token` |
| `chan-structured-output` | parsed JSON result doc | `{ jsonEquals } / { jsonSubset }` | `path_token`, `null_preserving` |
| `chan-filesystem` | presence/attrs of allowlisted paths (NOT full tree) | `{ exists } / { absent } / { mode }` | `path_token` |
| `chan-file-content` | contents of an allowlisted file | `{ equals } / { contains }` | `path_token`, `null_preserving` |
| `chan-image` | built-image config + labels | `{ labels: {...}, env: [...], entrypoint: [...] }` | `label_semantic`, `null_preserving` |
| `chan-process-graph` | container/network/volume/mount graph | `{ mounts: [...], networks: [...], volumes: [...] }` | `mount_source_canonical`, `path_token` |
| `chan-injected-process` | env, user, cwd, PATH, signals, TTY, exit propagation | `{ env: {...}, user, cwd, path: [...], tty: bool }` | `path_env_segmented`, `null_preserving`, `path_token` |
| `chan-temporal` | lifecycle ordering, first-create vs restart, resume, cleanup | `{ order: [...], firstCreate: {...}, restart: {...} }` | ordering-preserving; `null_preserving` |

## Named normalization rules (extend the single `normalize.rs` — FR-022..030)

| Rule | Effect | Guard |
|------|--------|-------|
| `path_token` | temp workspace/project paths → `<WORKSPACE>` / `<PROJECT>` tokens | rewrite, never delete (FR-024) |
| `label_semantic` | parse metadata labels into key/value; compare semantically | not opaque strings (FR-026); no blanket label removal (FR-029) |
| `mount_source_canonical` | canonicalize mount `source` before compare | compare after substitution (FR-027) |
| `path_env_segmented` | split PATH into segments; optional executable probe | segment-wise, not string-equal (FR-028) |
| `null_preserving` | keep missing / null / empty / defaulted distinct | only a named rule may collapse a specific field (FR-025) |

`NORMALIZER_VERSION` is bumped when any rule changes; recorded in provenance; participates in staleness (FR-030). No rule may blanket-remove env vars, labels, mount sources, entrypoints, commands, or networks (FR-029) — the reclassified former `NOISE_ENV_KEYS`/`INTENTIONAL_LABEL_PREFIXES` become named, scoped, rationale-carrying rules (research D6).

## Failure phase (FR-009 / clarify Q5)

When an operation fails, `chan-*` observers still record whatever was captured, and the CLI-process observer records `failurePhase` from the closed set:
`config-resolution · build · container-create · lifecycle:onCreate · lifecycle:updateContent · lifecycle:postCreate · lifecycle:postStart · lifecycle:postAttach · exec`.

## Per-channel verdict (FR-015)

```text
ChannelVerdict = { channel, outcome, detail }
outcome ∈ { agree, diverge, allowed-difference, no-reference-for-platform, stale, error }
```

`allowed-difference` is emitted only when a divergence is fully covered by a scoped `AllowedDifference` matching `(behavior, observablePath, context)`; otherwise the raw divergence stands as `diverge`. `CaseVerdict.overall` = the worst channel outcome; the case is attributable to its linked behavior(s) (FR-042).
