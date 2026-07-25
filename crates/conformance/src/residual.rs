//! Residual records — `conformance/registry/residuals.json` (data-model.md §3).
//!
//! A residual is a baseline unit that **cannot yet be expressed as data**. It is
//! *representation debt*, not a coverage gap: the behavior is still covered, by the
//! carrier program that has not been retired. Residuals therefore **never block
//! certification** (FR-054) — unlike `gaps.json`, which continues to block. What a
//! residual *does* block is deleting its carrier (FR-013): a program is deletable only
//! when every unit it carries has migrated.
//!
//! **Ownership**: hand-authored. Generation never writes this file.
//!
//! Two fields are load-bearing and are therefore enforced at *deserialize* time rather
//! than left to a later validation pass (FR-013, FR-055):
//!
//! - `missingCapability` — a specific named capability. An empty string is rejected
//!   here; a merely *vague* one ("not supported yet") is **V23** at validation.
//! - `followUp` — a tracked issue reference. Without it a residual is an open-ended
//!   excuse rather than queued work.
//!
//! `blockedCarrier` is optional **only** for `external-corpus-entry` residuals — the 33
//! pinned manifest entries block no program because no program runs them (research
//! D8). Absent-and-required is **V23**, checked against the baseline category in
//! `validate.rs` (User Story 2, T032), because that judgement needs the baseline.
//!
//! ## Queued versus permanent (024, P1)
//!
//! Not every residual is *debt*. Some units can never be expressed as data — because a
//! principle forbids it (feature-authoring subcommands are out of scope, Constitution II),
//! or because the claim is not a conformance claim at all (an intra-deacon consistency
//! check has no reference and no spec expectation, so the three-axis model has nothing to
//! record). Those units carry a `followUp` that promises work which cannot happen.
//!
//! [`ResidualDisposition`] separates the two, and the split is what makes the queue
//! readable: a queue that asymptotes at a nonzero floor forever carries no signal, whereas
//! `residualQueue` reaching zero is a claim someone can act on. `Permanent` therefore
//! **requires** an `outOfScopeRationale` and **forbids** a `followUp`; `Queued` requires a
//! `followUp` and forbids a rationale. Both are enforced at deserialize time via
//! [`RawResidualRecord`] — a permanent residual with a tracked follow-up is a
//! contradiction, not a nuance.

use std::path::Path;

use serde::{Deserialize, Deserializer, Serialize};

use crate::load::{LoadError, SchemaError, deserialize_located, read_file};

/// Whether a residual is migratable work or a permanent, principled exclusion (024 P1).
///
/// Defaults to [`Queued`](ResidualDisposition::Queued) so an existing record without the
/// field keeps its current meaning: representation debt awaiting a capability.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ResidualDisposition {
    /// Migratable once a named capability exists. Requires `followUp`.
    #[default]
    Queued,
    /// Never migratable — a principle or a category mismatch forbids it, not a missing
    /// feature. Requires `outOfScopeRationale`; a `followUp` is a contradiction.
    Permanent,
}

impl ResidualDisposition {
    /// Whether this disposition is [`Permanent`](ResidualDisposition::Permanent).
    pub fn is_permanent(self) -> bool {
        matches!(self, ResidualDisposition::Permanent)
    }
}

/// One residual record (data-model.md §3).
///
/// Deserialized via [`RawResidualRecord`] so the disposition-dependent field rules are
/// enforced at load time rather than deferred to a validation pass.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", try_from = "RawResidualRecord")]
pub struct ResidualRecord {
    /// `res-<slug>`.
    pub id: String,
    /// Baseline units this residual covers; non-empty.
    pub units: Vec<String>,
    /// The program that cannot be deleted while this residual stands. Optional for
    /// `external-corpus-entry` residuals only (research D8).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub blocked_carrier: Option<String>,
    /// A specific named capability the declarative system lacks (e.g. "cross-CLI
    /// container-state snapshot comparison"). Never a vague "not supported yet".
    pub missing_capability: String,
    /// Whether this residual is queued work or a permanent exclusion (024 P1).
    #[serde(default)]
    pub disposition: ResidualDisposition,
    /// A tracked issue reference. Required for `queued`, forbidden for `permanent`
    /// (FR-055; 024 P1).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub follow_up: Option<String>,
    /// Why this unit is permanently inexpressible — names the principle or the category
    /// mismatch. Required for `permanent`, forbidden for `queued` (024 P1).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub out_of_scope_rationale: Option<String>,
    /// Behaviors still covered by the carrier, so coverage accounting stays truthful.
    #[serde(default)]
    pub behaviors: Vec<String>,
}

/// The on-disk shape, validated into [`ResidualRecord`] by [`TryFrom`].
///
/// Exists solely so the `disposition` ↔ `followUp` / `outOfScopeRationale` rules are
/// cross-field checks at deserialize time. `deny_unknown_fields` lives here, on the shape
/// that actually reads the file.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RawResidualRecord {
    id: String,
    #[serde(deserialize_with = "non_empty_vec")]
    units: Vec<String>,
    #[serde(default)]
    blocked_carrier: Option<String>,
    #[serde(deserialize_with = "non_empty_string")]
    missing_capability: String,
    #[serde(default)]
    disposition: ResidualDisposition,
    #[serde(default)]
    follow_up: Option<String>,
    #[serde(default)]
    out_of_scope_rationale: Option<String>,
    #[serde(default)]
    behaviors: Vec<String>,
}

impl TryFrom<RawResidualRecord> for ResidualRecord {
    type Error = String;

    fn try_from(raw: RawResidualRecord) -> Result<Self, Self::Error> {
        let follow_up = present_value(raw.follow_up.as_deref(), "followUp")?;
        let rationale =
            present_value(raw.out_of_scope_rationale.as_deref(), "outOfScopeRationale")?;

        match raw.disposition {
            ResidualDisposition::Queued => {
                if follow_up.is_none() {
                    return Err(format!(
                        "residual `{}` is `queued` but has no `followUp` — a residual \
                         without a tracked follow-up is an open-ended excuse rather than \
                         queued work (FR-055). If it can never be migrated, set \
                         `\"disposition\": \"permanent\"` and give an `outOfScopeRationale`.",
                        raw.id
                    ));
                }
                if rationale.is_some() {
                    return Err(format!(
                        "residual `{}` is `queued` but carries an `outOfScopeRationale` — \
                         a rationale states why a unit is PERMANENTLY inexpressible, so it \
                         contradicts queued work. Set `\"disposition\": \"permanent\"` (and \
                         drop `followUp`) if that is what was meant.",
                        raw.id
                    ));
                }
            }
            ResidualDisposition::Permanent => {
                if rationale.is_none() {
                    return Err(format!(
                        "residual `{}` is `permanent` but has no `outOfScopeRationale` — a \
                         permanent exclusion must name the principle or category mismatch \
                         that forbids expressing it (e.g. \"feature authoring is out of \
                         scope, Constitution II\"), never assert itself.",
                        raw.id
                    ));
                }
                if follow_up.is_some() {
                    return Err(format!(
                        "residual `{}` is `permanent` but carries a `followUp` — a permanent \
                         exclusion has nothing to track, and a tracked reference promises \
                         work that cannot happen. Drop `followUp`, or make it `queued`.",
                        raw.id
                    ));
                }
            }
        }

        Ok(ResidualRecord {
            id: raw.id,
            units: raw.units,
            blocked_carrier: raw.blocked_carrier,
            missing_capability: raw.missing_capability,
            disposition: raw.disposition,
            follow_up,
            out_of_scope_rationale: rationale,
            behaviors: raw.behaviors,
        })
    }
}

/// Normalize an optional field to `None` when absent, rejecting a present-but-blank or
/// sentinel-carrying value. A field written as `""` is an authoring mistake, not an
/// intentional absence — treating the two alike would let a blank `followUp` satisfy the
/// `queued` requirement.
fn present_value(value: Option<&str>, field: &str) -> Result<Option<String>, String> {
    match value {
        None => Ok(None),
        Some(raw) => {
            let trimmed = raw.trim();
            if trimmed.is_empty() {
                return Err(format!(
                    "`{field}` is present but blank — omit the field entirely to mean \
                     \"absent\", so a blank value can never stand in for a real one"
                ));
            }
            if trimmed == UNREVIEWED_SENTINEL {
                return Err(format!(
                    "`{field}` carries the `{UNREVIEWED_SENTINEL}` scaffold sentinel — a \
                     scaffolded record must be reviewed and filled in before it is committed"
                ));
            }
            Ok(Some(raw.to_string()))
        }
    }
}

/// The `residuals.json` envelope — a `records` collection, matching every other
/// hand-authored registry file.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ResidualFile {
    #[serde(default)]
    pub records: Vec<ResidualRecord>,
}

/// The sentinel `migration scaffold` emits into every skeleton field. The loader
/// REJECTS it, so scaffolded output can never be committed unedited — mirroring
/// `inventory scaffold` / `clause scaffold`.
pub const UNREVIEWED_SENTINEL: &str = "UNREVIEWED";

/// Reject an empty/whitespace-only string — or the scaffold sentinel — at deserialize
/// time, so a residual can never load carrying a blank or unreviewed required field
/// (constitution IV — fail fast, no silent fallback to `""`).
fn non_empty_string<'de, D>(de: D) -> Result<String, D::Error>
where
    D: Deserializer<'de>,
{
    let value = String::deserialize(de)?;
    if value.trim().is_empty() {
        return Err(serde::de::Error::custom(
            "must be a non-empty value: a residual without a named missing capability and \
             a tracked follow-up is an open-ended excuse, not queued work (FR-055)",
        ));
    }
    if value.trim() == UNREVIEWED_SENTINEL {
        return Err(serde::de::Error::custom(format!(
            "carries the `{UNREVIEWED_SENTINEL}` scaffold sentinel — a scaffolded record \
             must be reviewed and filled in before it is committed"
        )));
    }
    Ok(value)
}

/// Reject an empty list at deserialize time — a residual covering no unit accounts for
/// nothing.
fn non_empty_vec<'de, D>(de: D) -> Result<Vec<String>, D::Error>
where
    D: Deserializer<'de>,
{
    let value = Vec::<String>::deserialize(de)?;
    if value.is_empty() {
        return Err(serde::de::Error::custom(
            "must list at least one baseline unit — a residual that covers no unit \
             accounts for nothing",
        ));
    }
    Ok(value)
}

/// Load `residuals.json` from `path`.
///
/// A missing file yields an empty list (the file arrives with User Story 2; loading
/// before it is authored is not an error). A present-but-malformed file is a located
/// [`LoadError::Schema`] — never silently empty (constitution IV).
pub fn load_residuals(path: &Path) -> Result<Vec<ResidualRecord>, LoadError> {
    if !path.exists() {
        return Ok(Vec::new());
    }
    let raw = read_file(path).map_err(|e| LoadError::Schema(vec![e]))?;
    let file: ResidualFile =
        deserialize_located(path, &raw).map_err(|e| LoadError::Schema(vec![e]))?;
    Ok(file.records)
}

/// Collect duplicate-`id` residuals as located schema errors, so two records can never
/// silently claim the same identity.
pub(crate) fn duplicate_id_errors(path: &Path, records: &[ResidualRecord]) -> Vec<SchemaError> {
    let mut seen: std::collections::HashSet<&str> = std::collections::HashSet::new();
    let mut out = Vec::new();
    for record in records {
        if !seen.insert(record.id.as_str()) {
            out.push(SchemaError {
                file: path.to_path_buf(),
                location: Some(record.id.clone()),
                message: format!("duplicate residual id `{}`", record.id),
            });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const GOOD: &str = r##"{
      "records": [
        {
          "id": "res-state-diff-cross-cli-snapshot",
          "units": ["parity_state_diff::single-container-parity"],
          "blockedCarrier": "parity_state_diff",
          "missingCapability": "cross-CLI container-state snapshot comparison",
          "followUp": "#999",
          "behaviors": ["bhv-up-container-state"]
        }
      ]
    }"##;

    #[test]
    fn well_formed_residual_loads() {
        let file: ResidualFile = serde_json::from_str(GOOD).expect("well-formed residual loads");
        let record = &file.records[0];
        assert_eq!(record.id, "res-state-diff-cross-cli-snapshot");
        assert_eq!(record.blocked_carrier.as_deref(), Some("parity_state_diff"));
        assert_eq!(record.units.len(), 1);
    }

    #[test]
    fn external_corpus_residual_may_omit_the_blocked_carrier() {
        let raw = r##"{
          "records": [
            {
              "id": "res-realworld-corpus",
              "units": ["realworld::images-python"],
              "missingCapability": "vendored fixtures for network-fetched third-party workspaces",
              "followUp": "#1000"
            }
          ]
        }"##;
        let file: ResidualFile = serde_json::from_str(raw).expect("carrier-free residual loads");
        assert!(file.records[0].blocked_carrier.is_none());
    }

    #[test]
    fn empty_missing_capability_is_rejected_at_deserialize_time() {
        let raw = GOOD.replace("cross-CLI container-state snapshot comparison", "   ");
        let err = serde_json::from_str::<ResidualFile>(&raw)
            .expect_err("a blank missingCapability must not load");
        assert!(
            err.to_string().contains("non-empty"),
            "the diagnosis must name the requirement, got: {err}"
        );
    }

    #[test]
    fn missing_follow_up_is_rejected() {
        let raw = GOOD.replace("\"followUp\": \"#999\",", "");
        assert!(
            serde_json::from_str::<ResidualFile>(&raw).is_err(),
            "followUp is required for a queued residual (FR-055)"
        );
        let blank = GOOD.replace("\"#999\"", "\"  \"");
        assert!(
            serde_json::from_str::<ResidualFile>(&blank).is_err(),
            "a blank followUp is an authoring mistake, not an intentional absence"
        );
    }

    /// A record with no `disposition` keeps its pre-024 meaning, so the field can be
    /// added without rewriting every committed record.
    #[test]
    fn disposition_defaults_to_queued() {
        let file: ResidualFile = serde_json::from_str(GOOD).expect("loads");
        assert_eq!(file.records[0].disposition, ResidualDisposition::Queued);
        assert!(!file.records[0].disposition.is_permanent());
    }

    #[test]
    fn a_permanent_residual_requires_a_rationale_and_forbids_a_follow_up() {
        // Permanent + rationale, no followUp: the well-formed shape.
        let good_permanent = GOOD.replace(
            "\"followUp\": \"#999\",",
            "\"disposition\": \"permanent\",\n          \"outOfScopeRationale\": \
             \"feature authoring is permanently out of scope (Constitution II)\",",
        );
        let file: ResidualFile = serde_json::from_str(&good_permanent)
            .expect("a permanent residual with a rationale loads");
        let record = &file.records[0];
        assert!(record.disposition.is_permanent());
        assert!(record.follow_up.is_none());
        assert!(record.out_of_scope_rationale.is_some());

        // Permanent with no rationale: it would assert itself.
        let no_rationale =
            GOOD.replace("\"followUp\": \"#999\",", "\"disposition\": \"permanent\",");
        let err = serde_json::from_str::<ResidualFile>(&no_rationale)
            .expect_err("a permanent residual must name why it is out of scope");
        assert!(
            err.to_string().contains("outOfScopeRationale"),
            "the diagnosis must name the missing field, got: {err}"
        );

        // Permanent AND a followUp: promises work that cannot happen.
        let both = GOOD.replace(
            "\"followUp\": \"#999\",",
            "\"disposition\": \"permanent\",\n          \"outOfScopeRationale\": \"out of scope\",\n \
             \"followUp\": \"#999\",",
        );
        let err = serde_json::from_str::<ResidualFile>(&both)
            .expect_err("a permanent residual has nothing to track");
        assert!(
            err.to_string().contains("followUp"),
            "the diagnosis must name the contradiction, got: {err}"
        );
    }

    #[test]
    fn a_queued_residual_may_not_carry_an_out_of_scope_rationale() {
        let raw = GOOD.replace(
            "\"followUp\": \"#999\",",
            "\"followUp\": \"#999\",\n          \"outOfScopeRationale\": \"out of scope\",",
        );
        let err = serde_json::from_str::<ResidualFile>(&raw)
            .expect_err("a rationale contradicts queued work");
        assert!(
            err.to_string().contains("outOfScopeRationale"),
            "the diagnosis must name the contradicting field, got: {err}"
        );
    }

    #[test]
    fn the_scaffold_sentinel_is_rejected_in_either_optional_field() {
        let sentinel_follow_up = GOOD.replace("\"#999\"", "\"UNREVIEWED\"");
        assert!(
            serde_json::from_str::<ResidualFile>(&sentinel_follow_up).is_err(),
            "a scaffolded followUp must not load"
        );
        let sentinel_rationale = GOOD.replace(
            "\"followUp\": \"#999\",",
            "\"disposition\": \"permanent\",\n          \"outOfScopeRationale\": \"UNREVIEWED\",",
        );
        assert!(
            serde_json::from_str::<ResidualFile>(&sentinel_rationale).is_err(),
            "a scaffolded outOfScopeRationale must not load"
        );
    }

    #[test]
    fn empty_units_is_rejected() {
        let raw = GOOD.replace("[\"parity_state_diff::single-container-parity\"]", "[]");
        assert!(
            serde_json::from_str::<ResidualFile>(&raw).is_err(),
            "a residual covering no unit accounts for nothing"
        );
    }

    #[test]
    fn unknown_fields_are_rejected() {
        let raw = GOOD.replace("\"behaviors\":", "\"surprise\":");
        assert!(serde_json::from_str::<ResidualFile>(&raw).is_err());
    }

    #[test]
    fn missing_file_loads_empty_and_malformed_file_fails_loud() {
        let dir = tempfile::tempdir().expect("tempdir");
        let absent = dir.path().join("residuals.json");
        assert!(
            load_residuals(&absent)
                .expect("absent file is not an error")
                .is_empty(),
            "an absent residuals.json is empty, not an error"
        );

        let bad = dir.path().join("bad.json");
        std::fs::write(&bad, "{ not json").expect("write fixture");
        assert!(
            load_residuals(&bad).is_err(),
            "a malformed residuals.json must fail loud, never default to empty"
        );
    }

    #[test]
    fn duplicate_ids_are_reported() {
        let file: ResidualFile = serde_json::from_str(GOOD).expect("loads");
        let mut records = file.records.clone();
        records.push(file.records[0].clone());
        let errors = duplicate_id_errors(Path::new("residuals.json"), &records);
        assert_eq!(errors.len(), 1);
        assert!(
            errors[0]
                .message
                .contains("res-state-diff-cross-cli-snapshot")
        );
        assert!(duplicate_id_errors(Path::new("residuals.json"), &file.records).is_empty());
    }
}
