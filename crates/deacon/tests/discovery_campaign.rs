//! Live discovery campaign binary (025-exploratory-parity-discovery).
//!
//! **Selected ONLY by `[profile.discovery]`.** Every other profile — `default`,
//! `dev-fast`, `full`, `ci`, `mvp-integration`, `parity` — excludes it in its
//! `default-filter`, so those lanes are truthful by *non-selection*: a green pull-request
//! run never implies a campaign ran (FR-055/FR-057).
//!
//! ## Why this file exists at Phase 2 rather than at US1
//!
//! The lane wiring (T006/T007/T121) and this binary (T040) are mutually dependent:
//! nextest **hard-fails** a whole config when a `binary(=NAME)` predicate names a binary
//! that does not exist, so wiring the allow-list before the file exists would break every
//! lane in the workspace, not just this one. The file therefore lands with the wiring,
//! carrying exactly the verification T040 asks for — *"verify it is selected by
//! `[profile.discovery]`'s allow-list and excluded from all six other profiles"*.
//!
//! The campaign acceptance tests (seed reproduction, the trivial-failure ceiling,
//! mutation-category coverage, the oracle fail-loud path, budget exhaustion, the
//! admission cap) land here in **T025–T029**, **T067**, **T101–T103**, **T123**, and
//! **T124** as additional test functions.

/// The environment variable nextest sets to the profile it selected the run under.
const NEXTEST_PROFILE: &str = "NEXTEST_PROFILE";

/// The one profile permitted to select this binary.
const DISCOVERY_PROFILE: &str = "discovery";

/// If this binary is running under a nextest profile at all, that profile must be
/// `discovery`.
///
/// This is the lane-isolation invariant enforced from **inside** the thing being isolated,
/// which is a stronger claim than the config cross-check in `discovery_hermetic`: that one
/// asserts the filters *say* the right thing, this one asserts the binary *was not
/// actually selected* by anything else. A future edit that widened a pull-request lane's
/// filter would fail here even if the config still parsed and the cross-check still passed
/// against some other reading of it.
///
/// An absent `NEXTEST_PROFILE` means the binary was run directly (`cargo test --test
/// discovery_campaign`), which is a deliberate developer act rather than a lane, and is
/// allowed. It is **not** a silent skip: the assertion below has nothing to assert about a
/// run that no profile selected.
#[test]
fn this_binary_runs_only_under_the_discovery_profile() {
    let Some(profile) = std::env::var_os(NEXTEST_PROFILE) else {
        // Run outside nextest: no profile selected it, so there is no selection to check.
        return;
    };
    let profile = profile.to_string_lossy().into_owned();
    assert_eq!(
        profile, DISCOVERY_PROFILE,
        "a live discovery campaign binary was selected by [profile.{profile}]. Only \
         [profile.{DISCOVERY_PROFILE}] may select it — every other lane must be truthful by \
         non-selection, so that a green pull-request run never implies a campaign ran \
         (FR-055/FR-057). Fix the profile's `default-filter` in .config/nextest.toml."
    );
}
