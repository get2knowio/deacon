# `obligation-gating` fixture registries

Fixture registries for the User Story 2 acceptance tests of
`024-deterministic-conformance-coverage` (`crates/conformance/tests/obligation_gating.rs`,
T052–T060; FR-078). They exercise **V28** (an applicable obligation with zero dispositions,
or with more than one) and **V29** (a filler rationale, a triple argued rather than tested, a
stale disposition, a dangling payload reference, a blanket waiver scope), and prove which
dispositions block strict certification and which do not.

## Why base + variants, and not nine complete registries

`base/` is one complete, self-consistent registry; each directory under `variants/` holds
**only the files that differ from it**, at the same relative paths. The test helper copies
`base/` into a tempdir, overlays one variant, and generates that tempdir's sibling
`obligations/obligations.json`.

Nine full copies of a twelve-file registry would bury each scenario's one interesting edit in
several hundred identical lines, and the review question here is always the same — *what one
thing changed, and does the gate notice?* The overlay makes that the literal content of the
directory.

Generating the obligation inventory at test time rather than committing one per variant is
the same argument applied to a machine-owned file: it is a pure function of the registry
(V27 says so), so a committed copy would be a second thing to keep in sync with no
independent authority.

## The base registry

Six scenario dimensions (the FR-003 required set), each with a single value, and one
operation — so the model yields **10 combination obligations** (`C(5,2)` pairs under one
operation) plus **3 behavior obligations**: the smallest shape in which pairs exist at all.

Its dispositions are deliberately a mix, so `base` is simultaneously the clean control and
the T055 scenario (an unexpired `waived` and a `non-testable` never block, and both are
enumerated):

| Obligation | Disposition | Why it is there |
|---|---|---|
| `bhv-readconfig-basic-parse` | `case` | the ordinary covered path |
| `bhv-readconfig-malformed-jsonc-rejected` | `waived` | non-blocking until its waiver expires (T054 re-evaluates the same fixture at a later `--today`) |
| `bhv-exec-podman-keep-id` | **none** | Podman-only, so outside the active Docker profile: `inactive-environment` is derived, and nobody owes it a decision. Leaving it undispositioned is the assertion |
| one combination | `case` | proves a combination can be covered |
| nine combinations | `non-testable` | each with a rationale that names a ground |

`waivers/wvr-blanket-state-field.json` is present but unused by `base`: its `field: "*"`
scope exists only so the `blanket-waiver` variant can point a disposition at it.

## The variants

| Variant | The one change | What it must produce |
|---|---|---|
| `undispositioned` | one combination's `odp-` record deleted | **V28**, naming the obligation, its operation and assignment |
| `gap` | one combination re-dispositioned `gap`, and the `gap-` record declared | certification **blocks** — through the gap record, which is where gap semantics already live |
| `conflicting` | a second `odp-` record for one obligation | **V28**, naming both records; resolution picks no winner |
| `filler-rationale` | a `non-testable` rationale that restates that it is out of scope | **V29** |
| `blanket-waiver` | a `waived` disposition backed by the `field: "*"` waiver | **V29** (FR-023, the analogue of V19) |
| `triple` | a high-risk triple, dispositioned `non-testable` | **V29** — a triple accepts only `case` or `gap` (FR-015) |
| `stale` | `sdim-features`'s value renamed | every obligation pinning it re-hashes: the old dispositions are **stale** (V29) and the new obligations are **undispositioned** (V28) |
| `new-behavior` | a behavior added, with a case, but no disposition for the obligation it generates | **V28** (SC-014) |

`stale` is deliberately driven by a *model* edit rather than by hand-writing a record that
names a nonexistent obligation. Both produce a stale record, but only the model edit shows
the mechanism the drift workflow actually meets: an obligation's id is substance-anchored, so
changing what it is produces a NEW obligation needing its own decision, and leaves the old
judgement pointing at nothing. A hand-written dangling id would prove the check fires without
proving it fires when it matters.
