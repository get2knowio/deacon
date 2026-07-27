//! The injected-difference pipeline proof
//! (025-exploratory-parity-discovery, US5, FR-042a).
//!
//! Injects a known difference through `crate::inject::perturb_source` — the existing
//! **sealed** `EvidenceSource` boundary — and requires it to traverse generation →
//! comparison → minimization → candidate → classification → promotable. Reusing that
//! boundary inherits the property this proof cannot easily re-establish: injecting into
//! an observer's *return* value does not compile, so the proof can never assert on data
//! it planted past the part under test (research D7).
//!
//! It also inherits `InjectionInapplicable`, which matters as much: a perturbation that
//! never landed must fail loudly rather than be counted as "the pipeline found nothing".
//!
//! Deliberately empty at Phase 2: this module is filled by **T081**/**T082**.
