//! Exploratory parity discovery — the **hermetic** half
//! (025-exploratory-parity-discovery, plan.md § Project Structure).
//!
//! Everything in this module is pure data-to-data logic: it never invokes the pinned
//! oracle, never talks to Docker, and never touches the network. That is not a
//! convention to be careful about — it is what makes the hermeticity claim of FR-055
//! cheap to hold, because the generator, the shrinker, the signature, and the queue are
//! unit-testable in the fast lane with no external dependency at all. The live half
//! (campaign driver, differential comparison, minimization predicate, candidate
//! assembly, corpus fetch, pipeline proof) lives in `parity_harness::discovery`.
//!
//! ## The two data roots
//!
//! | Root | Ownership | Reachable from `certify`? |
//! |---|---|---|
//! | `conformance/discovery/` | machine-produced + hand-triaged | **No** — a sibling of `registry/`; no loader path reaches it |
//! | `conformance/registry/metamorphic.json` | hand-authored | Yes — it is an assertion the project makes |
//!
//! The separation is structural, not conventional (research D6): [`crate::load`]
//! enumerates *named* subdirectories under `conformance/registry/` and has no wildcard
//! walk at the registry root, so a sibling of `registry/` has no code path that could
//! reach it. Exactly one reference crosses the boundary — `Finding.promotedTo →
//! case-<id>` — and it points **out** of the discovery root into the registry. Nothing
//! in the registry points back, so following references from the registry can never
//! arrive at a finding.
//!
//! ## Violation classes are D-numbered, not V-numbered
//!
//! `discovery check` emits **D1–D5** over the discovery data root; `validate` emits
//! **V1–V32** over the registry. They are numbered separately on purpose: folding the
//! D-series into the V-series would imply the registry validator can see the queue,
//! which is precisely what research D6 says it must not (contracts/findings-queue.md).
//!
//! ## Module map
//!
//! | Module | Owns |
//! |---|---|
//! | [`rng`] | the in-repo deterministic PRNG (research D2) |
//! | [`grammar`] | the constraint inventory indexed as a generation grammar (research D1) |
//! | [`generate`] | constrained candidate generation |
//! | [`mutate`] | the eleven-category mutation operator catalogue |
//! | [`shrink`] | structural reduction; the reproduction predicate is a *parameter* (research D5) |
//! | [`signature`] | the normalized signature + value-shape class (research D3) |
//! | [`queue`] | the findings-queue model, strict loader, atomic writer, and D1–D5 |
//! | [`metamorphic`] | `mrl-` relation model + V31/V32 |
//! | [`corpus`] | the corpus manifest model + immutable-reference validation |
//! | [`report`] | byte-stable campaign + queue reports |

/// The first 8 lowercase-hex chars of SHA-256 over `parts`, **length-prefixed**.
///
/// The single hashing primitive for every discovery id (`sig-`, `fnd-`, `wit-`, `cmp-`,
/// `cnd-`, `cor-`), deliberately shared rather than re-derived per module: two truncation
/// conventions that could drift is the same defect class as two normalization paths. Only
/// the *truncation convention* is shared with `inventory::hash8` / `clause::hash8` — those
/// hash a schema pointer and a prose excerpt respectively, and coupling to either would
/// tie a discovery id's identity to an unrelated record's field set.
///
/// ## Why length-prefixing rather than a separator byte
///
/// The concatenation must be **injective** — `("ab", "c")` must never hash as
/// `("a", "bc")` — or two structurally distinct signatures could share an id, and two
/// distinct defects would silently merge into one finding.
///
/// The other `hash8`s in this crate get injectivity from a `\u{1f}` separator plus the
/// argument that registry ids and dimension values are printable ASCII, so no input can
/// contain the separator. **That argument does not hold here.** A signature's `path` comes
/// verbatim from the diff and is ultimately built from user-controlled configuration keys,
/// so a generated candidate could contain any byte at all — including the separator, at
/// which point a hostile-or-merely-unlucky key would collapse two signatures into one.
/// Prefixing each part with its byte length makes the encoding injective unconditionally,
/// with no assumption about the input alphabet.
pub fn hash8(parts: &[&str]) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    for part in parts {
        hasher.update((part.len() as u64).to_le_bytes());
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

#[cfg(test)]
mod tests {
    use super::hash8;

    #[test]
    fn hashing_is_injective_regardless_of_the_input_alphabet() {
        // The boundary case a separator byte would get wrong.
        assert_ne!(hash8(&["ab", "c"]), hash8(&["a", "bc"]));
        assert_ne!(hash8(&["a", ""]), hash8(&["", "a"]));
        assert_ne!(hash8(&["a"]), hash8(&["a", ""]));

        // And the case the separator argument cannot cover: a part that CONTAINS the
        // byte the other hash8s separate on. A signature's `path` is built from
        // user-controlled configuration keys, so this is reachable input, not a
        // thought experiment.
        assert_ne!(hash8(&["a\u{1f}b"]), hash8(&["a", "b"]));
        assert_ne!(hash8(&["a\u{1f}b", "c"]), hash8(&["a", "b\u{1f}c"]));
    }

    #[test]
    fn the_digest_is_eight_lowercase_hex_chars() {
        let h = hash8(&["chan-stdout", "configuration.remoteUser"]);
        assert_eq!(h.len(), 8);
        assert!(
            h.chars()
                .all(|c| c.is_ascii_hexdigit() && !c.is_uppercase())
        );
        assert_eq!(h, hash8(&["chan-stdout", "configuration.remoteUser"]));
    }
}

pub mod corpus;
pub mod generate;
pub mod grammar;
pub mod metamorphic;
pub mod mutate;
pub mod queue;
pub mod report;
pub mod rng;
pub mod shrink;
pub mod signature;
