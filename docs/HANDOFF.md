# Handoff — parity page truthfulness & the waiver adjudication queue

**Session of 2026-08-01.** Working state, not project documentation: delete or replace it
when the queue below is empty. The durable rules this session produced live in
`conformance/RULES.md` ("A waiver is a maintainer decision, not a default") and in
`crates/conformance/src/parity_page.rs`'s module doc, not here.

## The standing rule that drove this session

> "I want almost no waivers — a waiver should be when you and I have discussed the item
> and I've agreed that a waiver makes sense. And there will be very few cases where I will
> think a waiver makes sense."

A `wvr-` record is never authored on the harness's say-so. The default disposition for a
newly surfaced difference is a **fix** (`follow-spec` / `align-with-reference` plus a
`parity-drift` issue) or a **`gap-`** record — both keep pressure on. A waiver ends the
argument permanently, so it needs explicit agreement on the specific item first.

## Where things stand

Merged this session: **#420** (outdated resolves a partial version pin), **#421** (parity
matrix as a work queue), **#422** (headline counts evidence, not claims), **#424** (a
waiver that is enforced vs one that merely exists).

Open, both green except the always-red `live-certification` lane:

| PR | What | Note |
|---|---|---|
| **#425** | Retires the 7 waivers that never waived a divergence | Held for your word — it touches records you are still setting policy on |
| **#426** | `spec` and `CLI` columns on the parity page | **Stacked on #425.** Retarget to `main` before merging #425 with `--delete-branch`, or GitHub closes it |

Waivers went 22 → 15. `certify` blocking count is unchanged at 48, so nothing regressed:
the 10 `gap-pairwise-*` records are still what makes the registry uncertified, exactly as
024 left it.

## The queue: 15 waivers, three groups

Run this to regenerate the table (it is derived, never hand-maintained):

```bash
jq -s '[.[].records[]]' conformance/registry/behaviors/*.json > /tmp/bhv.json
jq -s '[.[].records[]]' conformance/registry/cases/*.json     > /tmp/cases.json
jq -s '.'               conformance/registry/waivers/*.json   > /tmp/wvr.json
jq -rn --slurpfile b /tmp/bhv.json --slurpfile c /tmp/cases.json --slurpfile w /tmp/wvr.json '
  ($b[0] | INDEX(.id)) as $B
| ([$c[0][] | .allowedDifferences[]? | .waiverId] | map(select(.)) | unique) as $consumed
| $w[0][] | .id as $wid | (.behaviors[0]) as $bid
| ($B[$bid]) as $bh
| ([$c[0][] | select((.behaviors//[]) | index($bid))] | length) as $n
| [ ($wid|sub("^wvr-";"")), $bh.spec, $bh.decision,
    (if ($consumed|index($wid)) then "live" else "INERT" end), ($n|tostring) ] | @tsv
' | sort -t$'\t' -k3,3 -k2,2 | column -t -s$'\t'
```

`jq` gotcha that cost time twice: `$consumed | index(.id)` pipes the **array** as input to
`.id`. Bind the field first (`.id as $wid | … index($wid)`).

### Group A — 4 that mislabel deacon's spec-conformance as a deliberate deviation

`spec: conformant` + `intentional-divergence`. The spec says `features` is an object;
deacon rejects a bare string and the reference accepts it. **deacon follows the spec and
the reference is lenient** — filing that as "we deliberately differ" understates us.

| Waiver | Enforced? |
|---|---|
| `wvr-unsupported-enum-values` | live |
| `wvr-upgrade-empty-feature-set` | live |
| `wvr-wrong-type-features` | live |
| `wvr-wrong-type-forwardports` | live |

**Action, no ruling needed:** change each behavior's `decision` to `follow-spec` (R5 is
satisfied — the spec axis is already `conformant`). The waiver itself stays, because it is
what keeps the live-differential case from failing; only its *meaning* changes from "we
chose to differ" to "the reference deviates from the spec."

This was blocked until #426: the page rendered `follow-spec` + `reference: divergent` as
❌ *"differs, and we intend to fix it."* With the `spec`/`CLI` columns it reads `✔ ✘`,
which is correct. **Do this after #426 lands.**

### Group B — 3 tolerances that are load-bearing, not claims

`spec: conformant` + `follow-spec` + `reference: divergent`: deacon already follows the
spec, and the differential case would fail without the tolerance.

| Waiver | Enforced? |
|---|---|
| `wvr-discovery-named-subfolder` | live |
| `wvr-ruc-completed-hooks-not-rerun` | live |
| `wvr-outdated-extends-chain-features` | **INERT** — see below |

These expose an **instrument gap**: the waiver type is overloaded. It carries both "deacon
deliberately differs and we accept it" (needs your agreement) and "the reference deviates
from the spec and deacon is right" (a consequence of following the spec, needs nobody's).
`allowedDifferences` accepts a `waiverId` **or** a `divergenceId`, but `divergenceId` only
resolves to an `ext-`/intentional-divergence record, so a `follow-spec` behavior has no
option but a waiver. Worth a third instrument; not urgent.

`wvr-outdated-extends-chain-features` is inert *and* the nightly reports a real
`chan-stdout.table` difference on `case-outdated-extends-chain-differential` that its
tolerance is not absorbing — the scope does not match what actually diverges. Fix the
scope or fix deacon's `outdated` table output. Left alone deliberately: deleting the only
record naming a real difference needs the direction decided first.

### Group C — 8 that need your ruling, and each blocks a release

All `spec: unspecified` + `intentional-divergence`. The spec is silent, so deacon's
behavior is a genuine choice.

| Waiver | The choice | Enforced? | Cases |
|---|---|---|---|
| `wvr-malformed-json` | deacon rejects a hard JSONC syntax error; the reference parses on | live | 7 |
| `wvr-features-duplicate-in-one-document` | deacon rejects two keys resolving to one Feature | **INERT** | 1 |
| `wvr-outdated-malformed-lockfile-rejected` | deacon fails on an unparseable lockfile; the reference continues | **INERT** | 1 |
| `wvr-readconfig-authored-empty-omitted` | deacon omits an authored `null`; the reference emits it | live | 2 |
| `wvr-readconfig-merged-computed-empties` | deacon omits synthesized empties; the reference materializes them | **INERT** | **0** |
| `wvr-up-changed-config-recreates` | deacon recreates the container on config change | **INERT** | 1 |
| `wvr-compose-project-file-set` | which Compose files get composed, and the derived labels | live | 1 |
| `wvr-container-metadata-label-serialization` | byte form of the metadata label (whitespace / key order) | live | 2 |

**The consequence, stated up front.** R5 forbids `follow-spec` when the spec is silent and
R6 forbids `align-with-reference` when the reference differs, so removing one of these
leaves `unresolved-gap` as the *only* remaining disposition → a `gap-` record → **`certify`
blocks until deacon is fixed or you agree to the waiver.** That is arguably the correct
pressure, but it means "delete these eight" is a decision to block releases, not a cleanup.

A read to argue with: the first three are the constitution-IV "strict on the developer's
mistakes" principle already endorsed, and are the likeliest to survive discussion. The last
two are byte-level formatting differences where fixing deacon looks better than waiving.
The middle three are open.

`wvr-readconfig-merged-computed-empties` is the odd one: **zero cases**, so it is the sole
coverage for `bhv-readconfig-merged-computed-empties-omitted`. Authoring its case does
double duty — first coverage for that behavior AND makes its waiver live.

## Next actions, in order

1. Retarget **#426** to `main`, merge **#425**, then merge #426.
2. **Group A** — flip the 4 behaviors to `follow-spec`, one small PR. No ruling needed.
3. **Group C** — bring the 8 to the user one at a time, smallest first.
4. **#423** — the remaining 5 inert waivers are enforced by nothing (`corpus_case` scope
   went dead when 023 US7 deleted the four corpus carriers). Each needs a case tolerating
   it via `allowedDifferences`, which is also what makes it self-invalidating again.
5. The unstarted third item from the original list: `bhv-exec-restored-path-ordering`
   (**#370** — exec drops the image ENV `PATH`; root-caused, needs a precedence decision).

## What this session established, worth not re-deriving

- **The recurring defect class is rendering a claim where a reader reads evidence.** Three
  PRs came out of one instance each: the headline counted recorded dispositions while the
  grid counted evidence (#422); a waiver id proved a record was written, not that anything
  re-checks it (#424); `❌` blamed deacon for the reference's deviation (#426). Look for
  this shape first in anything that summarises the registry.
- **Nothing verifies that a waiver is reachable.** `validate` checks its structure and
  `certify` checks its expiry, but no gate asks whether any runner reaches it. A
  structurally perfect waiver that no case tolerates passes every gate in the system
  (#423).
- **`no-counterpart` was unreachable for the whole life of the migration.** The existence
  check ran before the disposition match, so an entry naming a record the registry no
  longer has — which is what the disposition *means* — was self-refuting. Zero instances
  ever. Fixed in #425, which also added `retired` for a record withdrawn on purpose.
- **A test asserting over the whole rendered page is vacuous when the legend contains the
  token being tested.** Hit twice. Scope assertions to `rows_of(&render(&reg))`.
- **Verify every new guard fails without its fix.** Both new guards in #425/#426 were
  checked that way, and it caught a test whose own scaffolding tripped an unrelated rule.
