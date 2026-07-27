//! Campaign driver: seed, tier, budget, per-candidate timeout, admission cap, and
//! outcome accumulation (025-exploratory-parity-discovery, US1/US2/US6/US7).
//!
//! The driver owns the four tiers and their prerequisites (research D10): `metamorphic`
//! needs nothing external, `config-differential` is the nightly scheduled tier,
//! `container-differential` is invoked-only, and `corpus` is the weekly network-backed
//! tier. Budgets are per-tier rather than shared, because sharing lets the slow tier
//! starve the fast one — and the fast tier is where nearly all the exploration happens.
//!
//! Deliberately empty at Phase 2: this module is filled by **T033**/**T037**/**T052**
//! (US1/US2), **T071** (the admission cap), **T096** (the metamorphic tier), and
//! **T108** (the corpus tier).
