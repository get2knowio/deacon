//! Metamorphic relation (`mrl-`) records — `conformance/registry/metamorphic.json`
//! (025-exploratory-parity-discovery, data-model.md § 7, US6).
//!
//! Relations live **inside** the registry, unlike findings, because a relation is an
//! *assertion the project makes* — "reordering these keys must not change the result,
//! and here is the clause that says so" — and it references `clu-`/`bhv-` ids only the
//! registry loader can resolve (research D11). A finding, by contrast, is a *candidate*
//! for an assertion: machine-produced, unreviewed, possibly wrong, and structurally
//! unable to reach `certify`.
//!
//! Deliberately empty at Phase 2: this module is filled by **T091**–**T094**, which also
//! add violation classes **V31** (relation integrity) and **V32** (a mandated relation
//! family with no record) to [`crate::validate`].
