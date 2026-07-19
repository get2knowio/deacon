//! Record model for the repository-owned conformance registry (data-model.md).
//!
//! Every record type here is a strict-JSON schema: `#[serde(deny_unknown_fields)]`
//! plus `rename_all = "camelCase"` so the on-disk field names match the contract
//! (`verifiedAgainst`, `outOfScope`, `schemaVersion`, …). The registry is a
//! deacon-owned modeled format — constitution IV's "strict on mistakes" side
//! applies: an unknown or misnamed field is a hard schema failure at load, never a
//! silent drop (contracts/registry-schema.md).
//!
//! Closed enums use `rename_all` so their JSON spellings are exactly the closed
//! sets in contracts/registry-schema.md. `Waiver`'s [`Scope`]/[`Expect`] shapes
//! are byte-for-byte the parity-harness waiver schema so `parity-harness` can later
//! consume registry waiver records through this crate's loader without a second
//! schema (plan.md; research Decision 3).
//!
//! Identity rules (all record types) live in [`RecordType`] / [`parse_id`]: the ID
//! format regex `^(rev|src|dim|chan|prof|bhv|case|gap|wvr|ext)-[a-z0-9]+(-[a-z0-9]+)*$`
//! and the prefix↔type agreement helper. Duplicate/sort checks (V2) land in T006 —
//! this module only models and parses IDs.

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// The collection-file wrapper: `{ "schemaVersion": 1, "records": [ … ] }`.
///
/// Every registry collection file carries this envelope EXCEPT per-waiver files,
/// which are a single [`Waiver`] object (parity-waiver compatibility).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Collection<T> {
    /// Schema version of this file. Bumped only on a breaking schema evolution.
    pub schema_version: u32,
    /// The records in this file (validated to be ID-sorted in a later phase, V2).
    pub records: Vec<T>,
}

/// The ten record types, one per stable ID prefix (data-model.md Identity rules).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RecordType {
    Revision,
    Source,
    Dimension,
    Channel,
    Profile,
    Behavior,
    Case,
    Gap,
    Waiver,
    Extension,
}

impl RecordType {
    /// The stable ID prefix for this record type (without the trailing `-`).
    pub const fn prefix(self) -> &'static str {
        match self {
            RecordType::Revision => "rev",
            RecordType::Source => "src",
            RecordType::Dimension => "dim",
            RecordType::Channel => "chan",
            RecordType::Profile => "prof",
            RecordType::Behavior => "bhv",
            RecordType::Case => "case",
            RecordType::Gap => "gap",
            RecordType::Waiver => "wvr",
            RecordType::Extension => "ext",
        }
    }

    /// The record type owning `prefix`, or `None` if it is not a known prefix.
    pub fn from_prefix(prefix: &str) -> Option<RecordType> {
        Some(match prefix {
            "rev" => RecordType::Revision,
            "src" => RecordType::Source,
            "dim" => RecordType::Dimension,
            "chan" => RecordType::Channel,
            "prof" => RecordType::Profile,
            "bhv" => RecordType::Behavior,
            "case" => RecordType::Case,
            "gap" => RecordType::Gap,
            "wvr" => RecordType::Waiver,
            "ext" => RecordType::Extension,
            _ => return None,
        })
    }
}

/// Why a stable ID is malformed. All variants carry the offending id so callers can
/// surface a precise message (constitution IV).
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum IdError {
    /// The id does not match `<prefix>-<segment>(-<segment>)*` — empty segments,
    /// uppercase, or disallowed characters.
    #[error(
        "id {id:?} is malformed: expected \
         `^(rev|src|dim|chan|prof|bhv|case|gap|wvr|ext)-[a-z0-9]+(-[a-z0-9]+)*$`"
    )]
    Format { id: String },
    /// The id is well-formed but its leading segment is not a known record prefix.
    #[error("id {id:?} has unknown record prefix {prefix:?}")]
    UnknownPrefix { id: String, prefix: String },
}

/// Whether a single ID segment matches `[a-z0-9]+` (non-empty, lowercase
/// alphanumeric only). This is the per-segment half of the ID regex.
fn is_valid_segment(segment: &str) -> bool {
    !segment.is_empty()
        && segment
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit())
}

/// Parse and validate a stable ID against the registry ID regex, returning the
/// [`RecordType`] its prefix denotes.
///
/// This is BOTH the format validator and the prefix↔type agreement helper: a caller
/// holding a record of known type checks agreement with
/// `parse_id(id)? == expected_type`. Enforcing this across the registry (V2) lands
/// in T006; this function is the reusable primitive.
pub fn parse_id(id: &str) -> Result<RecordType, IdError> {
    // Split on '-': the first segment is the prefix, and there MUST be at least one
    // further segment. Every segment must be non-empty `[a-z0-9]+`, which also
    // rejects leading/trailing/double hyphens (they produce empty segments).
    let mut segments = id.split('-');
    let prefix = segments.next().unwrap_or_default();
    let rest: Vec<&str> = segments.collect();
    if rest.is_empty() {
        return Err(IdError::Format { id: id.to_string() });
    }
    if !is_valid_segment(prefix) || !rest.iter().all(|s| is_valid_segment(s)) {
        return Err(IdError::Format { id: id.to_string() });
    }
    RecordType::from_prefix(prefix).ok_or_else(|| IdError::UnknownPrefix {
        id: id.to_string(),
        prefix: prefix.to_string(),
    })
}

// ---------------------------------------------------------------------------
// Closed enumerations (contracts/registry-schema.md "Enumerations")
// ---------------------------------------------------------------------------

/// `SourceRevision.kind` — the four pinned source revision kinds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RevisionKind {
    Spec,
    Schema,
    Oracle,
    CliSurface,
}

/// `SourceUnit.inventory` — which of the four source inventories a unit belongs to.
/// MUST match the file the unit lives in (violation V-class, later phase).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Inventory {
    Schema,
    Spec,
    Cli,
    Observed,
}

/// `BehaviorUnit.spec` — conformance disposition against the normative spec (FR-009).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SpecStatus {
    Conformant,
    Nonconformant,
    Unspecified,
    NotApplicable,
}

/// `BehaviorUnit.reference` — alignment with the active profile's oracle (FR-010).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ReferenceStatus {
    Aligned,
    Divergent,
    Unknown,
    NotApplicable,
}

/// `BehaviorUnit.decision` — the project's recorded decision (FR-011).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Decision {
    FollowSpec,
    AlignWithReference,
    DeaconExtension,
    IntentionalDivergence,
    UnresolvedGap,
}

/// `Gap.kind` — the nature of a known gap.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum GapKind {
    /// No executable case yet.
    Coverage,
    /// Reference behavior is unknown.
    Knowledge,
    /// deacon lacks the behavior.
    Implementation,
}

// ---------------------------------------------------------------------------
// Records
// ---------------------------------------------------------------------------

/// A pinned upstream source revision (`rev-`) — `revisions.json`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SourceRevision {
    pub id: String,
    pub kind: RevisionKind,
    /// Commit SHA, semver, or equivalent immutable identifier.
    pub pin: String,
    /// Upstream location (informational).
    pub url: String,
    /// Repo-local machine-readable pin this must match (e.g.
    /// `fixtures/parity-corpus/oracle.json`); staleness check V7 (later phase).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub verified_against: Option<String>,
}

/// An explicit out-of-scope classification for a [`SourceUnit`] with no behaviors.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OutOfScope {
    pub reason: String,
}

/// One source-inventory unit (`src-`) — `sources/*.json`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SourceUnit {
    pub id: String,
    /// MUST match the inventory file this unit lives in.
    pub inventory: Inventory,
    /// Provenance anchor — a `rev-` id.
    pub revision: String,
    /// Where within the source (JSON pointer, section heading, flag, corpus case).
    pub locator: String,
    /// One-sentence statement of the requirement/observation.
    pub summary: String,
    /// Many-to-many links to `bhv-` records. May be empty ONLY if `out_of_scope`
    /// is set (violation V4 otherwise, later phase).
    #[serde(default)]
    pub behaviors: Vec<String>,
    /// Explicit out-of-scope classification; absence + empty `behaviors` = V4.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub out_of_scope: Option<OutOfScope>,
}

/// A context dimension and its closed value set (`dim-`) — `dimensions.json`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ContextDimension {
    pub id: String,
    /// Closed enumerated set (`linux`, `amd64`, `docker`, `0.87.0`, …).
    pub values: Vec<String>,
}

/// An observable channel (`chan-`) — `channels.json`. Closed set: an outcome
/// referencing an undeclared channel is violation V9 (later phase).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ObservableChannel {
    pub id: String,
    pub description: String,
}

/// A certification profile (`prof-`) — `profiles.json`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CertificationProfile {
    pub id: String,
    /// Assigns every declared dimension exactly one declared value. `IndexMap`
    /// preserves declaration order for deterministic serialization (constitution
    /// VI ordering rule).
    pub context: IndexMap<String, String>,
    /// Exactly one profile is active for validation/coverage in this feature.
    pub active: bool,
}

/// An applicability / context condition: a dimension pinned to a value subset.
/// An empty `applicability`/`context` array means "applies everywhere".
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Condition {
    /// A `dim-` id.
    pub dimension: String,
    /// A subset of that dimension's declared values.
    pub values: Vec<String>,
}

/// A normalized behavior unit (`bhv-`) — `behaviors/<area>.json`.
///
/// The `sources` field in data-model.md is DERIVED (the inverse of
/// `SourceUnit.behaviors`) and therefore not stored here. All three disposition
/// fields (`spec`/`reference`/`decision`) are mandatory (FR-012).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BehaviorUnit {
    pub id: String,
    /// Grouping key; matches the file name (`behaviors/<area>.json`).
    pub area: String,
    /// Normalized, externally observable behavior statement.
    pub statement: String,
    /// Applicability conditions; empty array = applicable in every context.
    #[serde(default)]
    pub applicability: Vec<Condition>,
    pub spec: SpecStatus,
    pub reference: ReferenceStatus,
    pub decision: Decision,
    /// Rationale, issue links.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
}

/// The executable-test reference of a [`TestCase`] (research Decision 9). `binary`
/// must exist as a test file under `crates/*/tests/` (V1, later phase).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Executable {
    /// The nextest test-binary name (a `crates/*/tests/<binary>.rs`).
    pub binary: String,
    /// Optional test-function filter within the binary.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub test: Option<String>,
    /// Optional corpus id for corpus-driven binaries.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub corpus: Option<String>,
    /// Optional corpus case name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub case: Option<String>,
}

/// One expected outcome on an observable channel (inline in [`TestCase`]).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ExpectedOutcome {
    /// A `chan-` id; must be declared (V9).
    pub channel: String,
    pub expectation: String,
}

/// An executable-test reference record (`case-`) — `cases.json`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TestCase {
    pub id: String,
    /// Linked behaviors; ≥1 required (empty = orphan, violation V3).
    #[serde(default)]
    pub behaviors: Vec<String>,
    /// Declared context; must intersect every linked behavior's applicability (V10).
    #[serde(default)]
    pub context: Vec<Condition>,
    pub executable: Executable,
    /// Expected outcomes; ≥1 required.
    #[serde(default)]
    pub outcomes: Vec<ExpectedOutcome>,
}

/// A known gap (`gap-`) — `gaps.json`. Gaps satisfy structural coverage (V5) but
/// ALWAYS fail strict certification (FR-020, FR-025). No expiry.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Gap {
    pub id: String,
    pub kind: GapKind,
    /// Linked behaviors; ≥1.
    #[serde(default)]
    pub behaviors: Vec<String>,
    pub description: String,
    /// Issue link.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tracking: Option<String>,
}

/// What a waiver attaches to (tagged union on `kind`).
///
/// Byte-for-byte the parity-harness `Scope`: `rename_all = "snake_case"` tags
/// (`corpus_case`, `state_field`) so `parity-harness` can consume registry waivers
/// through this crate's loader unchanged (research Decision 3).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Scope {
    /// A single corpus case's accept/reject (and, when both accept, value) outcome.
    CorpusCase { corpus: String, case: String },
    /// A single observable-state field on a named fixture of a named binary.
    /// `field` supports an exact match or a trailing-`*` prefix.
    StateField {
        binary: String,
        fixture: String,
        field: String,
    },
}

/// The characterized outcome of a waiver (tagged union on `kind`).
///
/// Byte-for-byte the parity-harness `Expect`: `rename_all = "kebab-case"` tags
/// (`both-reject`, `both-accept`, `deacon-stricter`, `reference-stricter`,
/// `field-divergence`). Not `Eq`: `FieldDivergence` carries arbitrary JSON.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
pub enum Expect {
    /// Both CLIs reject the input. Empty struct variant (not unit) so
    /// `deny_unknown_fields` also rejects a stray sibling key.
    BothReject {},
    /// Both CLIs accept; resolved values are compared normally.
    BothAccept {},
    /// deacon rejects where the reference accepts — an intentional strictness
    /// divergence (constitution IV). Optional informational stderr `signal`.
    DeaconStricter {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        signal: Option<Vec<String>>,
    },
    /// deacon accepts where the reference rejects — an intentional ahead-of-spec
    /// capability. Optional informational stderr `signal`.
    ReferenceStricter {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        signal: Option<Vec<String>>,
    },
    /// A specific normalized-value difference is expected between the two CLIs.
    FieldDivergence { ours: Value, reference: Value },
}

impl Expect {
    /// Whether this expectation characterizes a divergence (deacon-stricter /
    /// reference-stricter / field-divergence) rather than an agreement.
    pub fn is_divergence(&self) -> bool {
        matches!(
            self,
            Expect::DeaconStricter { .. }
                | Expect::ReferenceStricter { .. }
                | Expect::FieldDivergence { .. }
        )
    }
}

/// A migrated parity waiver, extended with registry links + expiry (`wvr-`) —
/// `waivers/<id>.json` (a single record object, not a [`Collection`]).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Waiver {
    pub id: String,
    /// Linked behaviors; ≥1.
    #[serde(default)]
    pub behaviors: Vec<String>,
    /// Harness-consumable scope, preserved from the parity schema.
    pub scope: Scope,
    /// Preserved parity expectation.
    pub expect: Expect,
    /// Required, non-empty rationale.
    pub rationale: String,
    /// ISO `YYYY-MM-DD`.
    pub added: String,
    /// ISO `YYYY-MM-DD`; `expires < today` → violation V6 (boundary passes).
    pub expires: String,
    /// Schema-known optional case input (an explicit `--config` argument) carried
    /// over verbatim from the legacy parity `expect.json` shape; plays no part in
    /// waiver semantics. Preserved so every migrated record round-trips.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub config: Option<String>,
}

/// An intentional Deacon extension (`ext-`) — `extensions.json`. Each linked
/// behavior MUST have `decision: deacon-extension` (V8 consistency, later phase).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DeaconExtension {
    pub id: String,
    /// Linked behaviors; ≥1.
    #[serde(default)]
    pub behaviors: Vec<String>,
    pub description: String,
    /// Pointer (e.g. `SECURITY.md`, `docs/DIFFERENTIATORS.md`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub docs: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // ---- ID parsing --------------------------------------------------------

    #[test]
    fn parse_id_accepts_well_formed_ids_and_returns_type() {
        assert_eq!(parse_id("rev-spec-113500f4").unwrap(), RecordType::Revision);
        assert_eq!(parse_id("rev-oracle-0-87-0").unwrap(), RecordType::Revision);
        assert_eq!(parse_id("src-obs-malformed").unwrap(), RecordType::Source);
        assert_eq!(parse_id("dim-os").unwrap(), RecordType::Dimension);
        assert_eq!(parse_id("chan-stdout").unwrap(), RecordType::Channel);
        assert_eq!(
            parse_id("prof-linux-amd64-docker-0870").unwrap(),
            RecordType::Profile
        );
        assert_eq!(
            parse_id("bhv-readconfig-malformed-jsonc-rejected").unwrap(),
            RecordType::Behavior
        );
        assert_eq!(parse_id("case-parity-corpus").unwrap(), RecordType::Case);
        assert_eq!(parse_id("gap-compose-marker").unwrap(), RecordType::Gap);
        assert_eq!(parse_id("wvr-extends-child").unwrap(), RecordType::Waiver);
        assert_eq!(
            parse_id("ext-workspace-trust").unwrap(),
            RecordType::Extension
        );
    }

    #[test]
    fn parse_id_rejects_malformed_ids() {
        // No segment after the prefix.
        assert!(matches!(parse_id("rev"), Err(IdError::Format { .. })));
        // Trailing / leading / double hyphen → empty segment.
        assert!(matches!(parse_id("rev-"), Err(IdError::Format { .. })));
        assert!(matches!(parse_id("-rev-x"), Err(IdError::Format { .. })));
        assert!(matches!(parse_id("rev--x"), Err(IdError::Format { .. })));
        // Uppercase and disallowed characters.
        assert!(matches!(parse_id("rev-Spec"), Err(IdError::Format { .. })));
        assert!(matches!(
            parse_id("rev-spec_x"),
            Err(IdError::Format { .. })
        ));
        assert!(matches!(
            parse_id("rev-spec.x"),
            Err(IdError::Format { .. })
        ));
        assert!(matches!(parse_id(""), Err(IdError::Format { .. })));
    }

    #[test]
    fn parse_id_rejects_unknown_prefix() {
        let err = parse_id("xyz-thing").unwrap_err();
        assert!(matches!(err, IdError::UnknownPrefix { .. }));
        assert!(err.to_string().contains("xyz"));
    }

    #[test]
    fn prefix_round_trips_for_every_record_type() {
        for ty in [
            RecordType::Revision,
            RecordType::Source,
            RecordType::Dimension,
            RecordType::Channel,
            RecordType::Profile,
            RecordType::Behavior,
            RecordType::Case,
            RecordType::Gap,
            RecordType::Waiver,
            RecordType::Extension,
        ] {
            assert_eq!(RecordType::from_prefix(ty.prefix()), Some(ty));
            // A valid id built from the prefix parses back to the same type
            // (prefix↔type agreement).
            let id = format!("{}-x", ty.prefix());
            assert_eq!(parse_id(&id).unwrap(), ty);
        }
    }

    // ---- Enum serde round-trips -------------------------------------------

    fn round_trip<T>(value: T, expected_json: serde_json::Value)
    where
        T: Serialize + for<'de> Deserialize<'de> + PartialEq + std::fmt::Debug,
    {
        let serialized = serde_json::to_value(&value).unwrap();
        assert_eq!(serialized, expected_json, "serialized spelling mismatch");
        let back: T = serde_json::from_value(expected_json).unwrap();
        assert_eq!(back, value, "round-trip mismatch");
    }

    #[test]
    fn revision_kind_spellings() {
        round_trip(RevisionKind::Spec, json!("spec"));
        round_trip(RevisionKind::Schema, json!("schema"));
        round_trip(RevisionKind::Oracle, json!("oracle"));
        round_trip(RevisionKind::CliSurface, json!("cli-surface"));
    }

    #[test]
    fn inventory_spellings() {
        round_trip(Inventory::Schema, json!("schema"));
        round_trip(Inventory::Spec, json!("spec"));
        round_trip(Inventory::Cli, json!("cli"));
        round_trip(Inventory::Observed, json!("observed"));
    }

    #[test]
    fn spec_status_spellings() {
        round_trip(SpecStatus::Conformant, json!("conformant"));
        round_trip(SpecStatus::Nonconformant, json!("nonconformant"));
        round_trip(SpecStatus::Unspecified, json!("unspecified"));
        round_trip(SpecStatus::NotApplicable, json!("not-applicable"));
    }

    #[test]
    fn reference_status_spellings() {
        round_trip(ReferenceStatus::Aligned, json!("aligned"));
        round_trip(ReferenceStatus::Divergent, json!("divergent"));
        round_trip(ReferenceStatus::Unknown, json!("unknown"));
        round_trip(ReferenceStatus::NotApplicable, json!("not-applicable"));
    }

    #[test]
    fn decision_spellings() {
        round_trip(Decision::FollowSpec, json!("follow-spec"));
        round_trip(Decision::AlignWithReference, json!("align-with-reference"));
        round_trip(Decision::DeaconExtension, json!("deacon-extension"));
        round_trip(
            Decision::IntentionalDivergence,
            json!("intentional-divergence"),
        );
        round_trip(Decision::UnresolvedGap, json!("unresolved-gap"));
    }

    #[test]
    fn gap_kind_spellings() {
        round_trip(GapKind::Coverage, json!("coverage"));
        round_trip(GapKind::Knowledge, json!("knowledge"));
        round_trip(GapKind::Implementation, json!("implementation"));
    }

    #[test]
    fn scope_matches_parity_snake_case_tags() {
        let corpus = Scope::CorpusCase {
            corpus: "errors".into(),
            case: "malformed-json".into(),
        };
        round_trip(
            corpus,
            json!({ "kind": "corpus_case", "corpus": "errors", "case": "malformed-json" }),
        );
        let field = Scope::StateField {
            binary: "parity_observable_state".into(),
            fixture: "compose-postgres".into(),
            field: "label:com.docker.compose.project*".into(),
        };
        round_trip(
            field,
            json!({
                "kind": "state_field",
                "binary": "parity_observable_state",
                "fixture": "compose-postgres",
                "field": "label:com.docker.compose.project*"
            }),
        );
    }

    #[test]
    fn expect_matches_parity_kebab_case_tags() {
        round_trip(Expect::BothReject {}, json!({ "kind": "both-reject" }));
        round_trip(Expect::BothAccept {}, json!({ "kind": "both-accept" }));
        round_trip(
            Expect::DeaconStricter { signal: None },
            json!({ "kind": "deacon-stricter" }),
        );
        round_trip(
            Expect::ReferenceStricter {
                signal: Some(vec!["image".into()]),
            },
            json!({ "kind": "reference-stricter", "signal": ["image"] }),
        );
        round_trip(
            Expect::FieldDivergence {
                ours: json!("deacon-x"),
                reference: json!("devcontainer-y"),
            },
            json!({ "kind": "field-divergence", "ours": "deacon-x", "reference": "devcontainer-y" }),
        );
        assert!(Expect::DeaconStricter { signal: None }.is_divergence());
        assert!(!Expect::BothReject {}.is_divergence());
    }

    // ---- deny_unknown_fields ----------------------------------------------

    #[test]
    fn records_reject_unknown_fields() {
        let bad_revision =
            r#"{ "id": "rev-x", "kind": "spec", "pin": "p", "url": "u", "oops": 1 }"#;
        assert!(serde_json::from_str::<SourceRevision>(bad_revision).is_err());

        let bad_behavior = r#"{
            "id": "bhv-x", "area": "a", "statement": "s",
            "spec": "conformant", "reference": "aligned", "decision": "follow-spec",
            "typo": true
        }"#;
        assert!(serde_json::from_str::<BehaviorUnit>(bad_behavior).is_err());

        let bad_collection = r#"{ "schemaVersion": 1, "records": [], "extra": 0 }"#;
        assert!(serde_json::from_str::<Collection<SourceRevision>>(bad_collection).is_err());
    }

    #[test]
    fn behavior_requires_all_three_axes() {
        // Missing `decision` → deserialization error (all three axes mandatory).
        let missing_axis = r#"{
            "id": "bhv-x", "area": "a", "statement": "s",
            "spec": "conformant", "reference": "aligned"
        }"#;
        assert!(serde_json::from_str::<BehaviorUnit>(missing_axis).is_err());
    }

    #[test]
    fn collection_and_camel_case_fields_round_trip() {
        let revision = SourceRevision {
            id: "rev-oracle-0-87-0".into(),
            kind: RevisionKind::Oracle,
            pin: "0.87.0".into(),
            url: "https://example".into(),
            verified_against: Some("fixtures/parity-corpus/oracle.json".into()),
        };
        let collection = Collection {
            schema_version: 1,
            records: vec![revision],
        };
        let value = serde_json::to_value(&collection).unwrap();
        assert_eq!(value["schemaVersion"], json!(1));
        assert_eq!(
            value["records"][0]["verifiedAgainst"],
            json!("fixtures/parity-corpus/oracle.json")
        );
        let back: Collection<SourceRevision> = serde_json::from_value(value).unwrap();
        assert_eq!(back, collection);
    }
}
