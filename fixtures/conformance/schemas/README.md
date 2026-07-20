# Extractor test fixtures — `fixtures/conformance/schemas/`

These are **hand-authored fixture schemas** for the constraint-inventory extractor's
tests (feature `020-schema-constraint-inventory`). They exercise composition,
recursion, reference cycles, unresolved/external refs, malformed input, and empty
schemas — the acceptance cases spec FR-023 mandates.

**They are NOT vendored upstream artifacts.** The real, byte-exact, pinned schemas live
under `conformance/schemas/<rev-pin>/` (fingerprinted by that directory's
`manifest.json`) and are never edited. Fixtures here are authored for THIS feature's
tests only, may be arbitrary, and carry no upstream provenance.

Later phases (US1 extraction, US3 drift, US4 external sources) populate this directory
with the fixture schema documents and their sibling manifest fixtures. It is
intentionally empty except for this README until then.
