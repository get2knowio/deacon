//! Constrained candidate generation from the pinned grammar
//! (025-exploratory-parity-discovery, US1).
//!
//! Draws from [`super::grammar`] so that `required` keys are satisfied for **valid**
//! candidates and violated deliberately for **near-valid** ones — the distinction the
//! `required` constraint kind exists to make (research D1).
//!
//! Deliberately empty at Phase 2: this module is filled by **T030**. The skeleton exists
//! now so the module map in [`super`] is complete and the later task has a home rather
//! than a naming decision. Nothing imports it yet.
