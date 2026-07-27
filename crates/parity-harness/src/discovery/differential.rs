//! Differential comparison of deacon against the verified pinned oracle over one
//! candidate (025-exploratory-parity-discovery, US1).
//!
//! Reuses `crate::exec`, `crate::oracle`, and `crate::prereq` — no new
//! process-execution path — and feeds `normalize::diff`'s existing `ConfigDivergence`
//! output into the hermetic signature derivation. Comparison relates **exit status and
//! structured content, never diagnostic message wording** (FR-016): two rejections that
//! differ only in phrasing are not a difference.
//!
//! Deliberately empty at Phase 2: this module is filled by **T034**–**T036** and
//! **T122**.
