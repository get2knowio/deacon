//! Real-world corpus manifest (`cor-`) — `conformance/discovery/corpus.json`
//! (025-exploratory-parity-discovery, data-model.md § 8, US7).
//!
//! The manifest is Rust-owned strict JSON rather than a Python tuple so the
//! immutable-reference check (**D4**) runs **hermetically**, on every pull request,
//! without network access: a validation that only runs when the network is up is a
//! validation that does not run (research D8). Corpus *content* is never vendored — this
//! file records provenance, not bytes.
//!
//! Deliberately empty at Phase 2: this module is filled by **T105**/**T106**.
