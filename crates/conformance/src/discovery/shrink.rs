//! Structural delta-debugging over the parsed configuration document
//! (025-exploratory-parity-discovery, data-model.md § 6, US2).
//!
//! Reduction never happens at the byte or line level: text-level ddmin on JSON produces
//! syntactically broken intermediates that cannot reproduce a signature living past
//! parsing, and each one costs a full oracle invocation to discover (research D5).
//!
//! The reproduction predicate is a **parameter**, not a call into the oracle, so the
//! reduction *strategy* stays hermetic and unit-testable against a synthetic predicate
//! while the live campaign supplies the real one (`parity_harness::discovery::minimize`).
//! Without that split the shrinker could only be tested by running a campaign.
//!
//! ## What exists here now, and what US2 adds
//!
//! The **ordered catalogue** below is declared at US1 because it is one half of
//! `generatorVersion` — the seventh element of every campaign's pinned input set
//! (data-model.md § 4) — so a campaign cannot record its own provenance without it. The
//! reduction *strategy* that walks the catalogue lands with **T041**–**T047**; the live
//! predicate that feeds it lands in `parity_harness::discovery::minimize` (T048).
//!
//! Declaring the order here rather than restating it in the generator is deliberate: the
//! order is reproducibility-critical (FR-020 requires the same finding and seed to yield
//! the identical minimal input, and greedy reduction is order-sensitive), and two
//! statements of one order are two statements that can disagree.

/// The seven reduction steps, **in application order** (data-model.md § 6):
/// `drop-optional-key`, `un-apply-mutation`, `empty-collection`,
/// `collapse-extends-level`, `drop-compose-service`, `minimize-scalar`, `drop-feature`.
///
/// Ordered because greedy reduction is order-sensitive: the same finding and seed must
/// yield the identical minimal input (FR-020), and a different order is a different fixed
/// point. Reordering these names is therefore a pin change, not a refactor — which is why
/// the order belongs to `generatorVersion` rather than to `mutationCatalogVersion`, whose
/// subject is the mutation operator set and which would misdescribe it.
///
/// `isMinimal` is true only when all seven have been applied once with no step preserving
/// the signature — which is what makes FR-021's minimality claim finite and checkable
/// rather than an unfalsifiable assertion about all possible smaller inputs.
pub const REDUCTION_STEPS: [&str; 7] = [
    "drop-optional-key",
    "un-apply-mutation",
    "empty-collection",
    "collapse-extends-level",
    "drop-compose-service",
    "minimize-scalar",
    "drop-feature",
];

/// The revision of the catalogue's **order**, bumped whenever a step is added, removed,
/// or moved.
///
/// Distinct from the step names themselves: renaming a step for clarity does not change
/// which minimal input a finding reduces to, but moving one does.
pub const REDUCTION_CATALOGUE_VERSION: u32 = 1;

/// The reduction catalogue's identity, as it appears inside a campaign's
/// `generatorVersion` — the ordered step names plus the order's revision.
///
/// Spelling the order out rather than hashing it keeps the pinned input set readable: a
/// reviewer comparing two campaigns can see *which* step moved, which an opaque digest
/// would hide behind "the generator changed".
pub fn reduction_catalogue_identity() -> String {
    format!(
        "reduce[{}]/v{REDUCTION_CATALOGUE_VERSION}",
        REDUCTION_STEPS.join(",")
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_catalogue_declares_the_seven_steps_in_data_model_order() {
        assert_eq!(
            REDUCTION_STEPS,
            [
                "drop-optional-key",
                "un-apply-mutation",
                "empty-collection",
                "collapse-extends-level",
                "drop-compose-service",
                "minimize-scalar",
                "drop-feature",
            ],
            "the ORDER is part of `generatorVersion` (FR-020): reordering these changes \
             which minimal input every recorded finding reduces to, so it is a reviewed \
             pin change rather than a refactor"
        );
    }

    #[test]
    fn step_names_are_unique() {
        let mut names = REDUCTION_STEPS.to_vec();
        names.sort_unstable();
        let before = names.len();
        names.dedup();
        assert_eq!(before, names.len(), "two reduction steps share a name");
    }

    #[test]
    fn the_identity_names_the_order_rather_than_hiding_it_behind_a_digest() {
        let identity = reduction_catalogue_identity();
        assert!(identity.starts_with("reduce[drop-optional-key,"));
        assert!(identity.ends_with("]/v1"));
        for step in REDUCTION_STEPS {
            assert!(identity.contains(step), "{step} missing from the identity");
        }
    }
}
