//! The generation grammar: the committed constraint inventory, indexed
//! (025-exploratory-parity-discovery, research D1, T012/T013).
//!
//! Generation draws its grammar from `conformance/inventory/constraints.json` — the
//! machine-owned, fingerprint-verified extraction of the vendored pinned schemas — and
//! **not** from re-parsing `conformance/schemas/<pin>/*.json`. Three properties come
//! free from that choice:
//!
//! 1. **The grammar pin is already a recorded revision.** `constraints.json` carries a
//!    `revision` field that V14 validates against the registry schema pin, so FR-002's
//!    `grammarVersion` element is already there and already guarded.
//! 2. **A schema pin bump automatically surfaces as a generation-input change.**
//!    Re-vendoring regenerates the inventory, `inventory diff` enumerates the delta, and
//!    every finding bound to the old revision is correctly invalidated with no separate
//!    bookkeeping.
//! 3. **No second extraction path.** FR-015 forbids a second normalization definition;
//!    the same argument applies one level up to schema interpretation. Two views of the
//!    pinned schema surface that could disagree is the identical defect class.
//!
//! Hand-authoring a grammar was rejected outright: it would generate the shapes its
//! author thought of, which is exactly what the curated fixtures already do and exactly
//! the maintainer imagination this feature exists to escape.
//!
//! ## Annotations are excluded
//!
//! Of the inventory's units, the `annotation` kind (`description`, `title`, …) carries
//! no generative content — it constrains nothing and can be neither satisfied nor
//! violated. [`Grammar::load`] drops those and keeps the rest; the counts the unit tests
//! pin are exactly the non-annotation kinds.

use std::collections::BTreeMap;
use std::path::Path;

use crate::load::{LoadError, load_inventory};
use crate::model::{ConstraintKind, ConstraintUnit};

/// The generation grammar: the non-annotation constraint units of one pinned inventory,
/// indexed for the two lookups generation and mutation actually perform — "what
/// constrains this schema pointer?" and "give me every unit of this kind".
///
/// Every index holds *positions* into [`units`](Grammar::units) rather than clones, so
/// the same unit reached through either index is the same unit, and the memory cost of
/// indexing is a `usize` per entry rather than a second copy of the inventory.
#[derive(Debug, Clone)]
pub struct Grammar {
    /// The inventory revision this grammar was built from (`rev-schema-<pin>`), recorded
    /// verbatim as a campaign's `grammarVersion` (data-model.md § 4).
    revision: String,
    /// The retained (non-annotation) units, in committed inventory order.
    units: Vec<ConstraintUnit>,
    /// Schema pointer → the positions of every unit that constrains it, in inventory
    /// order. `BTreeMap` because the pointer set is enumerated in reports and a
    /// declaration-order-free container would make that output unstable.
    by_pointer: BTreeMap<String, Vec<usize>>,
    /// Constraint kind → the positions of every unit of that kind, in inventory order.
    by_kind: BTreeMap<&'static str, Vec<usize>>,
}

/// The stable wire spelling of a constraint kind — the same kebab-case spelling the
/// committed inventory uses, so a per-kind count in a test reads as the file does.
///
/// Deliberately a free function rather than an inherent `ConstraintKind::as_str`: the
/// spelling is serde's (`rename_all = "kebab-case"`), and duplicating it as an inherent
/// method on the shared model would create a second definition that could drift from the
/// one the file actually round-trips through.
pub fn kind_name(kind: ConstraintKind) -> &'static str {
    match kind {
        ConstraintKind::PropertyExistence => "property-existence",
        ConstraintKind::Required => "required",
        ConstraintKind::Type => "type",
        ConstraintKind::Enum => "enum",
        ConstraintKind::Const => "const",
        ConstraintKind::Default => "default",
        ConstraintKind::UnionAlternative => "union-alternative",
        ConstraintKind::AllOf => "all-of",
        ConstraintKind::Conditional => "conditional",
        ConstraintKind::AdditionalProperties => "additional-properties",
        ConstraintKind::ArrayShape => "array-shape",
        ConstraintKind::ValueShape => "value-shape",
        ConstraintKind::Reference => "reference",
        ConstraintKind::Annotation => "annotation",
        ConstraintKind::UnmodeledKeyword => "unmodeled-keyword",
    }
}

impl Grammar {
    /// Load the grammar from a committed constraint inventory.
    ///
    /// A missing inventory is a hard error rather than an empty grammar: a generator
    /// with no grammar would produce nothing and report "found no differences", which is
    /// the silent-vacuity failure mode this whole feature exists to avoid
    /// (constitution IV).
    pub fn load(inventory_file: &Path) -> Result<Grammar, LoadError> {
        let inventory = load_inventory(inventory_file)?.ok_or_else(|| LoadError::Root {
            path: inventory_file.to_path_buf(),
            cause: "constraint inventory not found — the generation grammar is the \
                    committed inventory (research D1); regenerate it with `inventory generate`"
                .to_string(),
        })?;
        Ok(Grammar::from_units(inventory.revision, inventory.units))
    }

    /// Load the grammar from the workspace's default inventory
    /// (`conformance/inventory/constraints.json`).
    pub fn load_default() -> Result<Grammar, LoadError> {
        Grammar::load(&crate::default_inventory_file())
    }

    /// Build a grammar from an inventory revision and its units, dropping annotations.
    ///
    /// Exposed so a test (or a future re-vendoring workflow) can build a grammar from
    /// units it already holds without a round trip through the filesystem.
    pub fn from_units(revision: String, units: Vec<ConstraintUnit>) -> Grammar {
        let units: Vec<ConstraintUnit> = units
            .into_iter()
            .filter(|u| u.kind != ConstraintKind::Annotation)
            .collect();

        let mut by_pointer: BTreeMap<String, Vec<usize>> = BTreeMap::new();
        let mut by_kind: BTreeMap<&'static str, Vec<usize>> = BTreeMap::new();
        for (position, unit) in units.iter().enumerate() {
            by_pointer
                .entry(unit.pointer.clone())
                .or_default()
                .push(position);
            by_kind
                .entry(kind_name(unit.kind))
                .or_default()
                .push(position);
        }

        Grammar {
            revision,
            units,
            by_pointer,
            by_kind,
        }
    }

    /// The inventory revision (`rev-schema-<pin>`) this grammar was built from — a
    /// campaign's `grammarVersion`.
    pub fn revision(&self) -> &str {
        &self.revision
    }

    /// Every retained (non-annotation) unit, in committed inventory order.
    pub fn units(&self) -> &[ConstraintUnit] {
        &self.units
    }

    /// The total number of retained units.
    pub fn len(&self) -> usize {
        self.units.len()
    }

    /// Whether the grammar is empty. Always `false` for the committed inventory —
    /// [`Grammar::load`] refuses a missing file — but `clippy::len_without_is_empty`
    /// asks for it and a caller checking before drawing is reasonable.
    pub fn is_empty(&self) -> bool {
        self.units.is_empty()
    }

    /// Every unit constraining `pointer` (an RFC 6901 JSON Pointer into the pinned
    /// schema), in inventory order. Empty when the pointer is unconstrained or unknown —
    /// the two are the same fact for a generator, which simply has nothing to draw there.
    pub fn at_pointer(&self, pointer: &str) -> Vec<&ConstraintUnit> {
        self.by_pointer
            .get(pointer)
            .map(|positions| positions.iter().map(|&i| &self.units[i]).collect())
            .unwrap_or_default()
    }

    /// Every unit of `kind` constraining `pointer`, in inventory order.
    ///
    /// The composite lookup, because that is the question generation actually asks: not
    /// "what constrains this pointer" but "what *type* may this pointer hold" or "which
    /// keys here are `required`".
    pub fn at_pointer_of_kind(&self, pointer: &str, kind: ConstraintKind) -> Vec<&ConstraintUnit> {
        self.by_pointer
            .get(pointer)
            .map(|positions| {
                positions
                    .iter()
                    .map(|&i| &self.units[i])
                    .filter(|u| u.kind == kind)
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Every unit of `kind`, in inventory order.
    pub fn of_kind(&self, kind: ConstraintKind) -> Vec<&ConstraintUnit> {
        self.by_kind
            .get(kind_name(kind))
            .map(|positions| positions.iter().map(|&i| &self.units[i]).collect())
            .unwrap_or_default()
    }

    /// Every constrained schema pointer, sorted — the generator's draw domain.
    pub fn pointers(&self) -> Vec<&str> {
        self.by_pointer.keys().map(String::as_str).collect()
    }

    /// Retained unit counts per kind, keyed by the committed wire spelling.
    ///
    /// Sorted (`BTreeMap`) so a report or a failure message renders identically on every
    /// run; a re-vendored inventory therefore shows up as a readable diff rather than a
    /// reshuffle.
    pub fn kind_counts(&self) -> BTreeMap<&'static str, usize> {
        self.by_kind
            .iter()
            .map(|(kind, positions)| (*kind, positions.len()))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn workspace_grammar() -> Grammar {
        Grammar::load_default().expect("the committed constraint inventory must load")
    }

    /// The per-kind unit counts of the inventory at `rev-schema-113500f4`.
    ///
    /// Pinned deliberately (T013): the grammar is a generation *input*, so a re-vendored
    /// inventory that silently changes what can be generated would silently change what
    /// campaigns explore and quietly invalidate every recorded finding. Making the counts
    /// a test means a re-vendoring surfaces here — as a reviewed test change — instead of
    /// as an unexplained shift in campaign output weeks later.
    const EXPECTED_KIND_COUNTS: &[(&str, usize)] = &[
        ("additional-properties", 38),
        ("all-of", 4),
        ("array-shape", 41),
        ("const", 3),
        ("default", 9),
        ("enum", 11),
        ("property-existence", 117),
        ("reference", 12),
        ("required", 20),
        ("type", 187),
        ("union-alternative", 18),
        ("value-shape", 9),
    ];

    #[test]
    fn the_committed_inventory_loads_as_a_grammar() {
        let grammar = workspace_grammar();
        assert_eq!(
            grammar.revision(),
            format!("rev-schema-{}", crate::CURRENT_SCHEMA_PIN),
            "the grammar's revision is a campaign's `grammarVersion` and must equal the pin"
        );
        assert!(
            !grammar.is_empty(),
            "an empty grammar generates nothing and would report `found no differences`"
        );
    }

    #[test]
    fn per_kind_unit_counts_match_the_pinned_inventory() {
        let grammar = workspace_grammar();
        let counts = grammar.kind_counts();
        let expected: BTreeMap<&str, usize> = EXPECTED_KIND_COUNTS.iter().copied().collect();
        assert_eq!(
            counts, expected,
            "the generation grammar changed. If this is a deliberate re-vendoring, run \
             `inventory diff`, review the delta, and update EXPECTED_KIND_COUNTS in the same \
             commit — every finding bound to the old revision is invalidated by the change."
        );
    }

    #[test]
    fn the_generative_kinds_carry_the_documented_totals() {
        // research D1's table, asserted directly: these six lines are the counts the
        // design reasoned from, so they are worth stating in the vocabulary of the
        // decision rather than only as part of the map above.
        let grammar = workspace_grammar();
        assert_eq!(grammar.of_kind(ConstraintKind::Type).len(), 187);
        assert_eq!(
            grammar.of_kind(ConstraintKind::PropertyExistence).len(),
            117
        );
        assert_eq!(grammar.of_kind(ConstraintKind::ArrayShape).len(), 41);
        assert_eq!(grammar.of_kind(ConstraintKind::Required).len(), 20);
        assert_eq!(grammar.of_kind(ConstraintKind::UnionAlternative).len(), 18);
        assert_eq!(
            grammar.of_kind(ConstraintKind::Enum).len()
                + grammar.of_kind(ConstraintKind::Const).len(),
            14,
            "`enum` + `const` are the exact-legal-value kinds and the near-miss set one \
             edit away; research D1 counts them together"
        );
    }

    #[test]
    fn annotations_are_dropped_and_the_rest_is_retained() {
        let grammar = workspace_grammar();
        assert!(
            grammar.of_kind(ConstraintKind::Annotation).is_empty(),
            "annotations constrain nothing and can be neither satisfied nor violated"
        );
        let total: usize = EXPECTED_KIND_COUNTS.iter().map(|(_, n)| n).sum();
        assert_eq!(
            grammar.len(),
            total,
            "the grammar is exactly the non-annotation units"
        );
        assert_eq!(
            grammar.len(),
            469,
            "research D1 sizes the grammar at 469 non-annotation units of 609 total"
        );
    }

    #[test]
    fn lookup_by_pointer_returns_every_kind_at_that_pointer() {
        let grammar = workspace_grammar();
        // A pointer taken from the committed inventory rather than invented, so the
        // assertion is about the real indexing and not about a fixture.
        let pointer =
            "/definitions/dockerfileContainer/oneOf/0/properties/build/allOf/0/properties/context";
        let units = grammar.at_pointer(pointer);
        assert!(
            units.len() >= 2,
            "expected the `property-existence` and `type` units at {pointer}, got {}",
            units.len()
        );
        assert!(units.iter().all(|u| u.pointer == pointer));

        let types = grammar.at_pointer_of_kind(pointer, ConstraintKind::Type);
        assert_eq!(types.len(), 1, "one `type` unit at {pointer}");
        assert_eq!(types[0].substance, serde_json::json!({ "type": "string" }));

        assert!(
            grammar.at_pointer("/definitely/not/a/pointer").is_empty(),
            "an unknown pointer yields nothing to draw, not a panic"
        );
        assert!(
            grammar
                .at_pointer_of_kind(pointer, ConstraintKind::Annotation)
                .is_empty(),
            "annotations were dropped, so no pointer can reach one"
        );
    }

    #[test]
    fn pointers_are_sorted_and_index_positions_agree() {
        let grammar = workspace_grammar();
        let pointers = grammar.pointers();
        let mut sorted = pointers.clone();
        sorted.sort_unstable();
        assert_eq!(
            pointers, sorted,
            "pointer enumeration must be deterministic"
        );

        // Every unit is reachable through BOTH indexes, and the two agree — the
        // property that lets a caller mix the lookups without re-deriving anything.
        for unit in grammar.units() {
            assert!(
                grammar
                    .at_pointer(&unit.pointer)
                    .iter()
                    .any(|u| u.id == unit.id),
                "unit {} missing from the pointer index",
                unit.id
            );
            assert!(
                grammar.of_kind(unit.kind).iter().any(|u| u.id == unit.id),
                "unit {} missing from the kind index",
                unit.id
            );
        }
    }

    #[test]
    fn a_missing_inventory_fails_loudly() {
        let err = Grammar::load(Path::new("/nonexistent/constraints.json"))
            .expect_err("a missing inventory must not yield an empty grammar");
        let msg = err.to_string();
        assert!(
            msg.contains("constraint inventory not found"),
            "the diagnosis must name the cause, got: {msg}"
        );
    }
}
