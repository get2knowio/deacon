//! Exploratory parity discovery — the **live** half
//! (025-exploratory-parity-discovery, plan.md § Project Structure).
//!
//! Everything that touches the pinned oracle, Docker, or the network lives here; the
//! pure data-to-data logic (grammar, generation, mutation, reduction strategy,
//! signature, queue, relations, reports) lives in `deacon_conformance::discovery`. This
//! is the 022 hermetic/live split applied unchanged (research D4), and it is what keeps
//! the hermetic half runnable in the fast lane with no external dependency.
//!
//! ## What this half must not re-implement
//!
//! - **Process execution** — reuse [`crate::exec`]; a second execution path would be a
//!   second set of bounds, captures, and failure modes.
//! - **Oracle resolution and exact-version verification** — reuse [`crate::oracle`] and
//!   [`crate::prereq`]; a missing or mismatched oracle fails loudly, never silently
//!   (FR-003).
//! - **Normalization** — reuse [`crate::normalize`]. FR-015 permits exactly one
//!   normalization definition, and a signature computed from independently re-diffed
//!   values would be a second opinion on what differs, able to disagree with the one the
//!   comparison used (research D3).
//! - **Injection** — reuse [`crate::inject`]'s sealed `EvidenceSource` boundary, which
//!   already makes injecting into an observer's *return* value fail to compile
//!   (research D7).
//!
//! ## Exit-status discipline
//!
//! Every discovery command's exit status reflects **whether it ran**, never **what it
//! found** (contracts/discovery-cli.md, FR-058). A campaign that finds forty differences
//! exits `0`; a campaign that cannot verify the oracle exits non-zero. The single
//! exception is `discovery-proof`, whose status asserts a property of the *machinery*.
//!
//! ## Module map
//!
//! | Module | Owns |
//! |---|---|
//! | [`campaign`] | the driver: seed, tier, budget, per-candidate timeout, admission cap, outcome accumulation |
//! | [`differential`] | deacon vs the verified pinned oracle over one candidate |
//! | [`metamorphic_run`] | deacon-only relation evaluation (needs no oracle, Docker, or network) |
//! | [`minimize`] | supplies the live reproduction predicate to the hermetic shrinker |
//! | [`candidate`] | assembles the reviewable candidate under `target/discovery/candidates/` |
//! | [`corpus_fetch`] | network-lane fetch + content-digest verification |
//! | [`pipeline_proof`] | the injected-difference traversal proof |

pub mod campaign;
pub mod candidate;
pub mod corpus_fetch;
pub mod differential;
pub mod metamorphic_run;
pub mod minimize;
pub mod pipeline_proof;
