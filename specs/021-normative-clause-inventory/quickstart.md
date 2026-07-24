# Quickstart: Normative Clause Inventory

Developer walkthrough for the prose-clause machinery this feature adds to the dev-only
`deacon-conformance` crate (the companion to 020's schema-constraint inventory).
Everything below is offline and LLM-free except the two out-of-band human steps
(vendoring a revision; optionally LLM-assisted clause proposal), which never run in CI.

## Everyday commands

```bash
# Canonicalize the committed clause inventory from the vendored pinned prose
# (recomputes ids/fingerprints, verifies each excerpt is present at its heading)
cargo run -p deacon-conformance -- clause generate

# Verify the committed inventory is exactly what canonicalization produces (CI does this too)
cargo run -p deacon-conformance -- clause check

# Full registry validation — now includes the clause/classification join (V11–V15)
cargo run -p deacon-conformance -- validate

# Emit skeleton classification records for anything unclassified
cargo run -p deacon-conformance -- clause scaffold > /tmp/clc-skeletons.json

# Compare two inventory revisions (drift review — moves are first-class)
cargo run -p deacon-conformance -- clause diff old-clauses.json conformance/inventory/clauses.json --format md
```

Hermetic tests: `cargo nextest run -E 'binary(=clause_extraction) + binary(=clause_determinism) + binary(=clause_baseline) + binary(=clause_diff) + binary(=clause_classification_join)'`
(all in the fast lanes; no Docker, no network, no model).

## Authoring / proposing clauses (out-of-band; never in CI)

1. An LLM-assisted or manual pass reads a vendored document and drafts clause records
   (excerpt + strength + testability + locations) into `conformance/inventory/clauses.json`.
   A multi-requirement paragraph becomes several records; descriptive text is labeled
   `descriptive`/`informative`; unclear language is labeled `ambiguous` — **never** promoted
   to `must`.
2. `clause generate` canonicalizes: it fills/recomputes `id` + `fingerprint`, merges
   same-substance records, and **fails loudly** if an excerpt is not present under its
   heading or a strength label contradicts the source keywords. Fix the draft until it
   passes.
3. Review the plain JSON diff in the PR. The proposal tool is an authoring aid — it is never
   a dependency of `generate`/`check`/`validate`/`diff`/`certify`.

## Classifying a clause

1. Find the clause in `conformance/inventory/clauses.json` (or via a V12 violation from
   `validate`).
2. Add ONE record to `conformance/registry/clause-classifications/<doc>.json`:
   - Consumer clause, tested → `behavior-mapped` + the `bhv-` id(s) (several clauses MAY
     share one behavior).
   - Consumer clause, no behavior yet → create/extend the behavior first (or, honestly,
     leave it unclassified — it blocks, which is the point; research Decision 11).
   - Descriptive/informative → `non-testable` + rationale.
   - Whole authoring document → ONE document-scope `not-applicable` record in `authoring.json`
     (covers every non-`ambiguous` clause of that document).
   - `ambiguous` clause → resolve it: a per-clause record is REQUIRED (a document-scope
     default never covers ambiguity).
3. `validate` until clean. Never delete a clause to go green — units are machine-canonicalized.

## Re-vendoring on an upstream pin bump (the drift workflow)

1. **(network, one-time, human)** Download the ratified `docs/specs/` Markdown at the new
   commit into `conformance/spec/<newpin>/`; compute `sha256sum` per file; write the new
   `manifest.json` (with `scope` per document); add the `rev-spec-<newpin>` revision record.
2. Re-author/adjust affected clause excerpts, then `clause generate` → the committed
   inventory re-canonicalizes under the new revision.
3. `clause diff` old vs new → human-readable review document (new / removed / **moved** /
   changed / non-material).
4. `validate` now enumerates the exact review queue: V11 = stale classifications to
   delete/re-point, V12 = new/changed clauses to classify. **Moves carry their disposition
   over — no re-review.** Nothing inherits a disposition by wording similarity; `certify`
   blocks until the queue is empty.
5. Classify, delete stale records, commit everything in one PR.

## Guard rails to remember

- `conformance/inventory/clauses.json` is canonicalized — hand edits that change substance
  without valid provenance are detected (V14/V15) and rejected in CI.
- Clause-classification files are hand-authored — `clause generate` never touches them.
- No command fetches the network or invokes an LLM. Ambiguity is surfaced (V12), never
  silently made strict.
- Fixture prose for the acceptance paths (multi-requirement paragraphs, moved headings,
  ambiguity, authoring-scope) lives under `fixtures/conformance/prose/` — extend those, not
  the vendored pinned copies, which are byte-exact upstream artifacts and never edited.
- Identity is substance-anchored: a moved heading keeps the clause id (and disposition); a
  reworded obligation mints a new id and forces review.
