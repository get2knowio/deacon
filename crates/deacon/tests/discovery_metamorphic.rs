//! Live metamorphic-relation binary (025-exploratory-parity-discovery).
//!
//! **Selected ONLY by `[profile.discovery]`**, like `discovery_campaign`. Its exclusion
//! from the pull-request lanes is about **stochasticity, not cost**: this tier needs
//! neither the pinned oracle, nor Docker, nor the network (research D12), so it is cheap
//! enough to run anywhere — and FR-055 is absolute regardless. A lane whose result varies
//! run to run cannot be a gate.
//!
//! That cheapness is load-bearing elsewhere, though: it makes this the only complete
//! vertical slice a contributor with no devcontainer CLI installed can develop and test
//! locally, which is why research D12 recommends building it first.
//!
//! ## Why this file exists at Phase 2 rather than at US6
//!
//! Same reason as `discovery_campaign`: nextest **hard-fails** a whole config when a
//! `binary(=NAME)` predicate names a binary that does not exist, so the allow-list
//! (T006/T007/T121) cannot land before the file does. It carries exactly the verification
//! T097 asks for — *"verify it is selected by `[profile.discovery]` and excluded from all
//! six other profiles"*.
//!
//! The relation evaluations (formatting / comment / key-order invariance, path
//! relocation, lifecycle equivalence, declaration-order sensitivity, and the inert-relation
//! proof) land here in **T084–T087**, **T090**, and **T127**.

/// The environment variable nextest sets to the profile it selected the run under.
const NEXTEST_PROFILE: &str = "NEXTEST_PROFILE";

/// The one profile permitted to select this binary.
const DISCOVERY_PROFILE: &str = "discovery";

/// If this binary is running under a nextest profile at all, that profile must be
/// `discovery`.
///
/// See `discovery_campaign`'s counterpart for why the invariant is asserted from inside
/// the binary as well as against the config: this catches a lane that actually selected
/// the binary, not merely a config that reads as though it would not.
#[test]
fn this_binary_runs_only_under_the_discovery_profile() {
    let Some(profile) = std::env::var_os(NEXTEST_PROFILE) else {
        // Run outside nextest: no profile selected it, so there is no selection to check.
        return;
    };
    let profile = profile.to_string_lossy().into_owned();
    assert_eq!(
        profile, DISCOVERY_PROFILE,
        "the live metamorphic binary was selected by [profile.{profile}]. Only \
         [profile.{DISCOVERY_PROFILE}] may select it — the exclusion is about \
         stochasticity, not cost, so \"this tier is cheap\" is never a reason to admit it \
         to a pull-request lane (FR-055/FR-057). Fix the profile's `default-filter` in \
         .config/nextest.toml."
    );
}
