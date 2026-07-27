//! Network-lane corpus fetch with content-digest verification
//! (025-exploratory-parity-discovery, US7).
//!
//! The only network-touching code in the feature. A digest is recorded on **first**
//! materialization and verified on every later fetch (FR-051); a mismatch fails that
//! entry loudly rather than comparing against unexpected content, and an unreachable
//! entry is reported as unreachable rather than as "ran and found nothing" (FR-052).
//!
//! Deliberately empty at Phase 2: this module is filled by **T107**.
