//! Deterministic RFC-2119 strength detection (021-normative-clause-inventory,
//! research Decision 4).
//!
//! [`detect_strength`] maps the presence of an UPPERCASE RFC-2119 keyword to a
//! [`Strength`], returning `None` when no keyword is present (the honest "ambiguous"
//! outcome a human must resolve). [`has_family`] answers the per-label V15 cross-check:
//! a clause the author labeled `must`/`should`/`may` MUST carry the corresponding keyword
//! family in its excerpt, and a `descriptive` clause MUST NOT hide a mandatory keyword.
//!
//! Keywords are matched only in their UPPERCASE RFC-2119 form (maximal runs of ASCII
//! uppercase letters), so lowercase hedge words ("should generally", "may want to") never
//! auto-promote a clause to a strict requirement — they surface as ambiguous (`None`).

use crate::model::Strength;

/// The three keyword families detectable from a single RFC-2119 keyword.
/// `algorithm` / `io-contract` / `descriptive` are authored labels, not keyword-derived.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Family {
    Must,
    Should,
    May,
}

/// Whether a maximal uppercase run is an RFC-2119 keyword of a given family.
fn family_of_keyword(word: &str) -> Option<Family> {
    match word {
        "MUST" | "REQUIRED" | "SHALL" => Some(Family::Must),
        "SHOULD" | "RECOMMENDED" => Some(Family::Should),
        "MAY" | "OPTIONAL" => Some(Family::May),
        _ => None,
    }
}

/// Every RFC-2119 keyword family present in `excerpt` (matched only in UPPERCASE form).
fn families_present(excerpt: &str) -> (bool, bool, bool) {
    let (mut must, mut should, mut may) = (false, false, false);
    for word in uppercase_runs(excerpt) {
        match family_of_keyword(word) {
            Some(Family::Must) => must = true,
            Some(Family::Should) => should = true,
            Some(Family::May) => may = true,
            None => {}
        }
    }
    (must, should, may)
}

/// Detect the primary normative strength of an excerpt from its RFC-2119 keywords,
/// with priority `must` > `should` > `may`. Returns `None` when no keyword is present
/// (ambiguous — a human decision, never auto-promoted; research Decision 5).
pub fn detect_strength(excerpt: &str) -> Option<Strength> {
    let (must, should, may) = families_present(excerpt);
    if must {
        Some(Strength::Must)
    } else if should {
        Some(Strength::Should)
    } else if may {
        Some(Strength::May)
    } else {
        None
    }
}

/// Whether `excerpt` carries the RFC-2119 keyword family corresponding to `strength`
/// (the per-label V15 cross-check). Returns `true` for the non-keyword strengths
/// (`algorithm`, `io-contract`, `descriptive`) — those are authored labels this function
/// does not police here; `descriptive` is policed separately by [`hides_mandatory_keyword`].
pub fn has_family(excerpt: &str, strength: Strength) -> bool {
    let (must, should, may) = families_present(excerpt);
    match strength {
        Strength::Must => must,
        Strength::Should => should,
        Strength::May => may,
        Strength::Algorithm | Strength::IoContract | Strength::Descriptive => true,
    }
}

/// Whether `excerpt` contains an unqualified mandatory (MUST-family) RFC-2119 keyword —
/// used to reject a `descriptive` label that hides a requirement (FR-005, V15).
pub fn hides_mandatory_keyword(excerpt: &str) -> bool {
    families_present(excerpt).0
}

/// Iterate maximal runs of ASCII uppercase letters in `text` (length ≥ 2), the candidate
/// RFC-2119 keyword tokens. A run must be delimited by non-uppercase-letter boundaries so
/// `MUST` inside `MUSTARDY` (were it uppercase) is one run and never a keyword match.
fn uppercase_runs(text: &str) -> impl Iterator<Item = &str> {
    text.split(|c: char| !c.is_ascii_uppercase())
        .filter(|w| w.len() >= 2)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_must_family() {
        assert_eq!(detect_strength("The tool MUST do X."), Some(Strength::Must));
        assert_eq!(
            detect_strength("The tool MUST NOT do X."),
            Some(Strength::Must)
        );
        assert_eq!(detect_strength("This is REQUIRED."), Some(Strength::Must));
        assert_eq!(
            detect_strength("The tool SHALL do X."),
            Some(Strength::Must)
        );
    }

    #[test]
    fn detects_should_and_may_families() {
        assert_eq!(
            detect_strength("The tool SHOULD do X."),
            Some(Strength::Should)
        );
        assert_eq!(
            detect_strength("This is RECOMMENDED."),
            Some(Strength::Should)
        );
        assert_eq!(detect_strength("The tool MAY do X."), Some(Strength::May));
        assert_eq!(detect_strength("This is OPTIONAL."), Some(Strength::May));
    }

    #[test]
    fn priority_is_must_then_should_then_may() {
        assert_eq!(
            detect_strength("The tool MUST do X but MAY do Y."),
            Some(Strength::Must)
        );
        assert_eq!(
            detect_strength("The tool SHOULD do X but MAY do Y."),
            Some(Strength::Should)
        );
    }

    #[test]
    fn lowercase_hedges_are_ambiguous() {
        assert_eq!(detect_strength("The tool should generally do X."), None);
        assert_eq!(detect_strength("The tool may want to do X."), None);
        assert_eq!(detect_strength("The tool is expected to do X."), None);
        assert_eq!(detect_strength("Purely descriptive prose."), None);
    }

    #[test]
    fn has_family_cross_check() {
        assert!(has_family("The tool MUST do X.", Strength::Must));
        assert!(!has_family("The tool SHOULD do X.", Strength::Must));
        assert!(has_family("The tool SHOULD do X.", Strength::Should));
        assert!(has_family("The tool MAY do X.", Strength::May));
        // Non-keyword strengths are not policed by has_family.
        assert!(has_family(
            "Compute the id as follows.",
            Strength::Algorithm
        ));
    }

    #[test]
    fn descriptive_hiding_a_mandatory_keyword_is_detectable() {
        assert!(hides_mandatory_keyword("The tool MUST do X."));
        assert!(!hides_mandatory_keyword("The tool is described here."));
        assert!(!hides_mandatory_keyword("The tool may do X."));
    }
}
