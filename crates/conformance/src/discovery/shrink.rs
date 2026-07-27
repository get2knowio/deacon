//! Structural delta-debugging over the parsed configuration document
//! (025-exploratory-parity-discovery, data-model.md § 6, US2).
//!
//! Reduction never happens at the byte or line level: text-level ddmin on JSON produces
//! syntactically broken intermediates that cannot reproduce a signature living past
//! parsing, and each one costs a full oracle invocation to discover (research D5).
//!
//! The reproduction predicate is a **parameter**, not a call into the oracle, so the
//! reduction *strategy* stays hermetic and unit-testable against a synthetic predicate
//! while the live campaign supplies the real one (`parity_harness::discovery::minimize`).
//! Without that split the shrinker could only be tested by running a campaign.
//!
//! Deliberately empty at Phase 2: this module is filled by **T041**–**T047**.
