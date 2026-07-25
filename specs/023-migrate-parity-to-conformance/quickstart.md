# Quickstart: Migrating Parity Assets into the Declarative Conformance System

**Branch**: `023-migrate-parity-to-conformance` · **Spec**: [spec.md](./spec.md) · **Baseline**: [research.md §1](./research.md)

Everything here is **dev-only tooling**. None of it ships in the `deacon` CLI (Constitution II).

---

## 0. One-time: freeze the baseline

```bash
cargo run -p deacon-conformance -- baseline generate --freeze $(git rev-parse --short HEAD)
cargo run -p deacon-conformance -- baseline check     # must be clean before anything else
```

This writes `conformance/migration/baseline.json` — at the freeze it was 144 records (111 executable units + 33 recorded-only external entries); it is **151** today because US4 deliberately added 7 fault-injection guard units, each re-frozen with `--force`. Commit it. From here on it is read-only evidence.

> **The drift gate (V25) is retired** (T099, FR-053). It compared the committed baseline against a fresh enumeration, which stops being possible the moment a superseded carrier is deleted — its units simply leave the enumeration. A permanent gate would forbid ever retiring the machinery this migration exists to retire. The artifact is retained; `baseline check` still reports drift, informationally.

> The enumeration calls the production discovery functions on purpose. Do not re-implement the directory walk — that is how the Tier-1 corpus was mistaken for 25 cases when it holds 24 (research D1).

---

## 1. The migration loop (one carrier at a time)

Work carrier by carrier, lowest residual risk first (research D4): the corpora and `read-configuration` before the Docker-backed state programs.

```bash
# a) See what is still unmapped for this carrier
cargo run -p deacon-conformance -- migration scaffold | grep parity_corpus_tier1
```

Scaffold prints skeleton mapping/residual records to **stdout** with `"UNREVIEWED"` sentinels the loader rejects. It never writes the registry — you author `mapping.json`, `residuals.json`, and the cases by hand.

```bash
# b) Author the declarative cases + migrate fixtures one-to-one, then validate
cargo run -p deacon-conformance -- validate        # V21–V24 plus the existing classes
cargo run -p deacon-conformance -- migration check  # every baseline unit accounted for
```

Both are hermetic — no Docker, no network — so they run in every profile and gate every commit.

```bash
# c) Prove the replacement is not more permissive (parity lane; needs oracle + Docker)
cargo run -p parity-harness --bin equivalence-report -- --carrier parity_corpus_tier1
```

Read the ledger before deleting anything:

- `equivalent` → clean migration.
- `stricter` → **expected** while retiring `prune`; the replacement now sees a difference the old path hid. Characterize each one (case, waiver, or tracked fix) and record it in `characterizedAs`. Do not suppress it.
- `more-permissive` → **stop**. The replacement lost a check. Fix it, or record an explicit justified accepted difference. Deletion stays blocked.

```bash
# d) Delete only when the predicate holds
cargo run -p deacon-conformance -- migration report --format md
```

A carrier may be deleted only when it appears in `deletableCarriers`: every unit `equivalent`/`stricter`, no residual naming it, and the migration report accounting for everything it carried. Two further readings of that section matter:

- `deletionBlockers` says *why* each surviving carrier is held back. Equivalence-clean is necessary, not sufficient — `parity_up_exec` is clean and still blocked, because it carries the only evidence for `bhv-exec-container-id-metadata` and deleting it would swap a green ledger for a V5 uncovered behavior.
- `deletedCarriers` lists the carriers already gone. Once deleted, a carrier is absent from the live registry and therefore from both other lists, so this is the only place the completed work appears.

---

## 2. Deleting a carrier (all in one change — FR-031)

```bash
git rm crates/deacon/tests/parity_corpus_tier1.rs
# and, in the SAME change:
#   - fixtures/parity-corpus/<migrated dirs>       (only after fixtureMapping is one-to-one)
#   - fixtures/parity-corpus/registry.json          (live_binaries + corpora entries)
#   - .config/nextest.toml                          (ALL profiles: parity selection + exclusions)
#   - .github/workflows/parity.yml, Makefile, docs  (no dangling references — FR-032)
#     …including any GLOB that reads a deleted path: the workflow's image pre-pull
#     globbed `fixtures/parity-corpus/*/` and silently matched nothing once those
#     directories went (T116). A glob that matches nothing is not an error.
cargo run -p deacon-conformance -- validate && cargo nextest run -E 'binary(=parity_registry_check)'
```

No compatibility alias is left behind. A reference to a removed surface fails validation.

---

## 3. Full gate before PR

```bash
cargo fmt --all && cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
make test-nextest                 # includes the hermetic migration guards
make test-parity                  # live lane; needs pinned oracle + Docker
```

PR title must use an allowed Conventional-Commit type — `feat`/`fix`/`chore`, never `test` or `style`.

---

## 4. Things that will bite you

- **A wave of new differences is success, not regression.** Retiring `prune` un-hides deacon-only and empty-valued fields across the 48 corpus units (research D3). Budget characterization time; do not "fix" it by re-adding a blanket rule — a blanket drop is V24 by construction.
- **Residuals block their carrier, not the release.** A residual keeps `parity_state_diff` alive but never blocks `certify` (FR-054). Gaps still block. Do not file a residual as a gap.
- **The behavior count must not grow.** Duplicate coverage becomes variants of one behavior (FR-014). If `after.behaviors > before.behaviors`, the report fails — you created a behavior where you needed a variant.
- **Never edit the baseline to go green.** With V25 retired the automated catch is gone, so this rests on review: the baseline is version-controlled, and lowering it is a conspicuous diff in an artifact whose whole purpose is to be diffed. Fix the mapping instead.
- **New test binary ⇒ three nextest spots + the parity registry.** Mirror `run_user_commands_prebuild`; expect `nextest.toml` conflicts between in-flight PRs and resolve to the UNION of `binary(=…)` clauses.
- **Expect several programs to survive.** Research D4 predicted `parity_state_diff` (8 units) and `parity_observable_state` (7); in the end **five** survived — those two plus `parity_build` (6/6 residual), `parity_exec` (one residual), and `parity_up_exec`, which is equivalence-clean but is the ONLY evidence for `bhv-exec-container-id-metadata`. Say so in the PR rather than letting it read as incomplete work.
- **A timeout is not always a hang — check whether it is a download.** `read-configuration --include-merged-configuration` reads an image's `devcontainer.metadata` label and pulls on a cache miss, matching the reference. `mcr.microsoft.com/devcontainers/universal:2-linux` is **9.93 GB**; a first pull blows the harness's 120 s bound while deacon logs nothing, because the wait is inside Docker, not inside deacon's HTTP client. Run `scripts/parity/prepull-fixture-images.sh` (or `make test-parity`, which does) before blaming the code. Cached, the same command returns in 0 s (T116).
- **A carrier can be equivalence-clean and still undeletable.** A unit maps one-to-one to its replacement case, but a legacy case may claim SEVERAL behaviors from one reported outcome. Deleting it uncovers the rest, which `validate` reports as V5 — after the irreversible act. `deletionBlockers` names this case explicitly now.
