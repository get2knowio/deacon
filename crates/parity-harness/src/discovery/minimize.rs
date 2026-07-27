//! The live reproduction predicate supplied to the hermetic shrinker
//! (025-exploratory-parity-discovery, US2).
//!
//! `deacon_conformance::discovery::shrink` takes its predicate as a parameter precisely
//! so the reduction strategy can be unit-tested against a synthetic predicate; this
//! module is the real one — it runs both implementations over a reduced input and
//! reports whether the *signature* is preserved (research D4/D5).
//!
//! Deliberately empty at Phase 2: this module is filled by **T048**.
