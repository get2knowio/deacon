//! The disallowed-Features policy gate, shared by `up` and `build`.
//!
//! An operator names Features they refuse to have installed, and this gate
//! refuses the run before deacon touches a registry or a daemon. The list is
//! deacon's own local knob — the comma-separated `DEACON_DISALLOWED_FEATURES`
//! environment variable, plus a (currently empty) compiled-in list — and NOT
//! the reference CLI's remote control manifest, which is a separate open
//! question ([#676]).
//!
//! What IS taken from the reference is how an entry matches a Feature id and
//! which Features the gate can see, because deacon's version of both silently
//! failed open ([#675]):
//!
//! - an entry matches by PREFIX terminated at a Feature-id separator, so
//!   `ghcr.io/devcontainers/features/node` covers `…/node:1` — the form a
//!   configuration almost always names. Exact-string matching made the natural
//!   entry block nothing at all.
//! - the gate sees the Features a run will actually install, which is the
//!   configuration's union with `--additional-features` — moving a Feature to
//!   the command line used to walk straight past it.
//! - `build` consults it too. It used to be reachable only from `up`.
//!
//! [#675]: https://github.com/get2knowio/deacon/issues/675
//! [#676]: https://github.com/get2knowio/deacon/issues/676

use anyhow::Result;
use deacon_core::errors::{ConfigError, DeaconError};
use tracing::debug;

/// Features refused regardless of the environment. Deliberately empty: deacon
/// ships no opinion about which Features are problematic, and #676 is where
/// adopting someone else's opinion is being decided.
const DISALLOWED_FEATURES: &[&str] = &[];

/// Characters that terminate the prefix half of a Feature id.
///
/// Mirrors the reference's `findDisallowedFeatureEntry`
/// (`src/spec-node/disallowedFeatures.ts`): a tag (`:`), a digest (`@`) or a
/// deeper path segment (`/`) continues the same Feature, so a prefix covers
/// them; any other character starts a DIFFERENT Feature and must not match.
/// `example.io/test/node` therefore covers `…/node:1`, `…/node/js` and
/// `…/node@abc`, but not `…/nodej` and not `…/node.js`.
const ID_SEPARATORS: [u8; 3] = *b"/:@";

/// Whether `entry` covers `feature_id` under the reference's prefix rule.
fn entry_covers(entry: &str, feature_id: &str) -> bool {
    // An empty entry is a prefix of everything. The reference can only get one
    // from a malformed manifest; deacon gets one from a stray comma
    // (`DEACON_DISALLOWED_FEATURES=a,,b`) or an empty variable, and blocking
    // every Feature is never what that meant. Callers filter these out, so this
    // guard is belt-and-braces for a direct call.
    if entry.is_empty() {
        return false;
    }
    let Some(rest) = feature_id.strip_prefix(entry) else {
        return false;
    };
    // `strip_prefix` succeeded, so the boundary is a char boundary and the
    // separators are ASCII — a byte look is exact here.
    match rest.as_bytes().first() {
        None => true, // Feature id equal to the entry.
        Some(byte) => ID_SEPARATORS.contains(byte),
    }
}

/// The entries an operator has disallowed, in the order they were written.
fn disallowed_entries() -> Vec<String> {
    let mut entries: Vec<String> = DISALLOWED_FEATURES.iter().map(|e| e.to_string()).collect();
    if let Ok(raw) = std::env::var("DEACON_DISALLOWED_FEATURES") {
        entries.extend(
            raw.split(',')
                .map(str::trim)
                .filter(|entry| !entry.is_empty())
                .map(str::to_string),
        );
    }
    entries
}

/// Refuse the run when any Feature it would install is disallowed.
///
/// `features` is the configuration's `features` object AFTER any
/// `--additional-features` merge, so the gate sees exactly the set that would
/// be installed — which is also why `--ignore-additional-features` correctly
/// takes an overlay back out of scope.
///
/// Callers MUST invoke this before any registry or daemon work and before
/// `initializeCommand`; the reference refuses without touching either.
pub(crate) fn check_for_disallowed_features(features: &serde_json::Value) -> Result<()> {
    let entries = disallowed_entries();
    if entries.is_empty() {
        return Ok(());
    }

    let Some(features_obj) = features.as_object() else {
        return Ok(());
    };

    debug!(
        entries = ?entries,
        count = features_obj.len(),
        "Checking Features against the disallowed list"
    );

    for feature_id in features_obj.keys() {
        if let Some(entry) = entries.iter().find(|entry| entry_covers(entry, feature_id)) {
            return Err(DeaconError::Config(ConfigError::DisallowedFeature {
                feature_id: feature_id.clone(),
                matched: entry.clone(),
            })
            .into());
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// The reference's own vectors, from `src/test/disallowedFeatures.test.ts`
    /// ("matches equal feature id and prefix").
    #[test]
    fn an_entry_covers_a_feature_id_the_way_the_reference_matches_one() {
        let entry = "example.io/test/node";

        assert!(entry_covers(entry, "example.io/test/node"), "equal");
        assert!(entry_covers(entry, "example.io/test/node:1"), "tag");
        assert!(entry_covers(entry, "example.io/test/node/js"), "sub-path");
        assert!(entry_covers(entry, "example.io/test/node@abc"), "digest");

        assert!(!entry_covers(entry, "example.io/test/nodej"), "longer name");
        assert!(!entry_covers(entry, "example.io/test/nod"), "shorter name");
        assert!(
            !entry_covers(entry, "example.io/test/node.js"),
            "'.' is not a separator"
        );
    }

    #[test]
    fn an_empty_entry_covers_nothing() {
        // A stray comma must not disallow the world.
        assert!(!entry_covers("", "ghcr.io/devcontainers/features/node:1"));
    }

    #[test]
    fn a_versioned_feature_is_blocked_by_its_unversioned_entry() {
        // The regression #675 was filed for: this is the entry an operator writes.
        temp_env::with_var(
            "DEACON_DISALLOWED_FEATURES",
            Some("ghcr.io/devcontainers/features/node"),
            || {
                let features = json!({ "ghcr.io/devcontainers/features/node:1": {} });
                let err = check_for_disallowed_features(&features)
                    .expect_err("a versioned id must be covered by its unversioned entry");
                let rendered = err.to_string();
                assert!(
                    rendered.contains("ghcr.io/devcontainers/features/node:1"),
                    "the diagnostic must name the Feature that was blocked: {rendered}"
                );
            },
        );
    }

    #[test]
    fn a_neighbouring_feature_is_not_blocked() {
        temp_env::with_var(
            "DEACON_DISALLOWED_FEATURES",
            Some("ghcr.io/devcontainers/features/node"),
            || {
                let features = json!({ "ghcr.io/devcontainers/features/nodejs:1": {} });
                assert!(check_for_disallowed_features(&features).is_ok());
            },
        );
    }

    #[test]
    fn entries_are_trimmed_and_empty_segments_dropped() {
        temp_env::with_var(
            "DEACON_DISALLOWED_FEATURES",
            Some(" , ghcr.io/x/y , "),
            || {
                assert!(check_for_disallowed_features(&json!({ "ghcr.io/a/b:1": {} })).is_ok());
                assert!(check_for_disallowed_features(&json!({ "ghcr.io/x/y:2": {} })).is_err());
            },
        );
    }

    #[test]
    fn an_unset_variable_blocks_nothing() {
        temp_env::with_var_unset("DEACON_DISALLOWED_FEATURES", || {
            assert!(check_for_disallowed_features(&json!({ "ghcr.io/a/b:1": {} })).is_ok());
        });
    }

    #[test]
    fn a_non_object_features_value_is_not_a_policy_question() {
        temp_env::with_var("DEACON_DISALLOWED_FEATURES", Some("ghcr.io/x/y"), || {
            assert!(check_for_disallowed_features(&json!(null)).is_ok());
        });
    }
}
