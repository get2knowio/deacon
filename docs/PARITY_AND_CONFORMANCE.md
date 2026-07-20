# Parity and Conformance: How deacon Knows It's Correct

deacon is a reimplementation. Someone else defined what it should do, and someone
else already built a tool that does it. That shapes everything about how we verify
correctness — and it's why this repo has machinery that a from-scratch project
wouldn't need.

This document explains that machinery: what the pieces are, what the words mean,
and what to actually do when you find a difference.

**Read this first if** you've seen the words "parity", "conformance", "divergence",
"gap", or "waiver" in a PR and weren't sure whether they meant the same thing.
(They don't.)

---

## 1. Why any of this exists

Two facts drive the whole design.

**Fact one: there is an authority.** The [containers.dev
specification](https://containers.dev) defines what a devcontainer tool must do.
We pin it — commit `113500f4` — so "the spec" means one exact document, not a
moving target.

**Fact two: there is an incumbent.** Microsoft's
[`@devcontainers/cli`](https://github.com/devcontainers/cli) is what people
actually use today. We pin it too — version `0.87.0`. We call it **the oracle**,
and it matters independently of the spec, because users will compare deacon
against *it*, not against a document. If the spec is ambiguous and the reference
picked an interpretation, users expect deacon to pick the same one.

So "is deacon correct?" is really two questions:

- Does it match **the spec**? (the normative question)
- Does it match **the reference**? (the practical question)

These can disagree. The spec might say one thing while the reference does another.
deacon might deliberately do something neither does. **Every mechanism in this
document exists to keep those questions separate and answerable.**

---

## 2. The four mechanisms

Four different things verify deacon. They get confused constantly — especially the
first two, because both involve the word "parity."

| | What it is | What it answers | Where |
|---|---|---|---|
| **Parity harness** | Runs deacon and the oracle on the same input, compares output | "Do we *differ* from the reference, right now?" | `crates/parity-harness/` |
| **Conformance registry** | Hand-authored records of known behavior | "Is that difference *known and accounted for*?" | `conformance/` |
| **Canaries** | Shell scripts running the real CLI against real Docker | "Does the thing work end to end?" | `examples/*/exec.sh` |
| **Unit & integration tests** | Ordinary Rust tests | "Is this function right?" | `crates/*/tests/`, `#[cfg(test)]` |

The critical distinction:

> **The harness *finds* differences. The registry *explains* them.**

The harness is a detector — it produces raw signal, no judgement. The registry is
a ledger — it's where a human writes down what a difference *means* and what we
intend to do about it. One is automated and dumb; the other is manual and
opinionated. **They are separate systems that happen to share a word.**

Canaries are the humblest mechanism and are worth taking seriously anyway. They're
shell scripts that run the real binary against real Docker and check real output.
In a recent sweep, the elaborate schema-analysis machinery found zero product bugs
while a canary that pins a feature by digest and greps for it found a real one
that shipped. Blunt instruments catch things sharp ones miss.

---

## 3. Vocabulary

These four words carry precise meanings. Getting them wrong leads to real mistakes
— filing work that doesn't exist, or hiding work that does.

### Divergence — a difference we have *characterized*

We know what deacon does, what the reference does, and **why**. It's backed by
evidence: a test case or a waiver. Two flavors:

**Fix-flavored** — deacon is wrong and we want parity.
> Decision: `follow-spec` or `align-with-reference`. Gets a GitHub issue labeled
> `parity-drift` and a fix.

**Intentional** — we deliberately differ, and we're keeping it.
> Decision: `intentional-divergence` (backed by a waiver) or `deacon-extension`
> (a capability the reference lacks). Never blocks anything.

### Gap — an admission of missing knowledge

We *don't know* what the reference does, or we haven't done the work to find out.
No evidence stands behind it.

> `reference: unknown` → `decision: unresolved-gap` → a `gap-*` record.
>
> **A gap always blocks release.** That's the entire point. You cannot certify
> around a gap; you either resolve it (add a real test case, delete the gap record
> in the same change) or it stays a blocker. This is deliberate pressure against
> the temptation to shrug and ship.

### Waiver — a characterized difference with an expiry

A waiver says: *"we know we differ here, here's why, and it's fine."* It needs a
`rationale`, an `added` date, and an `expires` date.

Waivers are **self-invalidating**: if the difference stops reproducing, the waiver
fails as *stale* and must be removed. You can't leave dead waivers lying around
pretending to explain something that no longer happens.

### Out of scope — recorded nowhere

Differences with **no observable effect** (stdout, stderr, exit code, container
state, filesystem), or with **no reference equivalent at all**.

> These get recorded **nowhere**. Not as a gap, not as a waiver, not as a behavior.
>
> This is easy to get wrong in the direction of over-recording. A real example:
> deacon writes internal lifecycle marker files. The reference has no concept of
> them. Someone once filed that as a gap — it sat there blocking, describing
> nothing. Out-of-scope means *silence*, not a record saying "n/a".

### The trap: divergence vs. gap

| | Divergence | Gap |
|---|---|---|
| Do we know what the reference does? | **Yes** | No |
| Is there evidence? | Yes — case or waiver | No |
| Does it block release? | No | **Yes, always** |
| What it says | "We differ, and here's why" | "We haven't done the work" |

Calling a gap a divergence hides work. Calling a divergence a gap blocks a release
for no reason. **The dividing line is evidence**, not confidence — "I'm pretty
sure the reference does X" is a gap until a test proves it.

---

## 4. The conformance registry

`conformance/registry/` holds hand-edited strict-JSON records. This is the durable
memory: what we know about deacon's behavior and why.

### The three-axis disposition

Every **behavior** record carries three independent axes. This is the heart of the
model, and the reason it works:

```json
{
  "id": "bhv-readconfig-wrong-type-features-rejected",
  "area": "read-configuration",
  "statement": "A `features` value that is not an object is rejected.",
  "spec": "conformant",              // ← how do we relate to the SPEC?
  "reference": "divergent",          // ← how do we relate to the ORACLE?
  "decision": "intentional-divergence"  // ← what do we INTEND to do?
}
```

Three axes rather than one status, because a single field can't express *"we
follow the spec, the reference doesn't, and we're keeping it that way."* That
sentence is a legitimate and common state — and it's unrepresentable if you only
have "pass/fail".

There is deliberately **no "different but acceptable" status**. Acceptability is
the `decision` axis's job. The first two axes are facts; the third is a choice.
Keeping facts and choices in separate fields is what stops the record from
becoming an opinion dressed as an observation.

The axes can't contradict each other arbitrarily — rules **R1–R8** in
[`conformance/RULES.md`](../conformance/RULES.md) enforce coherence, and
`validate` checks them mechanically.

### What else lives there

| File | Holds | Now |
|---|---|---|
| `behaviors/*.json` | One record per verified behavior, three axes | 24 |
| `cases.json` | Test cases proving behaviors — names a real test binary | 24 |
| `waivers/wvr-*.json` | Characterized differences, with expiry | 10 |
| `gaps.json` | Admissions of missing work | **0** |
| `extensions.json` | Capabilities the reference lacks | 6 |
| `revisions.json` | The pins: spec, schema, oracle, cli-surface | 4 |
| `dimensions.json` | Axes of variation: os, arch, runtime, oracle | 4 |
| `channels.json` | What counts as observable: stdout, exit code, … | 6 |
| `profiles.json` | Which context we certify against | 1 |

Two of these deserve a note:

**Channels** define what "observable" means — `chan-stdout`, `chan-stderr`,
`chan-exit-code`, `chan-container-state`, `chan-filesystem`, `chan-file-content`.
If a difference isn't visible on one of these, it's out of scope by definition.
This is what makes "no observable effect" a checkable claim rather than a
judgement call.

**Dimensions and profiles** capture that correctness is context-dependent.
`dim-os` × `dim-arch` × `dim-runtime` × `dim-oracle` defines a space; a profile
picks one point in it. Today we certify exactly one:
`prof-linux-amd64-docker-0870`. A behavior verified on Linux/Docker isn't
automatically claimed for Windows/Podman — and the model makes that honest instead
of implicit.

### Extensions: where deacon is deliberately *more*

Six capabilities the reference doesn't have — auto-forward ports, host CA
injection, user profiles, workspace-trust gating, `extends` chain resolution, and
`.env`-format secrets files.

These exist so deliberate differences never get misreported as drift. Without
`ext-extends-resolution`, every `extends` test would look like a parity failure
forever. The extension record says *"this difference is a feature"* — once, in one
place, instead of in a comment on every affected test.

---

## 5. The two gates

**This is the single most confusing thing in the repo.** Two gates, similar names,
completely different consequences.

### `parity / live-certification` — finds drift, does NOT block release

- Runs the nine live parity binaries against the pinned oracle
- Needs Docker and a real npm install of `@devcontainers/cli@0.87.0`
- Triggers on PRs touching parity paths, plus nightly
- **`release.yml` never runs it**

> **A red parity lane does not block a release.** It's a signal, not a gate.

### Conformance `certify` — the actual release gate

- Wired into the `verify` job of `.github/workflows/release.yml`
- Fails if **any gap record exists**, or any in-profile behavior is uncovered, or
  any inventory violation (V11–V14) fires
- Waivers, `not-applicable`, and `non-testable` are listed but **never block**

> **This is the only conformance gate in the release path.** Keep the registry
> gap-free, or a release will be blocked at exactly the wrong moment.

The rule that protects all of it:

> **Never make `certify` non-blocking, and never delete a real gap to go green.**
> That is the one move the entire model exists to prevent.

### Why the harness can't be the release gate

It needs Docker, network, and a specific npm package. It's slow and
environment-sensitive. Gating releases on it would mean either flaky releases or
pressure to weaken the check. So the harness *informs* and the registry *decides*
— the registry is a set of committed files, so the gate is fast, hermetic, and
deterministic.

### Truthful non-selection

The nine live parity binaries run **only** under `cargo nextest run --profile
parity`. Every other profile excludes them explicitly.

This is deliberate: a green fast-lane run never *implies* live parity ran. There's
no silent skip anywhere — a missing oracle, missing Docker, or a normalization
failure **fails loudly** rather than passing vacuously. A test that skips quietly
is worse than no test, because it manufactures false confidence.

---

## 6. The build-out loop

When the harness surfaces a difference, or you notice one by hand:

**Step 1 — Classify it.**
> Divergence (which flavor?), gap, or out-of-scope? If out-of-scope: **record
> nothing** and stop.

**Step 2 — Record it in the registry.**
> Add or extend a behavior with all three axes, link its source unit, cover it
> with a case, waiver, or gap. Run `validate`.

**Step 3 — If it's fix-flavored, also file a GitHub issue** (`parity-drift` label)
and cross-link it from the behavior's `notes`.
> **Both, cross-linked.** The issue is the *task*; the registry is the
> *characterization*. They answer different questions and neither replaces the
> other.

**Step 4 — Fix or waive.**
> Fixing a gap means adding a real case *and deleting the gap record in the same
> change*. Accepting a divergence means a waiver with a rationale and an expiry.

Conventions for the work itself:

- **One small CI-gated PR per step.** Conventional-Commit title —
  `feat`/`fix`/`chore`, never `test`/`style` (CI rejects those).
- **A new live parity binary** must be registered in
  `fixtures/parity-corpus/registry.json` **and** get nextest overrides in **all**
  profiles, or the hermetic `parity_registry_check` fails.
- **Keep it fail-loud.** No `#[ignore]`, no silent skip.

---

## 7. Worked examples

Real records from this repo.

### A: intentional divergence — we're stricter than the reference

`devcontainer.json` has `"features": "not-an-object"`. The reference accepts it
and echoes it back. deacon rejects it.

Is deacon wrong? No — it's a deliberate choice from the project constitution
("strict on mistakes"). A typo'd config should fail loudly, not silently do
nothing.

```json
// behaviors/read-configuration.json
{ "id": "bhv-readconfig-wrong-type-features-rejected",
  "spec": "conformant", "reference": "divergent",
  "decision": "intentional-divergence" }
```
```json
// waivers/wvr-wrong-type-features.json
{ "expect": { "kind": "deacon-stricter" },
  "rationale": "features is a bare string instead of a map. deacon now rejects
    this (type-strict, matching forwardPorts)… The reference keeps the raw JSON
    and accepts.",
  "added": "2026-07-19", "expires": "2027-01-19" }
```

Note the expiry. In January 2027 someone must re-justify this, or it lapses. And
if the reference ever becomes strict too, the waiver fails as *stale* and gets
removed. The record can't rot silently.

### B: extension — we do something the reference can't

deacon resolves `extends` chains. The reference at v0.87.0 doesn't implement the
proposal at all.

Recorded as `ext-extends-resolution`, so every `extends` behavior is understood as
capability rather than drift. Without it, these would look like permanent parity
failures and someone would eventually "fix" them by removing the feature.

### C: out of scope — recorded nowhere

deacon writes lifecycle marker files under `.devcontainer-state/`. The reference
has no such concept.

No observable difference on any channel; no reference equivalent. **Nothing is
recorded.** This was once mis-filed as a gap (issue #117) where it sat blocking and
describing nothing. Deleting that record was the fix.

### D: a real bug, found by the bluntest tool

A canary pins a feature by digest — `git:1@sha256:<hex>` — and greps the resolved
config for that digest. It failed.

The parse was fine. The *render* was lossy: `reference()` rejoined name and
version with `:`, producing `…/git:sha256:<hex>`, which re-parsed as name
`git:sha256` + tag `<hex>`, and the manifest request 404'd.

Fixed by joining digests with `@`. The lesson worth keeping: the parse had unit
tests and they all passed. Nothing tested render→parse as a **round-trip
property**. A shell script asserting an end-to-end fact caught what typed tests
missed — which is exactly why canaries earn their keep.

---

## 8. The schema constraint inventory

The newest layer, and the one whose purpose is least obvious.

Everything above verifies behaviors *we thought to check*. The inventory asks a
different question: **what does the spec actually require, in total?**

It machine-extracts every constraint from the two pinned containers.dev JSON
schemas — 609 units at the current pin — and forces each one to carry exactly one
human-authored disposition:

| Disposition | Meaning |
|---|---|
| `behavior-mapped` | We have a behavior record covering this |
| `non-testable` | Descriptive only (a `title`, a `description`) |
| `not-applicable` | Outside deacon's consumer-only scope |

The point is **coverage against an enumerated denominator**. Without it, "we
verified 24 behaviors" has no meaning — 24 out of what? With it, every constraint
in the spec's schema is either verified or explicitly, accountably dismissed.

The distribution is itself informative: the feature-authoring schema classifies
161-of-206 `not-applicable`, because deacon implements only the *consumer* surface.
That asymmetry is a scope boundary made visible rather than assumed.

Two invariants keep it honest:

- **`conformance/inventory/constraints.json` is machine-owned.** Hand edits are
  detected and rejected. It's regenerated from the vendored schemas and
  byte-compared.
- **Classifications are hand-authored** and generation never touches them. On a
  pin bump, nothing inherits a disposition by name — new and changed units surface
  as an explicit review queue, and `certify` blocks until it's empty.

```bash
cargo run -p deacon-conformance -- inventory generate   # rebuild from schemas
cargo run -p deacon-conformance -- inventory check      # verify committed == regenerated
cargo run -p deacon-conformance -- inventory diff A B   # review drift on a pin bump
cargo run -p deacon-conformance -- inventory scaffold   # skeletons for unclassified
```

---

## 9. Common confusions

**"Parity harness and conformance registry sound like the same thing."**
> They share a word and nothing else. The harness is automated detection; the
> registry is human judgement. Different crates, different lifecycles, different
> gates.

**"The parity lane is red — is the release blocked?"**
> No. `release.yml` never runs it. Only `certify` blocks releases.

**"Can I add a gap to remember something later?"**
> Only if you accept it blocks every release until resolved. That's not a bug —
> it's what a gap *is*. For work you want tracked without blocking, use a GitHub
> issue.

**"Deacon does something the reference doesn't. Bug or feature?"**
> If deliberate and wanted → extension (`ext-`). If deliberate and narrow →
> intentional divergence + waiver. If accidental → fix-flavored divergence + a
> `parity-drift` issue. If invisible on every channel → out of scope, record
> nothing.

**"All the tests pass, so deacon is correct?"**
> They pass on one profile (`linux/amd64/docker/oracle-0.87.0`), for behaviors
> someone thought to write down. The inventory exists precisely because "tests
> pass" was an unquantified claim.

**"A canary is green — is that behavior verified?"**
> Only as far as that script's own assertions go. A canary checking just an exit
> code proves very little. Green means the script ran clean, not that the
> behavior is correct.

---

## 10. Where things live

```
conformance/
  RULES.md                    R1–R8, gap-vs-waiver, out-of-scope, V11–V14
  registry/
    behaviors/*.json          three-axis behavior records
    cases.json  waivers/  gaps.json  extensions.json
    revisions.json  dimensions.json  channels.json  profiles.json
    classifications/          hand-authored constraint dispositions
  inventory/constraints.json  MACHINE-OWNED — never hand-edit
  schemas/<pin>/              vendored upstream schemas + SHA-256 manifest

crates/
  conformance/                dev-only: validate / report / certify / inventory
  parity-harness/             dev-only: oracle, exec, normalize, waiver, report
  deacon/  core/              the actual product

crates/deacon/tests/parity_*.rs   9 live binaries + 2 hermetic guards
examples/*/exec.sh                canaries
examples/CANARY_STATUS.md         cross-session canary memory
```

### Commands

```bash
# Conformance (hermetic, fast — no Docker, no network)
cargo run -p deacon-conformance -- validate    # structural integrity, V1–V14
cargo run -p deacon-conformance -- report      # deterministic coverage report
cargo run -p deacon-conformance -- certify     # THE RELEASE GATE

# Live parity (needs Docker + the pinned oracle; not runnable in most devcontainers)
make test-parity

# Canaries
cargo build --release -p deacon
DEACON_BIN=$PWD/target/release/deacon bash examples/<area>/<name>/exec.sh
```

### Further reading

- [`conformance/RULES.md`](../conformance/RULES.md) — normative rules
- [`.specify/memory/constitution.md`](../.specify/memory/constitution.md) — the
  principles the decisions appeal to
- [`examples/CANARY_STATUS.md`](../examples/CANARY_STATUS.md) — canary state and
  protocol
- [`CLAUDE.md`](../CLAUDE.md) — the operational version of all this
