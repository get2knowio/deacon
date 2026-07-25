# Contract: Dev-Only Command Surface

**Constitution II is binding here**: none of these commands may reach the shipped `deacon` CLI. A hermetic test asserts that `deacon --help` gains no subcommand from this feature.

Invocation: `cargo run -p deacon-conformance -- <cmd>` (hermetic) and `cargo run -p parity-harness --bin <bin>` (live).

---

## Hermetic — `deacon-conformance` (no Docker, no network, runs in every profile)

### `baseline generate`

Enumerate the pre-migration inventory and write `conformance/migration/baseline.json`.

| Aspect | Contract |
|---|---|
| Inputs | Repository tree; `fixtures/parity-corpus/registry.json`; the production discovery functions (`discover_tier1_cases`, `discover_error_cases`) — **never** an independent directory walk (research D1) |
| Output | Atomic write; byte-stable; sorted by `id` |
| `--freeze <sha>` | Records the freeze commit in `revision` |
| Exit | `0` on write; non-zero with a cause-specific message on any enumeration failure |
| Guard | Refuses to overwrite a frozen baseline unless `--force`, so re-running never silently relaxes the baseline (FR-045) |

### `baseline check`

Recompute in memory and byte-compare against the committed file. **Never writes.**

Exit `0` on match; `1` naming each drifted item and whether it was added, removed, or changed (FR-004). Emitted as **V25** when run under `validate`.

### `migration report [--format json|md] [--out-dir <dir>]`

Produce the before-and-after conservation accounting from `baseline.json` + `mapping.json` + the registry.

> **Naming**: this is the **migration conservation** report. It is deliberately NOT called `coverage` — `deacon-conformance` already owns `coverage.rs`, which evaluates *behavior* coverage against the active certification profile. Two different questions ("did anything get lost in the move?" vs "is every in-profile behavior covered?") must not share a name or a module.

| Aspect | Contract |
|---|---|
| Output | `target/conformance/migration-report.{json,md}`; deterministic — no timestamps, no absolute paths (FR-043) |
| stdout/stderr | JSON mode: the single document on stdout, diagnostics on stderr (Constitution VI) |
| Exit | `0` only when every baseline item is accounted for; otherwise `1` |

### `migration check`

The gating form: same computation, no file output, fails naming each unaccounted item, missing counterpart, weakened error path, or inflated behavior denominator (FR-040–FR-042, SC-005).

### `migration scaffold`

Emit skeleton `mapping.json` / `residuals.json` entries to **stdout** for every unmapped baseline unit, with `"UNREVIEWED"` sentinels that the loader rejects. **Never writes the registry** — mirrors `inventory scaffold` / `clause scaffold`. Generation never touches hand-authored files.

### `validate` (extended)

Adds **V21**–**V25** (data-model §7). Reports all violations in one run. Gates every PR via the hermetic `registry_valid` test.

### `certify` (extended)

Lists the residual queue as **non-blocking** information (FR-054). Still blocks only on gaps, uncovered in-profile behaviors, and the inventory/clause classes.

---

## Live — `parity-harness` (requires pinned oracle + Docker; `--profile parity` only)

### `equivalence-report`

Run each superseded carrier and its replacement over the full baseline and emit the per-unit ledger.

| Aspect | Contract |
|---|---|
| Output | `target/parity/equivalence.json` (data-model §5) |
| Preconditions | Verified pinned oracle and Docker — **fail loud**, never skip to pass (Constitution IV) |
| Exit | `0` when every compared unit is `equivalent` or `stricter`; `1` naming each `more-permissive` unit and the specific unsatisfied deletion condition (FR-037) |
| `--carrier <name>` | Restrict to one carrier, for incremental migration |

**Note**: `stricter` is a success that must still be acted on — each newly detected difference is characterized per FR-036, never suppressed.

---

## Retired surfaces (FR-031/FR-032 — removed in the same change as their replacement)

| Removed | Replaced by |
|---|---|
| `parity_corpus_tier1`, `parity_corpus_merged`, `parity_corpus_errors`, `corpus_runner/mod.rs` | declarative cases driven by `parity_conformance_runner` |
| `parity_read_configuration` | declarative cases |
| `make test-parity`'s per-binary selection | profile selection over the surviving runner |

Commands, `.config/nextest.toml`, `fixtures/parity-corpus/registry.json`, CI workflow, and documentation move in lockstep; a reference to a removed surface fails validation (FR-032). No compatibility alias is retained.
