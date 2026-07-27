//! The eleven-category mutation operator catalogue
//! (025-exploratory-parity-discovery, data-model.md § 5, US1).
//!
//! The catalogue lives in code rather than as a data file because each operator is
//! executable logic; `mutationCatalogVersion` (one of the seven pinned-input-set
//! elements) pins its identity. Every application records its `mop-<name>` on the
//! witness (FR-009), which is what lets a candidate name the operators that produced it
//! and what lets shrinking un-apply one operator as a reduction step (research D5).
//!
//! Deliberately empty at Phase 2: this module is filled by **T031**/**T032**.
