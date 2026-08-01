//! The hand-authored baseline-unit → destination mapping —
//! `conformance/migration/mapping.json` (data-model.md §2).
//!
//! Equal counts do not prove conservation: two sets of the same size can still have
//! lost an item and gained another (research D7). This table is the *proof*. Every
//! baseline unit appears exactly once, with an explicit disposition and, for
//! `migrated`/`deduplicated`, the concrete case ids that now carry it.
//!
//! **Ownership**: hand-authored. `baseline generate` never writes this file;
//! `migration scaffold` only emits skeletons to stdout carrying `"UNREVIEWED"`
//! sentinels the loader rejects — mirroring `inventory scaffold` / `clause scaffold`.
//!
//! Resolution, bidirectional orphan detection (V21), and one-to-one fixture
//! correspondence (V22) land with User Story 2 (T030–T032); this module defines the
//! record shapes the loader and those checks share.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

/// What happened to a baseline unit in the migration (data-model.md §2).
///
/// Distinct from [`crate::model::Disposition`], which classifies an *inventory unit*
/// (schema constraint / prose clause) against the consumer scope. These two never mix:
/// this one answers "where did this pre-migration test outcome go?".
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Disposition {
    /// Expressed as one or more declarative cases that assert the same thing.
    Migrated,
    /// Absorbed by a case that already covers the identical behavior; `rationale`
    /// names the absorbing case and why they are the same behavior.
    Deduplicated,
    /// Cannot yet be expressed as data; covered by a `res-` record, which blocks
    /// deletion of the carrier. Representation debt, never a coverage gap (FR-054).
    Residual,
    /// Deliberately dropped; `rationale` states why the loss is intentional and
    /// acceptable. Reported explicitly, never implied by a total.
    Retired,
}

impl Disposition {
    /// Whether this disposition requires a non-empty `caseIds` (data-model.md §2).
    pub fn requires_cases(self) -> bool {
        matches!(self, Disposition::Migrated | Disposition::Deduplicated)
    }

    /// Whether this disposition requires a `rationale` (data-model.md §2).
    pub fn requires_rationale(self) -> bool {
        matches!(self, Disposition::Deduplicated | Disposition::Retired)
    }

    /// The wire spelling, for diagnostics that name the offending category.
    pub fn as_str(self) -> &'static str {
        match self {
            Disposition::Migrated => "migrated",
            Disposition::Deduplicated => "deduplicated",
            Disposition::Residual => "residual",
            Disposition::Retired => "retired",
        }
    }
}

/// One `{ from, to }` fixture correspondence. Strictly one-to-one (FR-012): a `from`
/// appearing twice, or a `to` fed by two `from`s, is a silent merge/split/drop and is
/// **V22**.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FixtureMapping {
    /// Repo-relative pre-migration fixture directory (or `inline:<fn>` for a
    /// code-authored fixture), matching a `BaselineUnit.fixtures` entry.
    pub from: String,
    /// Repo-relative post-migration fixture directory under `conformance/fixtures/`.
    pub to: String,
}

/// One baseline unit's destination (data-model.md §2).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MigrationMapping {
    /// A `BaselineUnit.id`. Must resolve against the committed baseline, else **V21**.
    pub unit: String,
    /// What happened to the unit.
    pub disposition: Disposition,
    /// Cases that now carry the unit. Required and non-empty for
    /// `migrated`/`deduplicated`; empty for `residual`/`retired`. Each id must resolve
    /// in `cases.json`, else **V21** in the reverse direction.
    #[serde(default)]
    pub case_ids: Vec<String>,
    /// The covering `res-` record. Required iff `disposition: residual`; must resolve
    /// in `residuals.json`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub residual_id: Option<String>,
    /// Required for `deduplicated` (which case absorbs it and why they are the same
    /// behavior) and for `retired` (why the loss is intentional and acceptable).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rationale: Option<String>,
    /// One-to-one fixture correspondences for this unit's fixtures.
    #[serde(default)]
    pub fixture_mapping: Vec<FixtureMapping>,
}

/// What happened to a pre-migration **characterized exception** (a `wvr-` waiver or an
/// `ext-` deacon-extension record) — FR-024/FR-028/FR-051.
///
/// Exceptions are not baseline units: they are the *tolerances* the pre-migration system
/// carried. Every one must survive, mapped to **exactly one** post-migration mechanism —
/// mechanisms are never merged, because merging two tolerances silently widens both.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ExceptionDisposition {
    /// Carried forward by exactly one named mechanism under `conformance/registry/`.
    Preserved,
    /// The post-migration system has no counterpart concept. Requires a `rationale`;
    /// reported explicitly so the loss is a decision, never an omission (FR-028).
    NoCounterpart,
    /// The exception was deliberately retired AFTER the migration, because review
    /// found it tolerated no divergence — it recorded an agreement between the two
    /// CLIs, or duplicated a characterization another record already carries. Requires
    /// a `rationale` and names no mechanism.
    ///
    /// Distinct from [`NoCounterpart`](Self::NoCounterpart), which says the
    /// post-migration system never had a place to put it. This says it had one and the
    /// record was withdrawn on purpose. `preservedDirection`/`preservedScope` stay
    /// populated: what it tolerated pre-migration is still true history, and keeping it
    /// is what makes the retirement auditable rather than an erasure.
    Retired,
}

impl ExceptionDisposition {
    /// The wire spelling, for diagnostics that name the offending category.
    pub fn as_str(self) -> &'static str {
        match self {
            ExceptionDisposition::Preserved => "preserved",
            ExceptionDisposition::NoCounterpart => "no-counterpart",
            ExceptionDisposition::Retired => "retired",
        }
    }

    /// Whether the exception must still resolve to a record in the current registry.
    ///
    /// Only `preserved` does. The other two describe an exception that is deliberately
    /// ABSENT from it — `no-counterpart` never entered, `retired` was withdrawn — so
    /// demanding it resolve is unsatisfiable and made both dispositions unusable.
    pub fn requires_a_live_record(self) -> bool {
        matches!(self, ExceptionDisposition::Preserved)
    }
}

/// One pre-migration exception's correspondence to its post-migration mechanism
/// (FR-024–FR-028, FR-051).
///
/// `preservedDirection` and `preservedScope` record what the exception tolerated
/// **before** the migration. They are compared against the mechanism's current form:
/// a mechanism that now tolerates a *broader* difference than the recorded
/// pre-migration form fails validation (FR-027) — that is the only way a migration can
/// quietly widen a tolerance.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ExceptionMapping {
    /// The pre-migration exception identity (`wvr-…` or `ext-…`).
    pub exception: String,
    /// What happened to it. Defaults to `preserved`.
    #[serde(default = "default_preserved")]
    pub disposition: ExceptionDisposition,
    /// The post-migration mechanism ids. **Exactly one** when `preserved`; empty when
    /// `no-counterpart`. Two entries is a merge and is rejected.
    #[serde(default)]
    pub mechanisms: Vec<String>,
    /// The direction the exception tolerated pre-migration — a `wvr-` `expect.kind`
    /// (`both-reject` / `both-accept` / `reference-stricter` / `deacon-stricter` /
    /// `field-divergence`), or `none` for a non-directional extension record.
    pub preserved_direction: String,
    /// The scope the exception tolerated pre-migration, rendered canonically (e.g.
    /// `corpus_case:errors/malformed-json`), or `record:<id>` for an extension.
    pub preserved_scope: String,
    /// Why this correspondence is the right one — required for every entry.
    pub rationale: String,
}

fn default_preserved() -> ExceptionDisposition {
    ExceptionDisposition::Preserved
}

/// The `conformance/migration/mapping.json` envelope: the unit → destination records
/// plus the characterized-exception correspondences. Hand-authored; no generator writes
/// it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MappingFile {
    #[serde(default)]
    pub records: Vec<MigrationMapping>,
    /// Characterized-exception correspondences (FR-024, FR-051).
    #[serde(default)]
    pub exceptions: Vec<ExceptionMapping>,
}

// ---------------------------------------------------------------------------
// Resolution + orphan detection (T030, T031, T045)
// ---------------------------------------------------------------------------

/// One mapping-integrity problem, carrying the violation class it belongs to so
/// `validate.rs` can lift it verbatim into a [`crate::validate::Violation`].
///
/// The class is decided HERE, next to the rule, so the enforcement and the
/// human-readable class statement in `conformance/RULES.md` cannot drift apart.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MappingProblem {
    /// `V21` (mapping integrity) or `V22` (fixture correspondence).
    pub code: &'static str,
    /// The offending record — a baseline unit id, a case id, a fixture path, or an
    /// exception id.
    pub record: String,
    /// A precise diagnosis naming what is wrong and what to do.
    pub message: String,
}

impl MappingProblem {
    fn new(code: &'static str, record: impl Into<String>, message: impl Into<String>) -> Self {
        MappingProblem {
            code,
            record: record.into(),
            message: message.into(),
        }
    }
}

/// What a case must supply to be a valid migration destination: at least one behavior
/// and at least one observable channel (FR-008/FR-009, T033). Decoupled from
/// [`crate::model::TestCase`] so the mapping checks stay pure and unit-testable.
///
/// The last four fields are the **variant axes** (FR-015, T051): what distinguishes two
/// cases that legitimately share one behavior.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CaseFacts {
    /// The case id.
    pub id: String,
    /// Linked behavior ids.
    pub behaviors: Vec<String>,
    /// Observable channel ids the case declares (declarative `expected[]` or legacy
    /// `outcomes[]`).
    pub channels: Vec<String>,
    /// Fixture ids the case's operations materialize.
    pub fixtures: Vec<String>,
    /// Whether the case is declarative (a migration *destination*). Legacy pointer
    /// cases are pre-migration carriers and are exempt from the orphan-case direction.
    pub declarative: bool,
    /// Variant axis — the case's declared context conditions.
    pub context: Vec<String>,
    /// Variant axis — the oracle the case is evaluated against
    /// (`spec-expectation` / `snapshot` / `live-differential` / `invariant-metamorphic`),
    /// or the legacy binary name for a pointer case.
    pub oracle: String,
    /// Variant axis — the case's input shape: a canonical rendering of the operations
    /// it performs (subcommand + argv + fixtures), or the legacy binary for a pointer
    /// case. Two cases with the same input shape run the same thing.
    pub input_shape: String,
}

// ---------------------------------------------------------------------------
// Variant representation (T051, FR-015)
// ---------------------------------------------------------------------------

/// The axes on which two cases sharing one behavior may legitimately differ
/// (FR-015). A pair that differs on NONE of them is not a variant — it is a
/// duplicate, and duplicates inflate the case count without adding evidence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum VariantAxis {
    /// The declared context conditions differ (e.g. one is compose-scoped).
    Context,
    /// The oracle type differs (e.g. spec-expectation versus live-differential).
    OracleType,
    /// The observed channel set differs.
    Channel,
    /// The input shape differs — different argv, different fixtures, or a different
    /// operation sequence. This is the axis the merged-mode corpus variants use: the
    /// SAME workspace and behavior, distinguished only by
    /// `--include-merged-configuration`.
    InputShape,
}

impl VariantAxis {
    /// The wire spelling, for report rendering and diagnostics.
    pub fn as_str(self) -> &'static str {
        match self {
            VariantAxis::Context => "context",
            VariantAxis::OracleType => "oracle-type",
            VariantAxis::Channel => "channel",
            VariantAxis::InputShape => "input-shape",
        }
    }
}

/// One behavior's variant group: the distinct cases that share it, and — for each pair
/// beyond the first — the axes that distinguish them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VariantGroup {
    /// The shared behavior id.
    pub behavior: String,
    /// The case ids sharing it, ID-sorted.
    pub cases: Vec<String>,
    /// Every axis on which at least one pair in the group differs, sorted.
    pub distinguished_by: Vec<VariantAxis>,
}

impl VariantGroup {
    /// Whether this group represents genuine variants (more than one case, each
    /// distinguishable). A single-case group is not a variant group.
    pub fn is_variant_group(&self) -> bool {
        self.cases.len() > 1
    }
}

/// The axes on which two cases differ. Empty means they are indistinguishable — the
/// same evidence authored twice.
pub fn distinguishing_axes(a: &CaseFacts, b: &CaseFacts) -> Vec<VariantAxis> {
    let mut axes = Vec::new();
    if sorted(&a.context) != sorted(&b.context) {
        axes.push(VariantAxis::Context);
    }
    if a.oracle != b.oracle {
        axes.push(VariantAxis::OracleType);
    }
    if sorted(&a.channels) != sorted(&b.channels) {
        axes.push(VariantAxis::Channel);
    }
    if a.input_shape != b.input_shape || sorted(&a.fixtures) != sorted(&b.fixtures) {
        axes.push(VariantAxis::InputShape);
    }
    axes
}

fn sorted(values: &[String]) -> Vec<&str> {
    let mut out: Vec<&str> = values.iter().map(String::as_str).collect();
    out.sort_unstable();
    out
}

/// Group `cases` by the behaviors they share, recording what distinguishes the members
/// of each group (T051). Behaviors with a single case yield a one-member group; the
/// caller decides whether that is interesting.
pub fn variant_groups(cases: &[CaseFacts]) -> Vec<VariantGroup> {
    let mut by_behavior: BTreeMap<&str, Vec<&CaseFacts>> = BTreeMap::new();
    for case in cases {
        for behavior in &case.behaviors {
            by_behavior.entry(behavior.as_str()).or_default().push(case);
        }
    }

    by_behavior
        .into_iter()
        .map(|(behavior, members)| {
            let mut axes: BTreeSet<VariantAxis> = BTreeSet::new();
            for (i, a) in members.iter().enumerate() {
                for b in members.iter().skip(i + 1) {
                    axes.extend(distinguishing_axes(a, b));
                }
            }
            let mut case_ids: Vec<String> = members.iter().map(|c| c.id.clone()).collect();
            case_ids.sort();
            VariantGroup {
                behavior: behavior.to_string(),
                cases: case_ids,
                distinguished_by: axes.into_iter().collect(),
            }
        })
        .collect()
}

/// **V21 — variant well-formedness** (T051, FR-015): two cases that share a behavior
/// must differ on at least one variant axis.
///
/// This is what keeps the denominator honest from the *other* side. US3's headline rule
/// is "a variant must not become a new behavior"; the converse matters just as much —
/// two cases claiming one behavior with identical context, oracle, channels and input
/// shape are the same evidence counted twice, which inflates the case count while
/// proving nothing new.
pub fn check_variants(cases: &[CaseFacts]) -> Vec<MappingProblem> {
    let mut out = Vec::new();
    let mut by_behavior: BTreeMap<&str, Vec<&CaseFacts>> = BTreeMap::new();
    for case in cases {
        for behavior in &case.behaviors {
            by_behavior.entry(behavior.as_str()).or_default().push(case);
        }
    }

    for (behavior, members) in by_behavior {
        for (i, a) in members.iter().enumerate() {
            for b in members.iter().skip(i + 1) {
                if distinguishing_axes(a, b).is_empty() {
                    out.push(MappingProblem::new(
                        "V21",
                        &b.id,
                        format!(
                            "is indistinguishable from case {:?} on every variant axis \
                             (context, oracle type, channel, input shape) while sharing \
                             behavior {behavior:?} — two cases that run the same thing \
                             against the same oracle are one piece of evidence counted \
                             twice, not a variant (FR-015)",
                            a.id
                        ),
                    ));
                }
            }
        }
    }
    out
}

/// Check unit → destination mapping integrity in **both** directions (**V21**):
///
/// - every baseline unit appears exactly once (a missing one is an orphan test);
/// - every mapped `unit` resolves against the baseline;
/// - every `caseIds` entry resolves in `cases.json`;
/// - the disposition arity rules of data-model §2 hold;
/// - every case a `migrated`/`deduplicated` unit names resolves to ≥1 behavior AND ≥1
///   observable channel, with dangling behavior/channel ids rejected (T033).
///
/// `baseline_units` is `(unit id, program)`; `known_behaviors` / `known_channels` are
/// the registry's declared ids.
#[allow(clippy::too_many_arguments)]
pub fn check_mapping(
    baseline_units: &[String],
    mapping: &[MigrationMapping],
    cases: &[CaseFacts],
    residual_ids: &BTreeSet<String>,
    known_behaviors: &BTreeSet<String>,
    known_channels: &BTreeSet<String>,
) -> Vec<MappingProblem> {
    let mut out = Vec::new();

    // A registry with no committed baseline has nothing to map against. The ABSENCE
    // itself is reported by `validate::check_mapping`, which sees the whole registry and
    // can tell "no baseline and nothing referencing one" (legitimate) from "no baseline
    // but records that reference baseline units" (incoherent). This function sees only
    // the units, so it cannot make that call and must not guess.
    //
    // An empty mapping against a PRESENT baseline IS reported here (every unit is then an
    // orphan) — that is the honest reading.
    if baseline_units.is_empty() {
        return out;
    }

    let unit_ids: BTreeSet<&str> = baseline_units.iter().map(String::as_str).collect();
    let case_index: BTreeMap<&str, &CaseFacts> = cases.iter().map(|c| (c.id.as_str(), c)).collect();

    let mut mapped_units: BTreeSet<&str> = BTreeSet::new();
    let mut reached_cases: BTreeSet<&str> = BTreeSet::new();

    for record in mapping {
        // Forward direction: the mapped unit must exist in the baseline.
        if !unit_ids.contains(record.unit.as_str()) {
            out.push(MappingProblem::new(
                "V21",
                &record.unit,
                "mapping entry names a baseline unit that does not exist — the mapping \
                 must resolve against the committed baseline",
            ));
        }
        mapped_units.insert(record.unit.as_str());

        // Disposition arity (data-model §2).
        let disposition = record.disposition;
        if disposition.requires_cases() && record.case_ids.is_empty() {
            out.push(MappingProblem::new(
                "V21",
                &record.unit,
                format!(
                    "disposition `{}` requires a non-empty `caseIds`",
                    disposition.as_str()
                ),
            ));
        }
        if !disposition.requires_cases() && !record.case_ids.is_empty() {
            out.push(MappingProblem::new(
                "V21",
                &record.unit,
                format!(
                    "disposition `{}` must not name any case (it produces none)",
                    disposition.as_str()
                ),
            ));
        }
        if disposition.requires_rationale()
            && record
                .rationale
                .as_ref()
                .is_none_or(|r| r.trim().is_empty())
        {
            out.push(MappingProblem::new(
                "V21",
                &record.unit,
                format!(
                    "disposition `{}` requires a `rationale` — an unexplained \
                     deduplication or retirement is an unreviewable loss",
                    disposition.as_str()
                ),
            ));
        }
        match (disposition, record.residual_id.as_deref()) {
            (Disposition::Residual, None) => out.push(MappingProblem::new(
                "V21",
                &record.unit,
                "disposition `residual` requires a `residualId`",
            )),
            (Disposition::Residual, Some(id)) if !residual_ids.contains(id) => {
                out.push(MappingProblem::new(
                    "V21",
                    &record.unit,
                    format!("`residualId` {id:?} does not resolve in residuals.json"),
                ));
            }
            (d, Some(id)) if d != Disposition::Residual => out.push(MappingProblem::new(
                "V21",
                &record.unit,
                format!(
                    "`residualId` {id:?} is set on a `{}` mapping; only `residual` \
                     carries one",
                    d.as_str()
                ),
            )),
            _ => {}
        }

        // Destination cases must exist and be usable as evidence (T033).
        for case_id in &record.case_ids {
            let Some(case) = case_index.get(case_id.as_str()) else {
                out.push(MappingProblem::new(
                    "V21",
                    &record.unit,
                    format!("names case {case_id:?}, which does not exist in cases.json"),
                ));
                continue;
            };
            reached_cases.insert(case.id.as_str());

            if case.behaviors.is_empty() {
                out.push(MappingProblem::new(
                    "V21",
                    &case.id,
                    "migration destination case resolves to no behavior — a case that \
                     proves nothing about a behavior conserves no coverage",
                ));
            }
            for behavior in &case.behaviors {
                if !known_behaviors.contains(behavior.as_str()) {
                    out.push(MappingProblem::new(
                        "V21",
                        &case.id,
                        format!("migration destination case names unknown behavior {behavior:?}"),
                    ));
                }
            }
            if case.channels.is_empty() {
                out.push(MappingProblem::new(
                    "V21",
                    &case.id,
                    "migration destination case declares no observable channel — there \
                     is nothing for the runner to compare",
                ));
            }
            for channel in &case.channels {
                if !known_channels.contains(channel.as_str()) {
                    out.push(MappingProblem::new(
                        "V21",
                        &case.id,
                        format!("migration destination case names unknown channel {channel:?}"),
                    ));
                }
            }
        }
    }

    // Forward orphans: a baseline unit no mapping entry reaches.
    for unit in &unit_ids {
        if !mapped_units.contains(unit) {
            out.push(MappingProblem::new(
                "V21",
                *unit,
                "baseline unit has no mapping entry — every unit needs exactly one \
                 destination (migrated | deduplicated | residual | retired)",
            ));
        }
    }

    // The REVERSE direction — "every declarative case is reached by some mapping entry"
    // — is **retired** (024 US3), for the reason V25 was retired. It was true exactly
    // while the declarative case set WAS the migration's output; the moment a case is
    // authored for coverage the migration never had, the rule reports a correct record as
    // an orphan and the only way to satisfy it would be to invent a baseline unit for a
    // case that migrated from nothing. Conservation needs the FORWARD direction (every
    // baseline unit has exactly one destination, checked above); that is what proves
    // nothing was lost. Nothing is proved by the reverse, and a permanent gate there would
    // forbid the coverage growth the migration exists to make room for.
    let _ = &reached_cases;

    out
}

/// Verify **V22** — fixture correspondence is strictly one-to-one, no fixture is
/// silently dropped, and no migrated fixture is left unreferenced.
///
/// - a `from` appearing twice with different `to` values is a **split**;
/// - a `to` fed by two different `from` values is a **merge**;
/// - a `from` that is not one of its unit's baseline fixtures is a **mis-declaration**;
/// - a baseline fixture of a `migrated`/`deduplicated` unit with no `fixtureMapping`
///   entry is a **drop**;
/// - a `to` that no case's `operations[].fixtures` references is an **unreferenced
///   orphan** — the fixture was moved but nothing runs against it.
///
/// `unit_fixtures` maps a baseline unit id to the fixtures it consumed. `to` values are
/// repo-relative directories under `conformance/fixtures/`; their last path segment is
/// the fixture id a case references.
pub fn check_fixture_mappings(
    mapping: &[MigrationMapping],
    unit_fixtures: &BTreeMap<String, Vec<String>>,
    cases: &[CaseFacts],
) -> Vec<MappingProblem> {
    let mut out = Vec::new();

    // `from` → the set of `to` values it feeds, and `to` → the set of `from` values.
    let mut from_to: BTreeMap<&str, BTreeSet<&str>> = BTreeMap::new();
    let mut to_from: BTreeMap<&str, BTreeSet<&str>> = BTreeMap::new();

    for record in mapping {
        let declared: BTreeSet<&str> = unit_fixtures
            .get(&record.unit)
            .map(|v| v.iter().map(String::as_str).collect())
            .unwrap_or_default();
        let mut covered: BTreeSet<&str> = BTreeSet::new();

        for fm in &record.fixture_mapping {
            from_to
                .entry(fm.from.as_str())
                .or_default()
                .insert(fm.to.as_str());
            to_from
                .entry(fm.to.as_str())
                .or_default()
                .insert(fm.from.as_str());
            covered.insert(fm.from.as_str());

            if !declared.contains(fm.from.as_str()) {
                out.push(MappingProblem::new(
                    "V22",
                    &record.unit,
                    format!(
                        "fixtureMapping `from` {:?} is not one of the unit's baseline \
                         fixtures {:?}",
                        fm.from,
                        declared.iter().collect::<Vec<_>>()
                    ),
                ));
            }
        }

        // A migrated/deduplicated unit must account for every fixture it consumed.
        if record.disposition.requires_cases() {
            for fixture in &declared {
                if !covered.contains(fixture) {
                    out.push(MappingProblem::new(
                        "V22",
                        &record.unit,
                        format!(
                            "baseline fixture {fixture:?} has no fixtureMapping entry — a \
                             migrated unit's fixtures may not be silently dropped"
                        ),
                    ));
                }
            }
        }
    }

    for (from, tos) in &from_to {
        if tos.len() > 1 {
            out.push(MappingProblem::new(
                "V22",
                *from,
                format!(
                    "fixture is split across {} destinations {:?}; the correspondence \
                     must be one-to-one",
                    tos.len(),
                    tos.iter().collect::<Vec<_>>()
                ),
            ));
        }
    }
    for (to, froms) in &to_from {
        if froms.len() > 1 {
            out.push(MappingProblem::new(
                "V22",
                *to,
                format!(
                    "fixture destination is fed by {} sources {:?}; the correspondence \
                     must be one-to-one (a silent merge loses one input)",
                    froms.len(),
                    froms.iter().collect::<Vec<_>>()
                ),
            ));
        }
    }

    // Unreferenced migrated fixtures: a destination no case actually runs against.
    let referenced: BTreeSet<&str> = cases
        .iter()
        .flat_map(|c| c.fixtures.iter().map(String::as_str))
        .collect();
    for to in to_from.keys() {
        let id = fixture_id_of(to);
        if !referenced.contains(id) {
            out.push(MappingProblem::new(
                "V22",
                *to,
                format!(
                    "migrated fixture {id:?} is referenced by no case — a fixture that \
                     nothing runs against conserves nothing"
                ),
            ));
        }
    }

    out
}

/// The fixture id a case references, given a repo-relative destination directory: the
/// last path segment (`conformance/fixtures/fx-a` → `fx-a`).
fn fixture_id_of(to: &str) -> &str {
    to.rsplit('/').next().unwrap_or(to)
}

/// How broad a tolerated difference is, as a total order. A migrated exception may
/// preserve or narrow its breadth, never widen it (FR-027).
///
/// The order encodes what each expectation lets through:
/// agreement expectations (`both-*`) tolerate no directional difference at all;
/// a one-directional expectation tolerates a disagreement in exactly one direction;
/// `field-divergence` tolerates a value difference regardless of direction.
pub fn direction_breadth(direction: &str) -> u8 {
    match direction {
        "none" => 0,
        "both-reject" | "both-accept" => 1,
        "reference-stricter" | "deacon-stricter" => 2,
        "field-divergence" => 3,
        // An unrecognized direction is treated as maximally broad so an unreviewed
        // spelling can never pass as narrow (fail-closed).
        _ => u8::MAX,
    }
}

/// How specific a tolerated scope is, as a total order (lower = narrower). A scope that
/// became less specific tolerates strictly more, so widening is detected structurally
/// rather than by string comparison.
pub fn scope_breadth(scope: &str) -> u8 {
    match scope.split(':').next().unwrap_or(scope) {
        // `corpus_case:<corpus>/<case>` — one case of one corpus.
        "corpus_case" => 0,
        // `record:<id>` — one named registry record.
        "record" => 0,
        // `case:<id>` — one declarative case.
        "case" => 0,
        // `corpus:<id>` — every case of a corpus.
        "corpus" => 1,
        // `behavior:<id>` — every case of a behavior.
        "behavior" => 2,
        // `global` and anything unrecognized: maximally broad (fail-closed).
        _ => u8::MAX,
    }
}

/// The current (post-migration) form of a mechanism, as `validate.rs` renders it from
/// the registry: its id, the direction it tolerates, and its canonical scope.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MechanismForm {
    /// The mechanism's registry id (`wvr-…` / `ext-…`).
    pub id: String,
    /// Its current tolerated direction, in the same vocabulary as
    /// [`ExceptionMapping::preserved_direction`].
    pub direction: String,
    /// Its current canonical scope, in the same vocabulary as
    /// [`ExceptionMapping::preserved_scope`].
    pub scope: String,
}

/// Check **V21** exception correspondences (FR-024, FR-027, FR-028, FR-051):
///
/// - every known exception has exactly one mapping entry;
/// - a `preserved` exception names **exactly one** resolvable mechanism (zero is an
///   orphan, more than one is a merge — mechanisms are never merged);
/// - a `no-counterpart` exception names none and carries a rationale;
/// - the mechanism's current direction and scope are **no broader** than the recorded
///   pre-migration form.
pub fn check_exception_mappings(
    exceptions: &[ExceptionMapping],
    known_exceptions: &BTreeSet<String>,
    mechanisms: &BTreeMap<String, MechanismForm>,
) -> Vec<MappingProblem> {
    let mut out = Vec::new();
    if known_exceptions.is_empty() {
        return out;
    }

    let mut seen: BTreeSet<&str> = BTreeSet::new();
    for entry in exceptions {
        if !seen.insert(entry.exception.as_str()) {
            out.push(MappingProblem::new(
                "V21",
                &entry.exception,
                "characterized exception has more than one mapping entry",
            ));
        }
        // Only a `preserved` entry claims its exception is still carried, so only it can
        // be contradicted by the exception's absence. `no-counterpart` and `retired` both
        // ASSERT that absence — running this check over them made every such entry
        // self-refuting, which is why `no-counterpart` had zero instances despite being
        // the model's documented way to record a deliberate loss.
        if entry.disposition.requires_a_live_record()
            && !known_exceptions.contains(entry.exception.as_str())
        {
            out.push(MappingProblem::new(
                "V21",
                &entry.exception,
                "mapping entry names a characterized exception that does not exist in \
                 the registry",
            ));
        }
        if entry.rationale.trim().is_empty() {
            out.push(MappingProblem::new(
                "V21",
                &entry.exception,
                "exception mapping requires a `rationale`",
            ));
        }

        match entry.disposition {
            d @ (ExceptionDisposition::NoCounterpart | ExceptionDisposition::Retired) => {
                if !entry.mechanisms.is_empty() {
                    out.push(MappingProblem::new(
                        "V21",
                        &entry.exception,
                        format!("a `{}` exception must name no mechanism", d.as_str()),
                    ));
                }
                continue;
            }
            ExceptionDisposition::Preserved => {}
        }

        if entry.mechanisms.len() != 1 {
            out.push(MappingProblem::new(
                "V21",
                &entry.exception,
                format!(
                    "a preserved exception must map to EXACTLY ONE mechanism, found {} \
                     — zero orphans the exception, more than one merges tolerances and \
                     silently widens both (FR-024)",
                    entry.mechanisms.len()
                ),
            ));
            continue;
        }

        let mechanism_id = &entry.mechanisms[0];
        let Some(form) = mechanisms.get(mechanism_id.as_str()) else {
            out.push(MappingProblem::new(
                "V21",
                &entry.exception,
                format!("names mechanism {mechanism_id:?}, which does not resolve in the registry"),
            ));
            continue;
        };

        if direction_breadth(&form.direction) > direction_breadth(&entry.preserved_direction) {
            out.push(MappingProblem::new(
                "V21",
                &entry.exception,
                format!(
                    "mechanism {:?} now tolerates direction `{}`, which is BROADER than \
                     the recorded pre-migration direction `{}` — a migration may narrow \
                     a tolerance, never widen it (FR-027)",
                    form.id, form.direction, entry.preserved_direction
                ),
            ));
        }
        if scope_breadth(&form.scope) > scope_breadth(&entry.preserved_scope) {
            out.push(MappingProblem::new(
                "V21",
                &entry.exception,
                format!(
                    "mechanism {:?} now applies at scope `{}`, which is BROADER than the \
                     recorded pre-migration scope `{}` — a migration may narrow a \
                     tolerance, never widen it (FR-027)",
                    form.id, form.scope, entry.preserved_scope
                ),
            ));
        }
    }

    for exception in known_exceptions {
        if !seen.contains(exception.as_str()) {
            out.push(MappingProblem::new(
                "V21",
                exception,
                "characterized exception has no mapping entry — every pre-migration \
                 exception must be explicitly dispositioned, never dropped (FR-028)",
            ));
        }
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disposition_wire_spellings_are_kebab_case() {
        for (d, wire) in [
            (Disposition::Migrated, "\"migrated\""),
            (Disposition::Deduplicated, "\"deduplicated\""),
            (Disposition::Residual, "\"residual\""),
            (Disposition::Retired, "\"retired\""),
        ] {
            let json = serde_json::to_string(&d).expect("disposition serializes");
            assert_eq!(json, wire);
            assert_eq!(format!("\"{}\"", d.as_str()), wire);
        }
    }

    #[test]
    fn arity_rules_match_the_data_model() {
        assert!(Disposition::Migrated.requires_cases());
        assert!(Disposition::Deduplicated.requires_cases());
        assert!(!Disposition::Residual.requires_cases());
        assert!(!Disposition::Retired.requires_cases());

        assert!(!Disposition::Migrated.requires_rationale());
        assert!(Disposition::Deduplicated.requires_rationale());
        assert!(Disposition::Retired.requires_rationale());
    }

    #[test]
    fn mapping_round_trips_and_rejects_unknown_fields() {
        let raw = r#"{
          "records": [
            {
              "unit": "parity_corpus_tier1::node-ts",
              "disposition": "migrated",
              "caseIds": ["case-tier1-node-ts"],
              "fixtureMapping": [
                { "from": "fixtures/parity-corpus/node-ts", "to": "conformance/fixtures/fx-tier1-node-ts" }
              ]
            }
          ]
        }"#;
        let file: MappingFile = serde_json::from_str(raw).expect("well-formed mapping loads");
        assert_eq!(file.records.len(), 1);
        assert_eq!(file.records[0].disposition, Disposition::Migrated);
        assert_eq!(file.records[0].fixture_mapping.len(), 1);
        assert!(file.records[0].residual_id.is_none());

        let bad = r#"{ "records": [ { "unit": "u", "disposition": "migrated", "surprise": 1 } ] }"#;
        assert!(
            serde_json::from_str::<MappingFile>(bad).is_err(),
            "unknown fields must be rejected, never silently ignored"
        );

        let bad_disposition = r#"{ "records": [ { "unit": "u", "disposition": "moved" } ] }"#;
        assert!(
            serde_json::from_str::<MappingFile>(bad_disposition).is_err(),
            "the disposition enum is closed"
        );
    }
}
