# Quickstart: Operating the Conformance System

**Feature**: `026-continuous-conformance-certification`

Five workflows. All commands are dev-only; none is part of the shipped `deacon` CLI.

---

## 1. Run a lane locally

| Lane | Command | Needs |
|---|---|---|
| PR-Hermetic | `cargo nextest run --profile default` | nothing |
| PR-Docker | `cargo nextest run --profile pr-docker` | Docker |
| Nightly stable differential | `cargo nextest run --profile parity` | Docker + the pinned oracle |
| Canary | `cargo nextest run --profile canary` | Docker + a canary revision |
| Release certification | `cargo run -p deacon-conformance -- certify --report-dir target/conformance` | a manifest |

Or via the Makefile shortcuts: `make test-lanes`, `make test-drift`, `make test-pr-docker`,
`make test-canary`, `make certify-report`.

Check the lane definitions themselves:

```bash
cargo run -p deacon-conformance -- lane check          # V34 — blocks a PR
cargo run -p deacon-conformance -- lane report         # writes target/conformance/lanes.md; never gates
```

A lane whose precondition is missing **fails** — it never skips. If `pr-docker` reports a missing engine,
that is the contract working, not a flake.

---

## 2. Read a certification refusal

```bash
cargo run -p deacon-conformance -- certify --report-dir target/conformance
```

Exit `1` means not certified. Every blocking condition names its record, and all of them are reported in one
run — fix the whole list, not the first line:

```
not certified: prof-linux-amd64-docker-0870
  stale-snapshot        case-up-decl-basic-image   caseHash
  missing-required-execution  execution-manifest.json  (V35-absent)
  unresolved-gap        gap-pairwise-exec
```

Common causes, in the order they usually turn up:

| Message | What happened | Fix |
|---|---|---|
| `V35-absent` | the container lane did not run | run `--profile pr-docker`, or fetch the CI artifact |
| `V35-revision` | manifest is from another commit | re-run the container lane on this revision |
| `stale-snapshot` | a case or fixture changed after the snapshot was recorded | `conformance-snapshot refresh`, then review the diff |
| `incorrect-oracle` | recorded oracle ≠ the declared pin | you are mid-upgrade — finish it, or restore the pin |
| `unresolved-gap` | a `gap-*` record exists | resolve it (add a case, delete the gap in the same change) |

Certification needs **no network, no Docker, and no reference implementation**. If it seems to want one of
those, that is a bug in the gate, not in your environment.

---

## 3. Triage a drift signal

```bash
cargo run -p parity-harness --bin drift-scan -- --today "$(date -u +%F)" --write   # exits 0 whatever it finds
cargo run -p deacon-conformance -- drift report          # renders target/drift/observations.md
```

A drift signal is **not** a failure. It says upstream moved; it does not say deacon is wrong. `drift-scan`
exits non-zero only when it could not run.

Distinguishing "nothing changed" from "nothing ran": check `lastCompletedRun` in
`conformance/drift/observations.json`. Empty `records` **with** a `lastCompletedRun` covering all five kinds
means no drift. A missing or partial `lastCompletedRun` means the scan did not complete — treat the empty
list as unknown, not as clean.

For each signal, decide: does it change what deacon should do? If yes, that is ordinary conformance work
(add a behavior, a case, a disposition). If it is a new reference release, see workflow 4.

---

## 4. Prepare a stable oracle upgrade

The pin never advances automatically. To propose one:

```bash
cargo run -p parity-harness --bin oracle-upgrade-propose -- --from 0.87.0 --to 0.88.0
cargo run -p deacon-conformance -- drift proposal check target/drift/upgrade-proposal.json
```

Review all seven sections. `"entries": []` means investigated and clean; a **missing** section means not
investigated, and the bundle is rejected.

If you accept it, three things land in one reviewed change:

1. the pin, in `conformance/registry/revisions.json` **and** `fixtures/parity-corpus/oracle.json`;
2. re-recorded snapshots via `cargo run -p parity-harness --bin conformance-snapshot refresh` — the git diff
   is the review surface;
3. affected dispositions, waivers, and gaps.

Canary results may inform the decision but cannot back it unless the run was fully pinned and hermetic.

---

## 5. Add a case and keep the lanes honest

Adding a case is still a pure data edit. What is new is that the lane assignment is *derived*, so you cannot
forget it — but you can still get the derivation wrong:

```bash
# 1. Add the case record to conformance/registry/cases/<area>.json
# 2. Its lane follows from oracleType + resourceGroup — no new field to set.
cargo run -p deacon-conformance -- validate      # V16/V26 + V34
cargo run -p deacon-conformance -- lane check    # is it assigned? which lane?
cargo run -p deacon-conformance -- lane report   # confirm it landed where you expected
```

Two traps worth knowing:

- **`oracleType` decides the lane.** A case typed `live-differential` runs nightly and contributes nothing to
  PR-Docker. If you wanted PR-time coverage, it needs to be `spec-expectation` or `snapshot`.
- **Flip `odp-cmb-*` in the same commit.** Explicit dispositions take precedence over the mechanical
  `scenarioContext` match, so a new case leaves its combination record reading `gap` until you change it.
  This is unchanged from 024 and remains the most common way a commit half-lands.

### Adding a new test binary

The lane denominator is derived from a scan of `crates/deacon/tests/*.rs`, so a new binary appears in the
denominator immediately and `lane check` fails until it is assigned. That is the intended sequence. Then:

- **hermetic binary** → add it to `lane-pr-hermetic`'s `programs`; it runs in `default`/`dev-fast` already;
- **live binary** → add it to exactly one lane, allow-list it in that lane's profile, and exclude it from the
  other six. The `default-filter` must be an explicit `binary(=…)` list, never a glob — a `conformance_*`
  glob would capture the hermetic `conformance_replay` and silently drop it from the fast lane, which is the
  mistake both the parity and discovery profiles have documented making.

`lane check` verifies the profile filter against the declared programs, so this drift fails structurally
rather than in review.
