//! Obligation (`obl-`) identity, generation, and disposition (`odp-`) resolution
//! (024-deterministic-conformance-coverage, contracts/obligation.md).
//!
//! This module currently owns **identity only** (T012/T013). Generation (T037/T038)
//! and disposition resolution (T061–T063) land later and build on the constructors
//! here.
//!
//! ## Identity is substance-anchored
//!
//! Following the `clu-` clause precedent, an obligation's id is derived from what it
//! *is*, never from where it sits:
//!
//! | Kind | Id | Hashed over |
//! |---|---|---|
//! | behavior | `obl-bhv-<hash8>` | `behavior ‖ canonical(context)` |
//! | combination | `obl-cmb-<hash8>` | `operation ‖ canonical(sorted assignment)` |
//!
//! The point of canonicalizing is that a **cosmetic** edit must not orphan a
//! hand-authored disposition. Reordering records, renaming a file, moving a dimension's
//! declaration position, or writing the same pair's keys in a different order all leave
//! the id alone. Changing what a combination *is* does change the id — and that is a new
//! obligation needing its own decision, which is correct.

use sha2::{Digest, Sha256};

use crate::model::Condition;

/// Field separator for hash inputs: ASCII Unit Separator, the same byte
/// [`crate::inventory`] uses. Registry ids and dimension values are printable ASCII,
/// so no input can contain it and the concatenation stays injective — `("ab", "c")`
/// can never hash as `("a", "bc")`.
const HASH_SEPARATOR: char = '\u{1f}';

/// The first 8 lowercase-hex chars of SHA-256 over `parts`, joined by
/// [`HASH_SEPARATOR`].
///
/// Deliberately self-contained rather than reaching for `inventory::hash8` or
/// `clause::hash8`: those are private and differently shaped (they hash a schema
/// pointer and a prose excerpt respectively), and coupling to either would tie an
/// obligation's identity to an unrelated record's field set. Only the *truncation
/// convention* is shared, so ids read alike across the registry.
fn hash8(parts: &[&str]) -> String {
    let mut hasher = Sha256::new();
    for (idx, part) in parts.iter().enumerate() {
        if idx > 0 {
            hasher.update([HASH_SEPARATOR as u8]);
        }
        hasher.update(part.as_bytes());
    }
    let digest = hasher.finalize();
    let mut hex = String::with_capacity(8);
    for b in &digest[..4] {
        use std::fmt::Write as _;
        let _ = write!(hex, "{b:02x}");
    }
    hex
}

/// Canonicalize an environment context for hashing: conditions sorted by dimension
/// (then by their values, so the order is total even for a malformed context naming one
/// dimension twice), and each condition's value SUBSET sorted.
///
/// Both sorts are semantic, not cosmetic tolerance for its own sake: a context is a
/// conjunction of conditions and each condition pins a value *set*, so neither order
/// carries meaning and neither may perturb the id.
fn canonical_context(context: &[Condition]) -> String {
    let mut conditions: Vec<(String, Vec<String>)> = context
        .iter()
        .map(|c| {
            let mut values = c.values.clone();
            values.sort();
            (c.dimension.clone(), values)
        })
        .collect();
    conditions.sort();

    let mut out = String::new();
    for (dimension, values) in conditions {
        out.push_str(&dimension);
        out.push(HASH_SEPARATOR);
        out.push_str(&values.join(&HASH_SEPARATOR.to_string()));
        out.push(HASH_SEPARATOR);
    }
    out
}

/// Canonicalize a scenario-dimension assignment for hashing: pairs sorted by key.
///
/// **The function sorts; the caller never has to.** contracts/obligation.md is explicit
/// that two authors writing the same pair in different key order must produce the same
/// id, because otherwise a disposition would silently detach the first time someone
/// reformatted a file. Accepting any iterable of pairs — an `IndexMap` in declaration
/// order, a `BTreeMap`, a plain `Vec` — means no caller can accidentally opt out of that
/// guarantee by picking an order-preserving container.
fn canonical_assignment<K: AsRef<str>, V: AsRef<str>, I: IntoIterator<Item = (K, V)>>(
    assignment: I,
) -> String {
    let mut pairs: Vec<(String, String)> = assignment
        .into_iter()
        .map(|(k, v)| (k.as_ref().to_string(), v.as_ref().to_string()))
        .collect();
    pairs.sort();

    let mut out = String::new();
    for (key, value) in pairs {
        out.push_str(&key);
        out.push(HASH_SEPARATOR);
        out.push_str(&value);
        out.push(HASH_SEPARATOR);
    }
    out
}

/// The `obl-bhv-<hash8>` id for a behavior paired with a context its own applicability
/// requires (contracts/obligation.md, data-model.md §4).
///
/// Hashed over `behavior ‖ canonical(context)`. Reordering the conditions, or the values
/// within a condition, does not change the id; naming a different behavior or pinning a
/// different value set does.
pub fn behavior_obligation_id(behavior: &str, context: &[Condition]) -> String {
    format!(
        "obl-bhv-{}",
        hash8(&[behavior, &canonical_context(context)])
    )
}

/// The `obl-cmb-<hash8>` id for a valid scenario-dimension combination — a pair, or a
/// selected high-risk triple — under one operation (contracts/obligation.md,
/// data-model.md §4).
///
/// Hashed over `operation ‖ canonical(sorted assignment)`. The assignment keys are
/// sorted **inside** this function, so authoring order can never fork an id.
pub fn combination_obligation_id<K, V, I>(operation: &str, assignment: I) -> String
where
    K: AsRef<str>,
    V: AsRef<str>,
    I: IntoIterator<Item = (K, V)>,
{
    format!(
        "obl-cmb-{}",
        hash8(&[operation, &canonical_assignment(assignment)])
    )
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use indexmap::IndexMap;

    use super::*;

    fn pairs(entries: &[(&str, &str)]) -> Vec<(String, String)> {
        entries
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    fn condition(dimension: &str, values: &[&str]) -> Condition {
        Condition {
            dimension: dimension.to_string(),
            values: values.iter().map(|v| v.to_string()).collect(),
        }
    }

    /// T013: the id must not depend on the order the author happened to write the pairs
    /// in. Built from an `IndexMap` — which PRESERVES insertion order, so a function
    /// that failed to sort internally would visibly fork here — rather than from a
    /// `BTreeMap`, which would sort on the caller's behalf and make the test vacuous.
    #[test]
    fn assignment_key_order_does_not_change_the_combination_id() {
        let mut forward: IndexMap<String, String> = IndexMap::new();
        forward.insert("sdim-config-source".to_string(), "compose".to_string());
        forward.insert("sdim-features".to_string(), "lockfile".to_string());
        forward.insert("sdim-layering".to_string(), "single".to_string());

        let mut reverse: IndexMap<String, String> = IndexMap::new();
        reverse.insert("sdim-layering".to_string(), "single".to_string());
        reverse.insert("sdim-features".to_string(), "lockfile".to_string());
        reverse.insert("sdim-config-source".to_string(), "compose".to_string());

        assert_ne!(
            forward.keys().collect::<Vec<_>>(),
            reverse.keys().collect::<Vec<_>>(),
            "the fixture must actually differ in iteration order, or the test proves nothing"
        );
        assert_eq!(
            combination_obligation_id("up", &forward),
            combination_obligation_id("up", &reverse),
            "assignment key order must not change obl-cmb identity (contracts/obligation.md)"
        );

        // The same claim through two other container shapes, so the guarantee is a
        // property of the function and not of one caller's choice of map.
        let vector = pairs(&[
            ("sdim-layering", "single"),
            ("sdim-config-source", "compose"),
            ("sdim-features", "lockfile"),
        ]);
        let tree: BTreeMap<String, String> = vector.iter().cloned().collect();
        assert_eq!(
            combination_obligation_id("up", &forward),
            combination_obligation_id("up", vector.clone()),
            "a Vec in a third order must agree with the IndexMap"
        );
        assert_eq!(
            combination_obligation_id("up", &forward),
            combination_obligation_id("up", &tree),
            "a BTreeMap must agree with the IndexMap"
        );
    }

    #[test]
    fn combination_id_tracks_operation_and_assignment_substance() {
        let base = pairs(&[
            ("sdim-config-source", "compose"),
            ("sdim-features", "lockfile"),
        ]);
        let id = combination_obligation_id("up", base.clone());
        assert!(id.starts_with("obl-cmb-"), "id must be prefixed: {id}");
        assert_eq!(id.len(), "obl-cmb-".len() + 8, "hash8 is 8 hex chars: {id}");

        assert_ne!(
            id,
            combination_obligation_id("build", base.clone()),
            "the same pair under a different operation is a different obligation"
        );
        assert_ne!(
            id,
            combination_obligation_id(
                "up",
                pairs(&[
                    ("sdim-config-source", "image"),
                    ("sdim-features", "lockfile"),
                ]),
            ),
            "changing a value is a different obligation"
        );
        assert_ne!(
            id,
            combination_obligation_id(
                "up",
                pairs(&[
                    ("sdim-config-source", "compose"),
                    ("sdim-features", "lockfile"),
                    ("sdim-container-state", "running"),
                ]),
            ),
            "a triple is not the pair it contains"
        );
        assert_eq!(
            id,
            combination_obligation_id("up", base),
            "identical inputs are identical ids"
        );
    }

    /// The separator must keep the concatenation injective: two different splits of the
    /// same characters must not collide.
    #[test]
    fn combination_id_separator_prevents_field_smearing() {
        assert_ne!(
            combination_obligation_id("up", pairs(&[("ab", "c")])),
            combination_obligation_id("up", pairs(&[("a", "bc")])),
            "a key/value split must be visible to the hash"
        );
        assert_ne!(
            combination_obligation_id("up", pairs(&[("a", "b"), ("c", "d")])),
            combination_obligation_id("up", pairs(&[("a", "bcd")])),
            "a pair boundary must be visible to the hash"
        );
    }

    #[test]
    fn behavior_id_is_stable_under_cosmetic_context_reordering() {
        let forward = vec![
            condition("dim-runtime", &["docker", "podman"]),
            condition("dim-os", &["linux"]),
        ];
        // The same context: conditions swapped, and one condition's value SUBSET
        // written in the other order.
        let reordered = vec![
            condition("dim-os", &["linux"]),
            condition("dim-runtime", &["podman", "docker"]),
        ];
        assert_eq!(
            behavior_obligation_id("bhv-x", &forward),
            behavior_obligation_id("bhv-x", &reordered),
            "condition order and value order are both insignificant"
        );
    }

    #[test]
    fn behavior_id_tracks_behavior_and_context_substance() {
        let context = vec![condition("dim-runtime", &["docker"])];
        let id = behavior_obligation_id("bhv-x", &context);
        assert!(id.starts_with("obl-bhv-"), "id must be prefixed: {id}");
        assert_eq!(id.len(), "obl-bhv-".len() + 8, "hash8 is 8 hex chars: {id}");

        assert_ne!(
            id,
            behavior_obligation_id("bhv-y", &context),
            "a different behavior is a different obligation"
        );
        assert_ne!(
            id,
            behavior_obligation_id("bhv-x", &[condition("dim-runtime", &["podman"])]),
            "a different pinned value is a different obligation"
        );
        assert_ne!(
            id,
            behavior_obligation_id("bhv-x", &[condition("dim-runtime", &["docker", "podman"])]),
            "widening the value subset is a different obligation"
        );
        assert_ne!(
            id,
            behavior_obligation_id("bhv-x", &[]),
            "an empty context (applies everywhere) is a different obligation"
        );
    }

    /// The two kinds share a truncation convention but MUST NOT share an id space: a
    /// behavior obligation and a combination obligation are different decisions.
    #[test]
    fn the_two_kinds_are_distinguishable_by_prefix() {
        let bhv = behavior_obligation_id("x", &[]);
        let cmb = combination_obligation_id("x", pairs(&[]));
        assert!(bhv.starts_with("obl-bhv-"));
        assert!(cmb.starts_with("obl-cmb-"));
        assert_ne!(
            bhv, cmb,
            "the two kinds must not collide on identical input"
        );
    }
}
