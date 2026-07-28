# Contract: `conformance lane` command group

**Crate**: `deacon-conformance` (dev-only). **Hermetic**: no network, no Docker, no oracle.
`deacon --help` MUST NOT gain any of these — asserted by `parity_registry_check`.

```
cargo run -p deacon-conformance -- lane <check|report|scaffold> [--lanes <DIR>] [--json]
```

Global flags `--registry <DIR>` and `--today <YYYY-MM-DD>` apply as they do to every other subcommand.
`--lanes <DIR>` overrides the lane root (defaults to `<workspace>/conformance/lanes`) so tests can point at
fixture trees — the same override pattern `--registry` already uses, and the reason no bypass flag is needed.

## `lane check`

Read-only by construction. Derives the execution-unit denominator (data-model §2), evaluates every lane's
`includes`, and reports **V34** violations.

**Exit codes**: `0` no violations · `1` one or more violations · `2` usage or IO error.

**Output**: one violation per line on stdout, or a single `{ "ok": bool, "violations": [...] }` document with
`--json`. Logs to stderr. Every violation names its offending record (FR-042):

```
V34 unit-prog-conformance_replay assigned to zero lanes
V34 lane-pr-docker declares nextestProfile "pr-docker" whose default-filter omits conformance_docker_pinned
V34 lane-canary declares blocking: true (FR-019 requires non-blocking)
```

**Determinism**: violations are emitted in a stable order (kind, then id). Running twice on unchanged inputs
produces byte-identical output.

## `lane report`

Writes `target/conformance/lanes.{json,md}` — for each lane: trigger, blocking status, preconditions, the
units it selects, and the units it explicitly excludes with the stated rationale (FR-005).

**Reporting never gates.** The exit code reflects whether the report could be *written*, never what it says —
`0` on successful write even when every lane is misconfigured, `2` if the output directory is unwritable.
This is the same rule `coverage report` follows, and for the same reason: a reporting command wired into CI
becomes a gate the moment its status depends on its content.

## `lane scaffold`

Emits a skeleton lane record to **stdout only** — never writes. Fields a human must decide carry the
`"UNREVIEWED"` sentinel, which the loader rejects, so a scaffold cannot be committed unedited.

```
$ cargo run -p deacon-conformance -- lane scaffold --for-unit unit-prog-new_binary
{ "id": "UNREVIEWED", "displayName": "UNREVIEWED", "trigger": "UNREVIEWED", ... }
```

## Interactions

- `validate` runs `lane check`'s V34 pass as part of `validate_path_with_inventory`, so a lane defect blocks
  a PR through the existing `registry_valid` test — no new CI wiring.
- `certify` does **not** consult lane records. A release verdict must not depend on CI configuration
  (research D1); lane integrity blocks the PR that broke it instead.
