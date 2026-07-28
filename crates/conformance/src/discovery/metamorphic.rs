//! Metamorphic relation (`mrl-`) records — `conformance/registry/metamorphic.json`
//! (025-exploratory-parity-discovery, data-model.md § 7,
//! contracts/metamorphic-catalogue.md, US6).
//!
//! Relations live **inside** the registry, unlike findings, because a relation is an
//! *assertion the project makes* — "reordering these keys must not change the result,
//! and here is the clause that says so" — and it references `clu-`/`bhv-` ids only the
//! registry loader can resolve (research D11). A finding, by contrast, is a *candidate*
//! for an assertion: machine-produced, unreviewed, possibly wrong, and structurally
//! unable to reach `certify`.
//!
//! ## The two effects, and why sensitivity is mandatory rather than a bonus
//!
//! | [`RelationEffect`] | Assertion | Catches |
//! |---|---|---|
//! | `invariance` | the transformation MUST NOT change the normalized result | a tool reading meaning that is not there |
//! | `sensitivity` | the transformation MUST change the normalized result | a tool ignoring meaning that *is* there |
//!
//! **A sensitivity relation is the one thing the differential structurally cannot
//! replace.** If deacon and the reference *both* wrongly ignore declaration order, the
//! differential comparison is clean and the defect is invisible to it — both sides agree,
//! and agreeing is exactly what the differential checks. A sensitivity relation asserts
//! the result *must* change, so consistent-wrongness fails it. That is why FR-043 mandates
//! both kinds rather than treating sensitivity as an optional extra.
//!
//! ## The ground requirement (FR-045)
//!
//! Every relation MUST name a [`ground`](MetamorphicRelation::ground) resolving to a
//! normative clause (`clu-`) or a recorded behavior (`bhv-`). Without one, a relation
//! records an author's intuition about what *ought* to be irrelevant — and an ungrounded
//! invariance relation that happens to be wrong does not fail, it **passes**, silently,
//! while asserting something the specification never said. A grounded one can be checked
//! by reading the clause. This mirrors the `ground` that 024 already requires on
//! applicability rules, and gets the same validation treatment
//! ([`crate::validate::check_metamorphic`], **V31**/**V32**).
//!
//! ## Ownership
//!
//! Hand-authored. No generator writes this file. The mandated family list
//! ([`MANDATED_RELATIONS`]) is named here rather than derived, for the same reason
//! `REQUIRED_SCENARIO_DIMENSIONS` is: *removing* a family must be a visible **V32**
//! failure rather than a quietly smaller relation set.

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::load::{LoadError, SchemaError, deserialize_located, read_file};

/// What a relation asserts about the transformation it declares
/// (contracts/metamorphic-catalogue.md, "The two effects").
///
/// A closed enum rather than a bare string so a misspelled effect is refused at **load**
/// time with a located diagnosis, strictly earlier than — and with the same outcome as —
/// the **V31** clause the contract states it under. A record whose effect nobody
/// recognises must never reach evaluation, where "unknown" would have to be resolved into
/// one of the two answers by a default.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RelationEffect {
    /// The transformation MUST NOT change the normalized result.
    Invariance,
    /// The transformation MUST change the normalized result.
    Sensitivity,
}

impl RelationEffect {
    /// The stable wire spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            RelationEffect::Invariance => "invariance",
            RelationEffect::Sensitivity => "sensitivity",
        }
    }
}

/// One metamorphic relation (data-model.md § 7).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MetamorphicRelation {
    /// `mrl-<slug>`; unique across **all** registry id namespaces (V2).
    pub id: String,
    /// What is applied to the input, in one reviewable sentence. Unique across the
    /// catalogue (**V31**): two records claiming the same transformation would be one
    /// relation asserted twice, and a failure could not be attributed to either.
    pub transformation: String,
    /// Which of the two things the relation asserts.
    pub effect: RelationEffect,
    /// `clu-<…>` or `bhv-<…>`; REQUIRED and MUST resolve (**V31**, FR-045).
    pub ground: String,
    /// The observable channels the relation asserts over. Non-empty, each a declared
    /// `chan-` (**V31**): a relation asserting over no channel observes nothing and can
    /// never fail.
    pub channels: Vec<String>,
    /// Why the ground supports the assertion — the argument a reviewer judges. Required
    /// and non-filler (**V31**).
    pub rationale: String,
}

/// The `metamorphic.json` envelope — a `records` collection, matching every other
/// hand-authored registry file.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MetamorphicFile {
    /// Schema version of this file.
    #[serde(default)]
    pub schema_version: u32,
    /// The relation records, in declaration order.
    #[serde(default)]
    pub records: Vec<MetamorphicRelation>,
}

/// The relation families FR-044 mandates (contracts/metamorphic-catalogue.md, "Mandated
/// families"). A family with no record is **V32**.
///
/// Named rather than derived, so *removing* a family is a visible failure instead of a
/// quietly smaller relation set — the same reasoning as
/// [`crate::validate::REQUIRED_SCENARIO_DIMENSIONS`].
pub const MANDATED_RELATIONS: &[&str] = &[
    "mrl-formatting-invariance",
    "mrl-comment-invariance",
    "mrl-key-order-invariance",
    "mrl-path-relocation",
    "mrl-lifecycle-equivalence",
    "mrl-extends-flattening",
    "mrl-declaration-order-sensitivity",
];

/// Load `metamorphic.json` from `path`.
///
/// A missing file yields an empty list (a fixture registry that ships none is not an
/// error; **V32** is what refuses an incomplete set for a registry that has opted in). A
/// present-but-malformed file is a located [`LoadError::Schema`] — never silently empty
/// (constitution IV).
pub fn load_metamorphic(path: &Path) -> Result<Vec<MetamorphicRelation>, LoadError> {
    if !path.exists() {
        return Ok(Vec::new());
    }
    let raw = read_file(path).map_err(|e| LoadError::Schema(vec![e]))?;
    let file: MetamorphicFile =
        deserialize_located(path, &raw).map_err(|e| LoadError::Schema(vec![e]))?;
    Ok(file.records)
}

/// Collect duplicate-`id` relations as located schema errors, so two records can never
/// silently claim the same identity.
pub(crate) fn duplicate_id_errors(
    path: &Path,
    records: &[MetamorphicRelation],
) -> Vec<SchemaError> {
    let mut seen: std::collections::BTreeSet<&str> = std::collections::BTreeSet::new();
    let mut out = Vec::new();
    for record in records {
        if !seen.insert(record.id.as_str()) {
            out.push(SchemaError {
                file: path.to_path_buf(),
                location: Some(record.id.clone()),
                message: format!(
                    "duplicate metamorphic relation id `{}` — every lookup takes the first \
                     match, so a duplicate would evaluate one record and silently ignore the \
                     other",
                    record.id
                ),
            });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const GOOD: &str = r##"{
      "schemaVersion": 1,
      "records": [
        {
          "id": "mrl-key-order-invariance",
          "transformation": "permute the key order within an unordered JSON object",
          "effect": "invariance",
          "ground": "clu-a1b2c3d4",
          "channels": ["chan-structured-output"],
          "rationale": "Object member order carries no meaning in JSON, and the configuration schema declares no ordered object. A result that changes under key permutation is reading order it must not read."
        }
      ]
    }"##;

    #[test]
    fn the_contract_example_record_loads() {
        // contracts/metamorphic-catalogue.md § Record schema, verbatim.
        let file: MetamorphicFile = serde_json::from_str(GOOD).expect("the contract example loads");
        let record = &file.records[0];
        assert_eq!(record.id, "mrl-key-order-invariance");
        assert_eq!(record.effect, RelationEffect::Invariance);
        assert_eq!(record.ground, "clu-a1b2c3d4");
        assert_eq!(record.channels, vec!["chan-structured-output".to_string()]);
        assert!(!record.rationale.is_empty());
    }

    #[test]
    fn an_unknown_effect_is_refused_at_load_rather_than_defaulted() {
        // The contract lists "unknown effect" under V31; the closed enum refuses it
        // strictly earlier, which is the same outcome reached sooner. What must never
        // happen is a record whose effect is unrecognised reaching evaluation, where
        // "unknown" would have to be resolved into one of the two answers by a default.
        let raw = GOOD.replace("\"invariance\"", "\"invariant-ish\"");
        let err = serde_json::from_str::<MetamorphicFile>(&raw)
            .expect_err("an unknown effect must not load");
        assert!(
            err.to_string().contains("invariant-ish"),
            "the diagnosis must name the offending spelling, got: {err}"
        );
    }

    #[test]
    fn unknown_fields_are_rejected() {
        let raw = GOOD.replace("\"ground\":", "\"grounds\":");
        let err = serde_json::from_str::<MetamorphicFile>(&raw)
            .expect_err("strict JSON must reject unknown fields");
        assert!(err.to_string().contains("grounds"), "got: {err}");
    }

    #[test]
    fn a_record_missing_its_ground_does_not_load() {
        // FR-045's floor, enforced at the shape that reads the file: `ground` is not
        // `Option`, so a record without one is unrepresentable rather than merely invalid.
        let raw = GOOD.replace(r#""ground": "clu-a1b2c3d4","#, "");
        let err = serde_json::from_str::<MetamorphicFile>(&raw)
            .expect_err("a groundless relation must not load");
        assert!(err.to_string().contains("ground"), "got: {err}");
    }

    #[test]
    fn both_effects_round_trip_through_their_wire_spellings() {
        for effect in [RelationEffect::Invariance, RelationEffect::Sensitivity] {
            let raw = serde_json::to_string(&effect).expect("serializes");
            assert_eq!(raw, format!("\"{}\"", effect.as_str()));
            let back: RelationEffect = serde_json::from_str(&raw).expect("round-trips");
            assert_eq!(back, effect);
        }
    }

    #[test]
    fn the_mandated_family_list_is_the_contract_table() {
        // contracts/metamorphic-catalogue.md § Mandated families (FR-044): seven rows,
        // exactly one of them a sensitivity relation.
        assert_eq!(MANDATED_RELATIONS.len(), 7);
        for id in MANDATED_RELATIONS {
            assert!(id.starts_with("mrl-"), "{id} must be an `mrl-` id");
        }
        assert!(MANDATED_RELATIONS.contains(&"mrl-declaration-order-sensitivity"));
        let mut sorted = MANDATED_RELATIONS.to_vec();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted.len(), MANDATED_RELATIONS.len(), "no duplicates");
    }

    #[test]
    fn missing_file_loads_empty_and_malformed_file_fails_loud() {
        let dir = tempfile::tempdir().expect("tempdir");
        assert!(
            load_metamorphic(&dir.path().join("metamorphic.json"))
                .expect("absent file is not an error")
                .is_empty()
        );
        let bad = dir.path().join("bad.json");
        std::fs::write(&bad, "{ not json").expect("write fixture");
        assert!(
            load_metamorphic(&bad).is_err(),
            "a malformed metamorphic.json must fail loud, never default to empty"
        );
        let good = dir.path().join("metamorphic.json");
        std::fs::write(&good, GOOD).expect("write fixture");
        assert_eq!(load_metamorphic(&good).expect("loads").len(), 1);
    }

    #[test]
    fn duplicate_ids_are_reported() {
        let file: MetamorphicFile = serde_json::from_str(GOOD).expect("loads");
        let mut records = file.records.clone();
        records.push(file.records[0].clone());
        let errors = duplicate_id_errors(Path::new("metamorphic.json"), &records);
        assert_eq!(errors.len(), 1);
        assert!(errors[0].message.contains("mrl-key-order-invariance"));
        assert!(duplicate_id_errors(Path::new("metamorphic.json"), &file.records).is_empty());
    }
}
