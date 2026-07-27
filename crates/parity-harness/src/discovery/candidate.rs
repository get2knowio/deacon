//! Reviewable-candidate assembly under `target/discovery/candidates/<fnd-id>/`
//! (025-exploratory-parity-discovery, data-model.md § 9, US2).
//!
//! Six parts, all required for the candidate to be self-contained (FR-024/FR-027): the
//! minimal `fixture/` tree, `context.json`, `raw.json`, `normalized.json`,
//! `provenance.json`, and `mapping.json`. `raw.json` and `normalized.json` are
//! **separate** files, mirroring the committed-snapshot layout — raw and normalized
//! evidence must never be conflated (FR-014, the FR-016 precedent from 022).
//!
//! `mapping.json` carries either a resolvable `bhv-` id or an explicit
//! `{"match": "none"}`; it never invents an id (FR-025), because a suggestion that
//! fabricates a behavior identity turns the reviewer's job into verifying a
//! plausible-looking id rather than deciding one.
//!
//! Deliberately empty at Phase 2: this module is filled by **T049**–**T051**.
