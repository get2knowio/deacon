//! Deacon-only metamorphic relation evaluation
//! (025-exploratory-parity-discovery, US6).
//!
//! This is the only discovery tier that needs **neither** the oracle **nor** Docker
//! **nor** the network (research D12), which makes it the cheapest complete vertical
//! slice through generation → comparison → signature → candidate. It also catches what
//! the differential structurally cannot: if deacon and the reference are *consistently*
//! wrong, the differential is clean and the defect is invisible, whereas a sensitivity
//! relation asserts the result **must** change and so fails on consistent wrongness.
//!
//! Deliberately empty at Phase 2: this module is filled by **T095**/**T127**.
