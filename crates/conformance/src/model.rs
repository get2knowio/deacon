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
//! format regex
//! `^(rev|src|dim|chan|prof|bhv|case|gap|wvr|ext|cst|cls)-[a-z0-9]+(-[a-z0-9]+)*$`
//! and the prefix↔type agreement helper. The `cst` (constraint unit) and `cls`
//! (classification) prefixes are the schema constraint inventory's record types
//! (020-schema-constraint-inventory). Duplicate/sort checks (V2) land in T006 —
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

/// The record types, one per stable ID prefix (data-model.md Identity rules).
///
/// `Constraint` (`cst`) and `Classification` (`cls`) are the schema constraint
/// inventory's record types (020-schema-constraint-inventory §5).
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
    Constraint,
    Classification,
    ClauseUnit,
    ClauseClassification,
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
            RecordType::Constraint => "cst",
            RecordType::Classification => "cls",
            RecordType::ClauseUnit => "clu",
            RecordType::ClauseClassification => "clc",
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
            "cst" => RecordType::Constraint,
            "cls" => RecordType::Classification,
            "clu" => RecordType::ClauseUnit,
            "clc" => RecordType::ClauseClassification,
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
         `^(rev|src|dim|chan|prof|bhv|case|gap|wvr|ext|cst|cls|clu|clc)-[a-z0-9]+(-[a-z0-9]+)*$`"
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

/// A conformance case record (`case-`) — `cases.json`.
///
/// A record is either **legacy** (binary-backed, with [`executable`](Self::executable)
/// and [`outcomes`](Self::outcomes), pointing at a hand-written Rust test) or
/// **declarative** (data-driven, with [`operations`](Self::operations), `oracleType`,
/// and `expected`, run by the shared conformance runner). Never both, never neither;
/// see [`TestCase::classify`] (research D2, 022-conformance-runner). The legacy and
/// declarative field blocks below are mutually exclusive, and both the loader
/// (`load.rs`) and the validator (`validate.rs`) enforce that fail-loud (FR-003).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TestCase {
    pub id: String,
    /// Linked behaviors; ≥1 required (empty = orphan, violation V3).
    #[serde(default)]
    pub behaviors: Vec<String>,
    /// Declared context; must intersect every linked behavior's applicability (V10).
    #[serde(default)]
    pub context: Vec<Condition>,

    // ---- Legacy (binary-backed) fields — present iff this is a legacy case ----
    /// The Rust test binary that exercises this case (legacy path only).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub executable: Option<Executable>,
    /// Expected outcomes on observable channels; ≥1 required for a legacy case.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub outcomes: Vec<ExpectedOutcome>,

    // ---- Declarative fields (022-conformance-runner) — present iff declarative ----
    /// The ordered actions the runner performs (declarative path only); ≥1 required.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub operations: Vec<Operation>,
    /// Which oracle this case is evaluated against (declarative path only).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub oracle_type: Option<OracleType>,
    /// Per-channel expectations captured after running the operations.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub expected: Vec<ExpectedObservable>,
    /// Scoped tolerances for characterized divergences ([`AllowedDifference`], US4).
    /// Excluded from `caseHash` (research D3) so annotating a tolerance never re-records.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub allowed_differences: Vec<AllowedDifference>,
    /// Path/glob allowlist for the filesystem channel; required iff a filesystem-channel
    /// expectation exists (keeps capture scoped, clarify Q1).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fs_allowlist: Vec<String>,
    /// Resources to reclaim after the run (success or failure).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cleanup: Option<Cleanup>,
    /// The nextest resource group for a Docker-backed case (default `none`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resource_group: Option<ResourceGroup>,
    /// Human prose; **excluded from `caseHash`** so annotating never re-records (D3).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
}

/// The shape of a [`TestCase`] record: legacy (binary-backed) or declarative
/// (data-driven), per research D2 (022-conformance-runner).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CaseKind {
    /// Binary-backed: `executable` present, no `operations`.
    Legacy,
    /// Data-driven: `operations` present, no `executable`.
    Declarative,
}

/// Why a [`TestCase`] record is malformed with respect to the legacy/declarative
/// either-or (FR-003). Rendered by [`CaseShapeError::message`] for located loader and
/// validator diagnostics (constitution IV — fail-loud).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CaseShapeError {
    /// Both `executable` (legacy) and `operations` (declarative) are present.
    Mixed,
    /// Neither `executable` nor `operations` is present.
    Neither,
}

impl CaseShapeError {
    /// A precise, human-readable diagnosis naming the mistake and the remedy.
    pub fn message(self) -> &'static str {
        match self {
            CaseShapeError::Mixed => {
                "case declares BOTH a legacy `executable` and declarative `operations`; a \
                 record must be exactly one shape (remove one)"
            }
            CaseShapeError::Neither => {
                "case declares NEITHER a legacy `executable` nor declarative `operations`; a \
                 record must be exactly one shape (add one)"
            }
        }
    }
}

impl TestCase {
    /// Classify this case as [`CaseKind::Legacy`] or [`CaseKind::Declarative`], or a
    /// [`CaseShapeError`] if it is a mixed/neither malformed record (research D2,
    /// FR-003). Legacy ⇔ has `executable`; declarative ⇔ has `operations`.
    pub fn classify(&self) -> Result<CaseKind, CaseShapeError> {
        match (self.executable.is_some(), !self.operations.is_empty()) {
            (true, true) => Err(CaseShapeError::Mixed),
            (false, false) => Err(CaseShapeError::Neither),
            (true, false) => Ok(CaseKind::Legacy),
            (false, true) => Ok(CaseKind::Declarative),
        }
    }
}

/// The consumer subcommand surface an [`Operation`] may invoke (Principle II).
///
/// A closed vocabulary, kept as a named constant so a Principle-II audit can grep it
/// and the validator (`validate.rs`) can emit a located, human-readable message for a
/// non-consumer subcommand rather than a cryptic enum-variant error (contract
/// case-schema.md: "each `operations[].subcommand` ∈ consumer surface | new V-series").
pub const CONSUMER_SUBCOMMANDS: &[&str] = &[
    "up",
    "down",
    "exec",
    "build",
    "read-configuration",
    "run-user-commands",
    "templates-apply",
    "doctor",
];

/// The observable-channel ids whose expectations require an `fsAllowlist` (data-model
/// §1 / contract observer-channel.md). Filesystem capture is allowlist-scoped, never a
/// full-tree diff (clarify Q1).
pub const FILESYSTEM_CHANNELS: &[&str] = &[CHAN_FILESYSTEM, CHAN_FILE_CONTENT];

// -- Channel id constants (data-model §4) -----------------------------------------
// Stable ids matching `conformance/registry/channels.json`. The first six are the
// pre-existing channels; the last five are added by 022-conformance-runner (T002).
/// Process exit status.
pub const CHAN_EXIT_CODE: &str = "chan-exit-code";
/// Raw stdout bytes.
pub const CHAN_STDOUT: &str = "chan-stdout";
/// Raw stderr bytes.
pub const CHAN_STDERR: &str = "chan-stderr";
/// Presence/attributes of allowlisted paths (NOT full tree).
pub const CHAN_FILESYSTEM: &str = "chan-filesystem";
/// Contents of an allowlisted file.
pub const CHAN_FILE_CONTENT: &str = "chan-file-content";
/// Container lifecycle state (retained for legacy cases).
pub const CHAN_CONTAINER_STATE: &str = "chan-container-state";
/// Parsed structured (JSON) result document, distinct from raw stdout.
pub const CHAN_STRUCTURED_OUTPUT: &str = "chan-structured-output";
/// Built-image configuration + metadata (labels parsed semantically).
pub const CHAN_IMAGE: &str = "chan-image";
/// Container + network + volume + mount graph.
pub const CHAN_PROCESS_GRAPH: &str = "chan-process-graph";
/// Env, user, cwd, PATH resolution, signals, TTY, exit propagation.
pub const CHAN_INJECTED_PROCESS: &str = "chan-injected-process";
/// Lifecycle ordering, first-create vs restart, resume, cleanup transitions.
pub const CHAN_TEMPORAL: &str = "chan-temporal";

/// Which oracle a declarative [`TestCase`] is evaluated against (data-model §1,
/// research D8). The four are semantically distinct verdicts; re-pointing a case at a
/// different target changes only this field (FR-007).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum OracleType {
    /// Compare normalized observables to the declared `expected`; no reference run.
    SpecExpectation,
    /// Compare to a committed, provenance-checked snapshot.
    Snapshot,
    /// Run deacon + the pinned reference and compare normalized observables.
    LiveDifferential,
    /// Evaluate a declared relationship across ≥2 operations (idempotence,
    /// first-create-vs-restart, resume) rather than a fixed output.
    InvariantMetamorphic,
}

/// A single action the runner performs, ordered within a [`TestCase`] (data-model §2).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Operation {
    /// Unique within the case (referenced by a metamorphic [`Relationship`]).
    pub id: String,
    /// Consumer subcommand to invoke; validated against [`CONSUMER_SUBCOMMANDS`].
    pub subcommand: String,
    /// Arguments after the subcommand; part of `caseHash`.
    #[serde(default)]
    pub argv: Vec<String>,
    /// Fixture ids this op materializes into the workspace.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fixtures: Vec<String>,
    /// Optional stdin payload.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stdin: Option<String>,
    /// For negative cases: the failure phase the op is expected to fail in.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expect_failure_phase: Option<FailurePhase>,
    /// Invariant/metamorphic only: the relationship this op asserts against a sibling.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub relationship: Option<Relationship>,
}

/// A metamorphic/invariant relationship asserted across operations (data-model §2).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Relationship {
    /// The kind of relationship being asserted.
    pub kind: RelationshipKind,
    /// The sibling operation id this relationship is evaluated against.
    pub against_op: String,
}

/// The closed set of metamorphic relationship kinds (data-model §2, FR-008).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RelationshipKind {
    /// Re-running the operation produces the same observable state.
    Idempotence,
    /// First create differs from a subsequent restart in the declared way.
    FirstCreateVsRestart,
    /// A resumed run reattaches to existing state rather than recreating it.
    Resume,
}

/// A fixture the runner materializes into the workspace (data-model §3). Inputs are
/// pinned (images by digest/tag, never `latest`); `fixtureHash` is derived from the
/// fixture bytes and feeds `caseHash` + provenance.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Fixture {
    /// Referenced by [`Operation::fixtures`].
    pub id: String,
    /// Repo-relative source path (pinned input).
    pub path: String,
    /// SHA-256 of the fixture bytes; derived, so optional on the wire.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fixture_hash: Option<String>,
}

/// A per-channel expectation on a declarative [`TestCase`] (data-model §5).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ExpectedObservable {
    /// A declared channel (`chan-…`); checked against `channels.json` (V9).
    pub channel: String,
    /// Which operation produced it (default: the last operation).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub operation: Option<String>,
    /// Channel-specific expectation shape (contract observer-channel.md). Required for
    /// `spec-expectation`; MAY be omitted for `live-differential`/`snapshot` (the
    /// reference/snapshot supplies the expectation).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub assertion: Option<Value>,
}

/// Resources the runner reclaims after a case runs, on success AND failure (data-model
/// §1, contract case-schema.md). `images` is `false` | `true` | `"case-built"`; typed
/// precisely in US5 (T052), stored as raw JSON here.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Cleanup {
    #[serde(default)]
    pub containers: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub images: Option<Value>,
    #[serde(default)]
    pub networks: bool,
    #[serde(default)]
    pub volumes: bool,
    #[serde(default)]
    pub tempdir: bool,
}

/// The nextest resource group for a Docker-backed case (data-model §1). Absence means
/// the default `none` (a hermetic case needs no Docker group).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ResourceGroup {
    /// Safe concurrent Docker usage (unique resource names).
    DockerShared,
    /// Exclusive Docker daemon access (shared state).
    DockerExclusive,
    /// Significant filesystem operations, no Docker.
    FsHeavy,
    /// No special group.
    None,
}

/// A scoped tolerance for a characterized divergence (data-model §6, research D9, US4).
///
/// A tolerated divergence is scoped to `(behavior, observablePath, context)` and backed
/// by a resolvable registry identity — exactly one of `waiverId`
/// (`conformance/registry/waivers/wvr-*`) or `divergenceId` (a `bhv-`/`ext-` intentional
/// divergence record). It applies ONLY to its `(behavior, observablePath)` (FR-033);
/// there are NO global ignore lists (FR-032). Excluded from `caseHash` (research D3).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AllowedDifference {
    /// A `bhv-…` linked by the case; the tolerance is scoped to this behavior.
    pub behavior: String,
    /// Context tags the tolerance applies under (documents where it is valid).
    #[serde(default)]
    pub context: Vec<String>,
    /// A dotted path WITHIN a channel (e.g. `chan-injected-process.env.TZ`), never a
    /// bare channel (that would be a global ignore, FR-032).
    pub observable_path: String,
    /// Why this difference is acceptable.
    pub rationale: String,
    /// Backing registry waiver id (`wvr-…`); exactly one of `waiverId`/`divergenceId`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub waiver_id: Option<String>,
    /// Backing intentional-divergence record id (`bhv-…`/`ext-…`); exactly one of
    /// `waiverId`/`divergenceId`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub divergence_id: Option<String>,
}

/// Why an [`AllowedDifference`]'s backing identity is malformed (exactly-one-of
/// `waiverId`/`divergenceId`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AllowedDifferenceIdError {
    /// Both `waiverId` and `divergenceId` are present.
    Both,
    /// Neither is present.
    Neither,
}

impl AllowedDifferenceIdError {
    /// A precise, human-readable diagnosis (constitution IV — fail-loud).
    pub fn message(self) -> &'static str {
        match self {
            AllowedDifferenceIdError::Both => {
                "allowed difference declares BOTH `waiverId` and `divergenceId`; exactly one \
                 backing identity is required"
            }
            AllowedDifferenceIdError::Neither => {
                "allowed difference declares NEITHER `waiverId` nor `divergenceId`; a backing \
                 waiver/divergence identity is required (no unbacked tolerances)"
            }
        }
    }
}

impl AllowedDifference {
    /// The single backing identity (waiver or divergence id), or an
    /// [`AllowedDifferenceIdError`] when both/neither is present.
    pub fn resolved_id(&self) -> Result<&str, AllowedDifferenceIdError> {
        match (&self.waiver_id, &self.divergence_id) {
            (Some(_), Some(_)) => Err(AllowedDifferenceIdError::Both),
            (None, None) => Err(AllowedDifferenceIdError::Neither),
            (Some(w), None) => Ok(w),
            (None, Some(d)) => Ok(d),
        }
    }

    /// Whether the `observablePath` is a BARE channel (a `chan-…` id with no dotted
    /// sub-path) or an empty/`*` wildcard — a global-ignore construct rejected by FR-032.
    /// A well-formed path is `chan-<name>.<sub.path>` (at least one dotted segment after
    /// the channel id).
    pub fn is_global_ignore(&self) -> bool {
        let p = self.observable_path.trim();
        if p.is_empty() || p == "*" {
            return true;
        }
        // Must start with a channel id and have a dotted sub-path after it.
        match p.split_once('.') {
            Some((chan, rest)) => !chan.starts_with("chan-") || rest.trim().is_empty(),
            None => true, // no dot → bare channel (or a bare token)
        }
    }
}

/// The closed set of failure phases (data-model §8, clarify Q5). Reuses deacon's
/// lifecycle/execution vocabulary — never an open string. Ordered as the run
/// progresses: config-resolution → build → container-create → lifecycle:* → exec.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum FailurePhase {
    /// Resolving/merging the devcontainer configuration.
    ConfigResolution,
    /// Building the image.
    Build,
    /// Creating the container.
    ContainerCreate,
    /// `onCreateCommand`.
    #[serde(rename = "lifecycle:onCreate")]
    LifecycleOnCreate,
    /// `updateContentCommand`.
    #[serde(rename = "lifecycle:updateContent")]
    LifecycleUpdateContent,
    /// `postCreateCommand`.
    #[serde(rename = "lifecycle:postCreate")]
    LifecyclePostCreate,
    /// `postStartCommand`.
    #[serde(rename = "lifecycle:postStart")]
    LifecyclePostStart,
    /// `postAttachCommand`.
    #[serde(rename = "lifecycle:postAttach")]
    LifecyclePostAttach,
    /// Executing a command in the container.
    Exec,
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

// ---------------------------------------------------------------------------
// Schema constraint inventory (020-schema-constraint-inventory, data-model.md §1–§3)
// ---------------------------------------------------------------------------

/// The schemas manifest — `conformance/schemas/<rev-pin>/manifest.json`
/// (data-model.md §1). Records the vendored pinned schema documents and their
/// SHA-256 fingerprints, keyed to a `schema`-kind [`SourceRevision`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SchemasManifest {
    /// Schema version of the manifest format.
    pub schema_version: u32,
    /// MUST name an existing `rev-` record of kind `schema` in `revisions.json`
    /// (V14 on mismatch).
    pub revision: String,
    /// One entry per vendored schema document, in file order.
    pub documents: Vec<ManifestDocument>,
}

/// One vendored schema document within a [`SchemasManifest`] (data-model.md §1).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ManifestDocument {
    /// Document key used in constraint IDs and diff match keys. Lowercase
    /// `[a-z0-9]+`, unique within the manifest.
    pub key: String,
    /// Filename of the vendored schema, a sibling of the manifest.
    pub file: String,
    /// Upstream URL at the pinned commit — provenance only, never fetched.
    pub upstream_url: String,
    /// SHA-256 of the vendored file bytes; verified before every parse
    /// (V14 / [`InventoryError::ManifestFingerprintMismatch`] on mismatch).
    pub sha256: String,
}

/// The generated, committed constraint inventory —
/// `conformance/inventory/constraints.json` (data-model.md §2).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ConstraintInventory {
    /// Schema version of the inventory format.
    pub schema_version: u32,
    /// Equals the manifest's revision (and therefore the registry schema pin).
    pub revision: String,
    /// Extracted constraint units, sorted by `id` in the committed artifact.
    pub units: Vec<ConstraintUnit>,
}

/// One extracted constraint facet (data-model.md §2). Identity lives in the
/// substance-and-location-derived `id`; a material change to `substance` produces a
/// new `id` (drift-forcing, research Decision 6).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ConstraintUnit {
    /// `cst-<doc>-<slug>-<kind code>-<hash8>` — grammar-valid per [`parse_id`].
    pub id: String,
    /// Manifest document key this facet was extracted from.
    pub document: String,
    /// RFC 6901 JSON Pointer to the schema object owning the facet (definition
    /// site — research Decision 3).
    pub pointer: String,
    /// The facet's constraint kind.
    pub kind: ConstraintKind,
    /// Canonicalized JSON value of the facet — the testable rule itself.
    pub substance: Value,
    /// Composition/condition context when the owning object sits inside a branch;
    /// serialized as `null` when the unit is at top level.
    #[serde(default)]
    pub context: Option<UnitContext>,
}

/// The closed constraint-kind taxonomy (data-model.md §2, research Decision 4).
/// `UnmodeledKeyword` is the fail-faithful catch-all: any keyword the extractor
/// does not model lands here rather than being dropped (constitution IV).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ConstraintKind {
    PropertyExistence,
    Required,
    Type,
    Enum,
    Const,
    Default,
    UnionAlternative,
    AllOf,
    Conditional,
    AdditionalProperties,
    ArrayShape,
    ValueShape,
    Reference,
    Annotation,
    UnmodeledKeyword,
}

/// Composition/condition context for a [`ConstraintUnit`] (data-model.md §2). An
/// untagged enum over the two documented shapes: a `oneOf`/`anyOf`/`allOf` branch
/// arm (`{ branch, index }`) or an `if`/`then`/`else` condition
/// (`{ condition }`). The inner structs carry `deny_unknown_fields` so the two
/// disjoint field sets discriminate the variant unambiguously (serde does not
/// permit `deny_unknown_fields` on the untagged enum itself).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum UnitContext {
    /// The owning object is a branch arm of a composition keyword.
    Branch(BranchContext),
    /// The owning object is the target of a conditional keyword.
    Condition(ConditionContext),
}

/// `{ "branch": "oneOf", "index": 2 }` — a composition branch arm.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BranchContext {
    /// The composition keyword owning the arm (`oneOf`, `anyOf`, `allOf`).
    pub branch: String,
    /// Zero-based arm index within that keyword's array.
    pub index: usize,
}

/// `{ "condition": "/definitions/x/if" }` — an `if`/`then`/`else` condition pointer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ConditionContext {
    /// JSON Pointer to the governing condition schema.
    pub condition: String,
}

/// A hand-authored classification record (`cls-`) —
/// `conformance/registry/classifications/{base,feature}.json` (data-model.md §3).
/// Exactly one per constraint unit; joins to the inventory by ID (V11/V12/V13).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Classification {
    /// `cls-` + the exact tail of `constraint`'s `cst-` id (structural mirror; V13
    /// if mismatched).
    pub id: String,
    /// The `cst-` constraint id this classifies; MUST exist in the committed
    /// inventory (V11 when stale).
    pub constraint: String,
    /// The disposition under the consumer-only scope.
    pub disposition: Disposition,
    /// Non-empty and every id an existing behavior iff `behavior-mapped`; MUST be
    /// absent/empty otherwise (V13).
    #[serde(default)]
    pub behaviors: Vec<String>,
    /// REQUIRED non-empty for `non-testable` / `not-applicable`; optional for
    /// `behavior-mapped` (V13).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rationale: Option<String>,
    /// Free-form notes (e.g. migration provenance).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
}

/// A classification disposition (data-model.md §3, closed set). `NotApplicable` and
/// `NonTestable` never block certification; `BehaviorMapped` requires covered
/// behaviors (research Decision 11).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Disposition {
    BehaviorMapped,
    NonTestable,
    NotApplicable,
}

// ---------------------------------------------------------------------------
// Normative clause inventory (021-normative-clause-inventory, data-model.md §1–§3)
// ---------------------------------------------------------------------------

/// The spec-prose manifest — `conformance/spec/<rev-pin>/manifest.json`
/// (data-model.md §1). Records the vendored pinned prose documents, their SHA-256
/// fingerprints, and a per-document consumer/authoring `scope`, keyed to a
/// `spec`-kind [`SourceRevision`]. The prose companion to [`SchemasManifest`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SpecManifest {
    /// Schema version of the manifest format.
    pub schema_version: u32,
    /// MUST name an existing `rev-` record of kind `spec` in `revisions.json`
    /// (V14 on mismatch).
    pub revision: String,
    /// One entry per vendored prose document, in file order.
    pub documents: Vec<SpecDocument>,
}

/// One vendored prose document within a [`SpecManifest`] (data-model.md §1).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SpecDocument {
    /// Document key used in clause IDs, classification files, and diff sort order.
    /// Lowercase `[a-z0-9-]+`, unique within the manifest.
    pub key: String,
    /// Filename of the vendored Markdown document, a sibling of the manifest.
    pub file: String,
    /// Upstream URL at the pinned commit — provenance only, never fetched.
    pub upstream_url: String,
    /// SHA-256 of the vendored file bytes; verified before every parse
    /// (V14 / [`crate::load::LoadError::SpecFingerprintMismatch`] on mismatch).
    pub sha256: String,
    /// Consumer vs authoring scope. Gates the document-scope disposition default:
    /// a document-scope `not-applicable` record is permitted only for `authoring`
    /// documents (research Decision 7; V13 otherwise).
    pub scope: DocumentScope,
}

/// A vendored prose document's scope under deacon's consumer-only mandate
/// (constitution II). `authoring` documents may carry a document-scope disposition
/// default (research Decision 7).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum DocumentScope {
    /// A consumer-facing document — every clause is classified per-clause.
    Consumer,
    /// An authoring/distribution document — a document-scope not-applicable default
    /// is permitted (with per-clause overrides for consumer install/apply clauses).
    Authoring,
}

/// The generated, committed clause inventory —
/// `conformance/inventory/clauses.json` (data-model.md §2). The prose companion to
/// [`ConstraintInventory`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ClauseInventory {
    /// Schema version of the inventory format.
    pub schema_version: u32,
    /// Equals the manifest's revision (and therefore the registry spec pin).
    pub revision: String,
    /// Canonicalized clause units, sorted by `id` in the committed artifact.
    pub units: Vec<ClauseUnit>,
}

/// One atomic normative clause (data-model.md §2). Identity is **substance-anchored**
/// (`hash8` over `document ‖ normalize_substance(excerpt)`, location excluded —
/// research Decision 2): a pure move keeps the `id` (and its disposition), a material
/// change mints a new `id` (drift-forcing).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ClauseUnit {
    /// `clu-<doc>-<substance-slug>-<strength>-<hash8>` — grammar-valid per
    /// [`parse_id`] with the new `clu` prefix.
    pub id: String,
    /// Manifest document key this clause was drawn from.
    pub document: String,
    /// Detected/authored normative strength.
    pub strength: Strength,
    /// Proposed testability class.
    pub testability: Testability,
    /// Full lowercase-hex SHA-256 of `normalize_substance(excerpt)` — the distinct
    /// fingerprint field the clarification requires; drift reads it (Decision 3).
    pub fingerprint: String,
    /// Non-empty; one entry per place the same normalized substance appears. Sorted
    /// by `(anchor, ordinal)`. Multiple locations = the same obligation stated in
    /// several places (they merge into one unit).
    pub locations: Vec<ClauseLocation>,
    /// Optional structural note (e.g. `{ "inCodeFence": true }`).
    #[serde(default)]
    pub context: Option<Value>,
}

/// The closed normative-strength taxonomy (data-model.md §2, research Decision 4).
/// `must`/`should`/`may` are RFC-2119 keyword families; `algorithm`/`io-contract`/
/// `descriptive` are authored labels not derivable from a single keyword.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Strength {
    Must,
    Should,
    May,
    Algorithm,
    IoContract,
    Descriptive,
}

/// The closed testability taxonomy (data-model.md §2, research Decision 5).
/// `ambiguous` means the strength/meaning could not be confidently determined; it
/// requires a per-clause classification before `certify` passes (no document-scope
/// cover — Decision 7).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Testability {
    DirectlyTestable,
    IndirectlyTestable,
    Informative,
    Ambiguous,
    NotApplicable,
}

/// One provenance location of a [`ClauseUnit`] (data-model.md §2). `ordinal` and
/// `heading`/`anchor` are provenance/order only — never identity inputs (Decision 2).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ClauseLocation {
    /// Human-readable heading path (e.g. `Lifecycle scripts > onCreateCommand`).
    pub heading: String,
    /// GitHub-style slug of the owning heading — the excerpt-present-at-anchor key.
    pub anchor: String,
    /// 1-based position of this excerpt within its heading (provenance/order only).
    pub ordinal: u32,
    /// Verbatim source substring — the human-readable field. MUST be present in the
    /// pinned document under `anchor` (V15 / `ExcerptNotFoundAtAnchor` otherwise).
    pub excerpt: String,
}

/// A hand-authored clause-classification record (`clc-`) —
/// `conformance/registry/clause-classifications/<doc>.json` (data-model.md §3).
/// Closed model; exactly one of `clause` / `document` present (the XOR invariant is
/// enforced structurally by V13, not by serde). Joins to the inventory by ID.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ClauseClassification {
    /// Per-clause: `clc-` + the exact tail of the referenced `clu-` id (structural
    /// mirror; V13 if mismatched). Document-scope: `clc-doc-<document key>`.
    pub id: String,
    /// The `clu-` clause id this classifies (per-clause records); MUST exist in the
    /// committed inventory (V11 when stale). Mutually exclusive with `document`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub clause: Option<String>,
    /// The manifest document key this dispositions wholesale (document-scope default);
    /// MUST be an `authoring`-scope document (V13 otherwise). Mutually exclusive with
    /// `clause`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub document: Option<String>,
    /// The disposition under the consumer-only scope (reuses 020's [`Disposition`];
    /// `non-testable` is prose "informative").
    pub disposition: Disposition,
    /// Non-empty and every id an existing behavior iff `behavior-mapped`; absent/empty
    /// otherwise (V13). Several clauses MAY map to one behavior (FR-010).
    #[serde(default)]
    pub behaviors: Vec<String>,
    /// REQUIRED non-empty for `non-testable` / `not-applicable`; optional for
    /// `behavior-mapped` (V13).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rationale: Option<String>,
    /// Free-form notes (e.g. supersession of a retired prose source unit).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
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
        assert_eq!(
            parse_id("cst-base-forwardports-type-3fa9c214").unwrap(),
            RecordType::Constraint
        );
        assert_eq!(
            parse_id("cls-base-forwardports-type-3fa9c214").unwrap(),
            RecordType::Classification
        );
        assert_eq!(
            parse_id("clu-reference-oncreatecommand-run-once-must-a1b2c3d4").unwrap(),
            RecordType::ClauseUnit
        );
        assert_eq!(
            parse_id("clc-reference-oncreatecommand-run-once-must-a1b2c3d4").unwrap(),
            RecordType::ClauseClassification
        );
        assert_eq!(
            parse_id("clc-doc-features").unwrap(),
            RecordType::ClauseClassification
        );
    }

    #[test]
    fn parse_id_accepts_constraint_and_classification_prefixes() {
        // The two schema-constraint-inventory prefixes parse to their record types
        // and round-trip through prefix()/from_prefix()
        // (020-schema-constraint-inventory).
        assert_eq!(RecordType::Constraint.prefix(), "cst");
        assert_eq!(RecordType::Classification.prefix(), "cls");
        assert_eq!(RecordType::from_prefix("cst"), Some(RecordType::Constraint));
        assert_eq!(
            RecordType::from_prefix("cls"),
            Some(RecordType::Classification)
        );
        // Realistic multi-segment ids (slug + kind + hash8 tail) parse cleanly.
        assert_eq!(
            parse_id("cst-feature-options-additional-properties-0a1b2c3d").unwrap(),
            RecordType::Constraint
        );
        assert_eq!(
            parse_id("cls-feature-options-additional-properties-0a1b2c3d").unwrap(),
            RecordType::Classification
        );
        // Malformed cst/cls ids are still rejected by the shared format check.
        assert!(matches!(parse_id("cst-"), Err(IdError::Format { .. })));
        assert!(matches!(parse_id("cls-Base"), Err(IdError::Format { .. })));
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
            RecordType::Constraint,
            RecordType::Classification,
            RecordType::ClauseUnit,
            RecordType::ClauseClassification,
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

    // ---- Schema constraint inventory (020) --------------------------------

    #[test]
    fn constraint_kind_spellings() {
        round_trip(
            ConstraintKind::PropertyExistence,
            json!("property-existence"),
        );
        round_trip(ConstraintKind::Required, json!("required"));
        round_trip(ConstraintKind::Type, json!("type"));
        round_trip(ConstraintKind::Enum, json!("enum"));
        round_trip(ConstraintKind::Const, json!("const"));
        round_trip(ConstraintKind::Default, json!("default"));
        round_trip(ConstraintKind::UnionAlternative, json!("union-alternative"));
        round_trip(ConstraintKind::AllOf, json!("all-of"));
        round_trip(ConstraintKind::Conditional, json!("conditional"));
        round_trip(
            ConstraintKind::AdditionalProperties,
            json!("additional-properties"),
        );
        round_trip(ConstraintKind::ArrayShape, json!("array-shape"));
        round_trip(ConstraintKind::ValueShape, json!("value-shape"));
        round_trip(ConstraintKind::Reference, json!("reference"));
        round_trip(ConstraintKind::Annotation, json!("annotation"));
        round_trip(ConstraintKind::UnmodeledKeyword, json!("unmodeled-keyword"));
    }

    #[test]
    fn disposition_spellings() {
        round_trip(Disposition::BehaviorMapped, json!("behavior-mapped"));
        round_trip(Disposition::NonTestable, json!("non-testable"));
        round_trip(Disposition::NotApplicable, json!("not-applicable"));
    }

    #[test]
    fn schemas_manifest_round_trips() {
        let manifest = SchemasManifest {
            schema_version: 1,
            revision: "rev-schema-113500f4".into(),
            documents: vec![
                ManifestDocument {
                    key: "base".into(),
                    file: "devContainer.base.schema.json".into(),
                    upstream_url: "https://example/base.json".into(),
                    sha256: "a0883c04".into(),
                },
                ManifestDocument {
                    key: "feature".into(),
                    file: "devContainerFeature.schema.json".into(),
                    upstream_url: "https://example/feature.json".into(),
                    sha256: "671fcd80".into(),
                },
            ],
        };
        let value = serde_json::to_value(&manifest).unwrap();
        // camelCase field name on the wire.
        assert_eq!(
            value["documents"][0]["upstreamUrl"],
            json!("https://example/base.json")
        );
        assert_eq!(value["schemaVersion"], json!(1));
        let back: SchemasManifest = serde_json::from_value(value).unwrap();
        assert_eq!(back, manifest);
    }

    #[test]
    fn constraint_inventory_round_trips_with_branch_context() {
        let inventory = ConstraintInventory {
            schema_version: 1,
            revision: "rev-schema-113500f4".into(),
            units: vec![
                ConstraintUnit {
                    id: "cst-base-forwardports-type-3fa9c214".into(),
                    document: "base".into(),
                    pointer: "/definitions/devContainerCommon/properties/forwardPorts".into(),
                    kind: ConstraintKind::Type,
                    substance: json!({ "type": "array" }),
                    context: None,
                },
                ConstraintUnit {
                    id: "cst-base-container-union-alternative-0a1b2c3d".into(),
                    document: "base".into(),
                    pointer: "/oneOf/2".into(),
                    kind: ConstraintKind::UnionAlternative,
                    substance: json!({ "$ref": "#/definitions/composeContainer" }),
                    context: Some(UnitContext::Branch(BranchContext {
                        branch: "oneOf".into(),
                        index: 2,
                    })),
                },
            ],
        };
        let value = serde_json::to_value(&inventory).unwrap();
        // Top-level unit serializes context as an explicit null.
        assert_eq!(value["units"][0]["context"], json!(null));
        assert_eq!(
            value["units"][1]["context"],
            json!({ "branch": "oneOf", "index": 2 })
        );
        let back: ConstraintInventory = serde_json::from_value(value).unwrap();
        assert_eq!(back, inventory);
    }

    #[test]
    fn unit_context_condition_round_trips_and_discriminates() {
        // Condition shape.
        let condition = UnitContext::Condition(ConditionContext {
            condition: "/definitions/x/if".into(),
        });
        round_trip(condition, json!({ "condition": "/definitions/x/if" }));
        // Untagged discrimination: a branch object never parses as a condition.
        let branch: UnitContext =
            serde_json::from_value(json!({ "branch": "anyOf", "index": 0 })).unwrap();
        assert!(matches!(branch, UnitContext::Branch(_)));
        // deny_unknown_fields on the inner structs: an extra key fails BOTH variants.
        assert!(
            serde_json::from_value::<UnitContext>(json!({ "branch": "oneOf", "index": 1, "x": 2 }))
                .is_err()
        );
    }

    #[test]
    fn classification_round_trips_behavior_mapped() {
        let cls = Classification {
            id: "cls-base-forwardports-type-3fa9c214".into(),
            constraint: "cst-base-forwardports-type-3fa9c214".into(),
            disposition: Disposition::BehaviorMapped,
            behaviors: vec!["bhv-readconfig-wrong-type-forwardports-rejected".into()],
            rationale: None,
            notes: Some("Supersedes retired src-schema-forwardports-type.".into()),
        };
        let value = serde_json::to_value(&cls).unwrap();
        assert_eq!(value["disposition"], json!("behavior-mapped"));
        // rationale None is skipped; notes Some is present.
        assert!(value.get("rationale").is_none());
        assert_eq!(
            value["notes"],
            json!("Supersedes retired src-schema-forwardports-type.")
        );
        let back: Classification = serde_json::from_value(value).unwrap();
        assert_eq!(back, cls);
    }

    #[test]
    fn classification_round_trips_not_applicable_with_rationale() {
        let cls = Classification {
            id: "cls-feature-options-additional-properties-0a1b2c3d".into(),
            constraint: "cst-feature-options-additional-properties-0a1b2c3d".into(),
            disposition: Disposition::NotApplicable,
            behaviors: vec![],
            rationale: Some("Feature-authoring surface, out of consumer scope.".into()),
            notes: None,
        };
        let value = serde_json::to_value(&cls).unwrap();
        assert_eq!(value["disposition"], json!("not-applicable"));
        assert_eq!(value["behaviors"], json!([]));
        let back: Classification = serde_json::from_value(value).unwrap();
        assert_eq!(back, cls);
    }

    #[test]
    fn inventory_records_reject_unknown_fields() {
        // Manifest.
        assert!(
            serde_json::from_str::<SchemasManifest>(
                r#"{ "schemaVersion": 1, "revision": "rev-schema-x", "documents": [], "oops": 1 }"#
            )
            .is_err()
        );
        // Constraint unit.
        assert!(
            serde_json::from_str::<ConstraintUnit>(
                r#"{ "id": "cst-x-y-type-00000000", "document": "base", "pointer": "/a",
                 "kind": "type", "substance": {}, "context": null, "typo": true }"#
            )
            .is_err()
        );
        // Classification.
        assert!(
            serde_json::from_str::<Classification>(
                r#"{ "id": "cls-x", "constraint": "cst-x", "disposition": "non-testable",
                 "rationale": "r", "extra": 1 }"#
            )
            .is_err()
        );
        // Unknown disposition (e.g. the scaffold sentinel) is a hard schema failure.
        assert!(
            serde_json::from_str::<Classification>(
                r#"{ "id": "cls-x", "constraint": "cst-x", "disposition": "UNREVIEWED" }"#
            )
            .is_err()
        );
    }

    // ---- Normative clause inventory (021) ---------------------------------

    #[test]
    fn strength_spellings() {
        round_trip(Strength::Must, json!("must"));
        round_trip(Strength::Should, json!("should"));
        round_trip(Strength::May, json!("may"));
        round_trip(Strength::Algorithm, json!("algorithm"));
        round_trip(Strength::IoContract, json!("io-contract"));
        round_trip(Strength::Descriptive, json!("descriptive"));
    }

    #[test]
    fn testability_spellings() {
        round_trip(Testability::DirectlyTestable, json!("directly-testable"));
        round_trip(
            Testability::IndirectlyTestable,
            json!("indirectly-testable"),
        );
        round_trip(Testability::Informative, json!("informative"));
        round_trip(Testability::Ambiguous, json!("ambiguous"));
        round_trip(Testability::NotApplicable, json!("not-applicable"));
    }

    #[test]
    fn document_scope_spellings() {
        round_trip(DocumentScope::Consumer, json!("consumer"));
        round_trip(DocumentScope::Authoring, json!("authoring"));
    }

    #[test]
    fn spec_manifest_round_trips() {
        let manifest = SpecManifest {
            schema_version: 1,
            revision: "rev-spec-113500f4".into(),
            documents: vec![
                SpecDocument {
                    key: "reference".into(),
                    file: "devcontainer-reference.md".into(),
                    upstream_url: "https://example/reference.md".into(),
                    sha256: "daef12b6".into(),
                    scope: DocumentScope::Consumer,
                },
                SpecDocument {
                    key: "features".into(),
                    file: "devcontainer-features.md".into(),
                    upstream_url: "https://example/features.md".into(),
                    sha256: "abcd1234".into(),
                    scope: DocumentScope::Authoring,
                },
            ],
        };
        let value = serde_json::to_value(&manifest).unwrap();
        assert_eq!(value["documents"][0]["scope"], json!("consumer"));
        assert_eq!(value["documents"][1]["scope"], json!("authoring"));
        assert_eq!(
            value["documents"][0]["upstreamUrl"],
            json!("https://example/reference.md")
        );
        let back: SpecManifest = serde_json::from_value(value).unwrap();
        assert_eq!(back, manifest);
    }

    #[test]
    fn clause_inventory_round_trips() {
        let inventory = ClauseInventory {
            schema_version: 1,
            revision: "rev-spec-113500f4".into(),
            units: vec![ClauseUnit {
                id: "clu-reference-oncreatecommand-run-once-must-a1b2c3d4".into(),
                document: "reference".into(),
                strength: Strength::Must,
                testability: Testability::DirectlyTestable,
                fingerprint: "9f2a".into(),
                locations: vec![ClauseLocation {
                    heading: "Lifecycle scripts > onCreateCommand".into(),
                    anchor: "oncreatecommand".into(),
                    ordinal: 1,
                    excerpt: "`onCreateCommand` ... MUST be run only once.".into(),
                }],
                context: None,
            }],
        };
        let value = serde_json::to_value(&inventory).unwrap();
        assert_eq!(value["units"][0]["strength"], json!("must"));
        assert_eq!(value["units"][0]["context"], json!(null));
        assert_eq!(value["units"][0]["locations"][0]["ordinal"], json!(1));
        let back: ClauseInventory = serde_json::from_value(value).unwrap();
        assert_eq!(back, inventory);
    }

    #[test]
    fn clause_classification_per_clause_and_document_scope_round_trip() {
        let per_clause = ClauseClassification {
            id: "clc-reference-oncreatecommand-run-once-must-a1b2c3d4".into(),
            clause: Some("clu-reference-oncreatecommand-run-once-must-a1b2c3d4".into()),
            document: None,
            disposition: Disposition::BehaviorMapped,
            behaviors: vec!["bhv-up-lifecycle-oncreate-once".into()],
            rationale: None,
            notes: Some("Supersedes retired prose source link.".into()),
        };
        let value = serde_json::to_value(&per_clause).unwrap();
        assert_eq!(value["disposition"], json!("behavior-mapped"));
        assert!(
            value.get("document").is_none(),
            "document skipped when None"
        );
        assert!(value.get("rationale").is_none());
        let back: ClauseClassification = serde_json::from_value(value).unwrap();
        assert_eq!(back, per_clause);

        let doc_scope = ClauseClassification {
            id: "clc-doc-features".into(),
            clause: None,
            document: Some("features".into()),
            disposition: Disposition::NotApplicable,
            behaviors: vec![],
            rationale: Some("Feature-authoring document; consumer-only scope.".into()),
            notes: None,
        };
        let value = serde_json::to_value(&doc_scope).unwrap();
        assert!(value.get("clause").is_none(), "clause skipped when None");
        assert_eq!(value["document"], json!("features"));
        let back: ClauseClassification = serde_json::from_value(value).unwrap();
        assert_eq!(back, doc_scope);
    }

    #[test]
    fn clause_records_reject_unknown_fields_and_sentinel() {
        assert!(
            serde_json::from_str::<SpecManifest>(
                r#"{ "schemaVersion": 1, "revision": "rev-spec-x", "documents": [], "oops": 1 }"#
            )
            .is_err()
        );
        assert!(
            serde_json::from_str::<ClauseUnit>(
                r#"{ "id": "clu-x-y-must-00000000", "document": "reference", "strength": "must",
                 "testability": "directly-testable", "fingerprint": "ab", "locations": [],
                 "context": null, "typo": true }"#
            )
            .is_err()
        );
        // The scaffold sentinel disposition is a hard schema failure at load.
        assert!(
            serde_json::from_str::<ClauseClassification>(
                r#"{ "id": "clc-x", "clause": "clu-x", "disposition": "UNREVIEWED" }"#
            )
            .is_err()
        );
    }
}
