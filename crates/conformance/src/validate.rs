//! Structural validation engine (violation classes V1–V10 + SCHEMA), FR-019.
//!
//! [`run`] evaluates every violation class over a loaded [`Registry`] and returns
//! ALL violations found in a single pass (never first-failure), sorted by code then
//! record ID (contracts/cli.md). [`validate_path`] is the load-then-validate
//! convenience the CLI and acceptance tests share: a schema-invalid registry folds
//! its located [`SchemaError`]s into `SCHEMA`-class violations; a genuinely
//! unreadable registry root is the only outcome surfaced as an `Err` (CLI exit 2).
//!
//! The violation classes (data-model.md):
//!
//! - **V1** dangling references (record IDs, dimension values, orphan behaviors) and
//!   missing executable test binaries (research Decision 9);
//! - **V2** duplicate stable IDs anywhere in the registry, ID-format violations, and
//!   prefix↔type mismatches (FR-004);
//! - **V3** a test case linked to no behavior;
//! - **V4** a source unit with empty `behaviors` and no `outOfScope`;
//! - **V5** an in-active-profile behavior with no case, no waiver, AND no gap;
//! - **V6** a waiver whose `expires` is earlier than today;
//! - **V7** a source revision whose `pin` disagrees with its `verifiedAgainst` file;
//! - **V8** disposition contradictions (rules R1–R8, research Decision 5), incl.
//!   extension↔decision consistency;
//! - **V9** an expected outcome referencing an undeclared observable channel;
//! - **V10** a test case whose context has an empty intersection with a linked
//!   behavior's applicability.
//!
//! Pure sync file IO only (V1 executable existence, V7 pin file); no Unix-only APIs
//! and no path-string parsing, so the crate compiles and validates identically on
//! the Windows `dev-fast` lane.

use std::collections::{HashMap, HashSet};
use std::path::Path;

use serde::Serialize;

use crate::load::{LoadError, Registry, SchemaError};
use crate::model::{
    BehaviorUnit, CertificationProfile, Condition, Decision, RecordType, ReferenceStatus,
    SpecStatus, parse_id,
};

/// A single structural violation. `code` is the stable class (`"V1"`..`"V10"` or
/// `"SCHEMA"`); `record` names the offending registry record (or, for SCHEMA, the
/// file); `message` is a precise, human-readable diagnosis (constitution IV).
///
/// `Serialize` produces exactly the contracts/cli.md JSON shape
/// (`{ "code", "record", "message" }`) — field names are already the wire names.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Violation {
    pub code: String,
    pub record: String,
    pub message: String,
}

impl Violation {
    fn new(code: &str, record: impl Into<String>, message: impl Into<String>) -> Violation {
        Violation {
            code: code.to_string(),
            record: record.into(),
            message: message.into(),
        }
    }
}

/// Sort rank for a violation code: `SCHEMA` first, then `V1`..`V10` numerically
/// (so `V10` sorts after `V2`, not lexicographically before it). Unknown codes sort
/// last, deterministically.
fn code_rank(code: &str) -> u32 {
    match code {
        "SCHEMA" => 0,
        other => other
            .strip_prefix('V')
            .and_then(|n| n.parse::<u32>().ok())
            .unwrap_or(u32::MAX),
    }
}

/// Load the registry at `root` and validate it, folding schema-load failures into
/// `SCHEMA`-class violations.
///
/// - `Ok(violations)` — the registry loaded (empty = valid) OR the load hit a
///   schema error (returned as `SCHEMA` violations, one per bad file). Both map to
///   CLI exit 0 (empty) / 1 (non-empty).
/// - `Err(LoadError::Root)` — the registry root is unreadable (CLI exit 2).
///
/// `repo_root` anchors repo-relative checks (V7 `verifiedAgainst` files, V1
/// executable-test files); it is the workspace root, NOT the registry dir, so a
/// fixture registry still resolves the real `fixtures/parity-corpus/oracle.json` and
/// `crates/*/tests/` tree.
pub fn validate_path(
    root: &Path,
    today: &str,
    repo_root: &Path,
) -> Result<Vec<Violation>, LoadError> {
    match Registry::load(root) {
        Ok(registry) => Ok(run(&registry, today, repo_root)),
        Err(LoadError::Schema(errors)) => Ok(schema_violations(&errors)),
        Err(root_err @ LoadError::Root { .. }) => Err(root_err),
    }
}

/// Convert the loader's located schema errors into `SCHEMA`-class violations. The
/// `record` field carries the file path (with `line:column` when the loader
/// captured one) so a schema failure names its location like every other violation.
pub fn schema_violations(errors: &[SchemaError]) -> Vec<Violation> {
    let mut out: Vec<Violation> = errors
        .iter()
        .map(|e| {
            let record = match &e.location {
                Some(loc) => format!("{}:{}", e.file.display(), loc),
                None => e.file.display().to_string(),
            };
            Violation::new("SCHEMA", record, e.message.clone())
        })
        .collect();
    sort_violations(&mut out);
    out
}

/// Run every violation-class check (V1–V10) over a loaded registry and return ALL
/// violations, sorted by code then record ID (FR-019, contracts/cli.md). Assumes the
/// registry already parsed cleanly (schema failures are handled by [`validate_path`]).
pub fn run(registry: &Registry, today: &str, repo_root: &Path) -> Vec<Violation> {
    let mut checker = Checker::new(registry, repo_root);
    // V2 before V1 so a duplicate/misprefixed id is reported on its own terms even
    // when the same record also participates in a dangling reference.
    checker.check_ids_and_duplicates(); // V2
    checker.check_references(); // V1 (record refs, dimension values, orphan behaviors)
    checker.check_executables(); // V1 (executable test binaries)
    checker.check_case_behaviors(); // V3
    checker.check_source_scope(); // V4
    checker.check_coverage(); // V5
    checker.check_waiver_expiry(today); // V6
    checker.check_pins(); // V7
    checker.check_contradictions(); // V8
    checker.check_channels(); // V9
    checker.check_context_intersection(); // V10

    let mut out = checker.violations;
    sort_violations(&mut out);
    out
}

/// Deterministic order: code rank, then record ID, then message.
fn sort_violations(violations: &mut [Violation]) {
    violations.sort_by(|a, b| {
        code_rank(&a.code)
            .cmp(&code_rank(&b.code))
            .then_with(|| a.record.cmp(&b.record))
            .then_with(|| a.message.cmp(&b.message))
    });
}

/// Cross-reference indices + accumulated violations for one validation run.
struct Checker<'a> {
    reg: &'a Registry,
    repo_root: &'a Path,
    violations: Vec<Violation>,

    /// Behavior id → the behavior (for R-rule and V10 lookups).
    behaviors: HashMap<&'a str, &'a BehaviorUnit>,
    /// Declared revision ids.
    revision_ids: HashSet<&'a str>,
    /// Dimension id → its declared value set.
    dim_values: HashMap<&'a str, HashSet<&'a str>>,
    /// Declared channel ids.
    channel_ids: HashSet<&'a str>,
    /// The single active profile, if any (coverage/in-profile checks no-op without
    /// one — a registry with zero behaviors validates cleanly).
    active_profile: Option<&'a CertificationProfile>,

    /// Behavior ids covered by ≥1 test case / waiver / gap (structural coverage).
    covered_by_case: HashSet<&'a str>,
    covered_by_waiver: HashSet<&'a str>,
    covered_by_gap: HashSet<&'a str>,
    /// Behavior ids referenced by ≥1 source unit (every behavior needs one — V1).
    sourced: HashSet<&'a str>,
}

impl<'a> Checker<'a> {
    fn new(reg: &'a Registry, repo_root: &'a Path) -> Checker<'a> {
        let behaviors: HashMap<&str, &BehaviorUnit> =
            reg.behaviors.iter().map(|b| (b.id.as_str(), b)).collect();
        let revision_ids: HashSet<&str> = reg.revisions.iter().map(|r| r.id.as_str()).collect();
        let dim_values: HashMap<&str, HashSet<&str>> = reg
            .dimensions
            .iter()
            .map(|d| (d.id.as_str(), d.values.iter().map(String::as_str).collect()))
            .collect();
        let channel_ids: HashSet<&str> = reg.channels.iter().map(|c| c.id.as_str()).collect();
        let active_profile = reg.profiles.iter().find(|p| p.active);

        let mut covered_by_case = HashSet::new();
        for case in &reg.cases {
            for b in &case.behaviors {
                covered_by_case.insert(b.as_str());
            }
        }
        let mut covered_by_waiver = HashSet::new();
        for waiver in &reg.waivers {
            for b in &waiver.behaviors {
                covered_by_waiver.insert(b.as_str());
            }
        }
        let mut covered_by_gap = HashSet::new();
        for gap in &reg.gaps {
            for b in &gap.behaviors {
                covered_by_gap.insert(b.as_str());
            }
        }
        let mut sourced = HashSet::new();
        for src in &reg.sources {
            for b in &src.behaviors {
                sourced.insert(b.as_str());
            }
        }

        Checker {
            reg,
            repo_root,
            violations: Vec::new(),
            behaviors,
            revision_ids,
            dim_values,
            channel_ids,
            active_profile,
            covered_by_case,
            covered_by_waiver,
            covered_by_gap,
            sourced,
        }
    }

    fn push(&mut self, code: &str, record: impl Into<String>, message: impl Into<String>) {
        self.violations.push(Violation::new(code, record, message));
    }

    // -- V2: duplicate ids, ID format, prefix↔type agreement -----------------

    fn check_ids_and_duplicates(&mut self) {
        let mut seen: HashSet<String> = HashSet::new();
        for (id, ty) in self.all_ids() {
            match parse_id(id) {
                Ok(parsed) if parsed == ty => {}
                Ok(parsed) => self.push(
                    "V2",
                    id,
                    format!(
                        "id prefix denotes a {} record but it appears as a {} record \
                         (prefix↔type mismatch, FR-004)",
                        record_type_name(parsed),
                        record_type_name(ty),
                    ),
                ),
                Err(err) => self.push("V2", id, err.to_string()),
            }
            if !seen.insert(id.to_string()) {
                self.push(
                    "V2",
                    id,
                    "duplicate stable id (ids must be unique across the whole registry)",
                );
            }
        }
    }

    /// Every `(id, record type)` pair in the registry, in collection order.
    fn all_ids(&self) -> Vec<(&'a str, RecordType)> {
        let mut out: Vec<(&str, RecordType)> = Vec::new();
        let r = self.reg;
        out.extend(
            r.revisions
                .iter()
                .map(|x| (x.id.as_str(), RecordType::Revision)),
        );
        out.extend(
            r.sources
                .iter()
                .map(|x| (x.id.as_str(), RecordType::Source)),
        );
        out.extend(
            r.dimensions
                .iter()
                .map(|x| (x.id.as_str(), RecordType::Dimension)),
        );
        out.extend(
            r.channels
                .iter()
                .map(|x| (x.id.as_str(), RecordType::Channel)),
        );
        out.extend(
            r.profiles
                .iter()
                .map(|x| (x.id.as_str(), RecordType::Profile)),
        );
        out.extend(
            r.behaviors
                .iter()
                .map(|x| (x.id.as_str(), RecordType::Behavior)),
        );
        out.extend(r.cases.iter().map(|x| (x.id.as_str(), RecordType::Case)));
        out.extend(r.gaps.iter().map(|x| (x.id.as_str(), RecordType::Gap)));
        out.extend(
            r.waivers
                .iter()
                .map(|x| (x.id.as_str(), RecordType::Waiver)),
        );
        out.extend(
            r.extensions
                .iter()
                .map(|x| (x.id.as_str(), RecordType::Extension)),
        );
        out
    }

    // -- V1: dangling references, dimension values, orphan behaviors ---------

    fn check_references(&mut self) {
        // Source units: revision provenance + behavior links.
        for src in &self.reg.sources {
            if !self.revision_ids.contains(src.revision.as_str()) {
                self.push(
                    "V1",
                    &src.id,
                    format!("references undefined source revision {:?}", src.revision),
                );
            }
            self.check_behavior_refs(&src.id, &src.behaviors);
        }

        // Active/declared profiles: context assigns declared dimension→value pairs.
        for profile in &self.reg.profiles {
            for (dim, val) in &profile.context {
                self.check_dim_value(&profile.id, dim, val);
            }
        }

        // Behaviors: applicability dimensions + values.
        for bhv in &self.reg.behaviors {
            self.check_conditions(&bhv.id, &bhv.applicability);
        }

        // Cases, gaps, waivers, extensions: behavior links (+ case context dims).
        for case in &self.reg.cases {
            self.check_behavior_refs(&case.id, &case.behaviors);
            self.check_conditions(&case.id, &case.context);
        }
        for gap in &self.reg.gaps {
            self.check_behavior_refs(&gap.id, &gap.behaviors);
        }
        for waiver in &self.reg.waivers {
            self.check_behavior_refs(&waiver.id, &waiver.behaviors);
        }
        for ext in &self.reg.extensions {
            self.check_behavior_refs(&ext.id, &ext.behaviors);
        }

        // Orphan behaviors: every behavior must be referenced by ≥1 source unit
        // (data-model BehaviorUnit.sources is the inverse of SourceUnit.behaviors).
        for bhv in &self.reg.behaviors {
            if !self.sourced.contains(bhv.id.as_str()) {
                self.push(
                    "V1",
                    &bhv.id,
                    "behavior is referenced by no source unit (every behavior needs ≥1 source)",
                );
            }
        }
    }

    fn check_behavior_refs(&mut self, from: &str, refs: &[String]) {
        for target in refs {
            if !self.behaviors.contains_key(target.as_str()) {
                self.push(
                    "V1",
                    from,
                    format!("references undefined behavior {target:?}"),
                );
            }
        }
    }

    fn check_conditions(&mut self, from: &str, conditions: &[Condition]) {
        for cond in conditions {
            for val in &cond.values {
                self.check_dim_value(from, &cond.dimension, val);
            }
            if cond.values.is_empty() {
                // A dimension pinned to no values still references the dimension id.
                self.check_dim_exists(from, &cond.dimension);
            }
        }
    }

    fn check_dim_exists(&mut self, from: &str, dim: &str) {
        if !self.dim_values.contains_key(dim) {
            self.push(
                "V1",
                from,
                format!("references undeclared context dimension {dim:?}"),
            );
        }
    }

    fn check_dim_value(&mut self, from: &str, dim: &str, val: &str) {
        match self.dim_values.get(dim) {
            None => self.push(
                "V1",
                from,
                format!("references undeclared context dimension {dim:?}"),
            ),
            Some(values) if !values.contains(val) => self.push(
                "V1",
                from,
                format!("references undeclared value {val:?} of dimension {dim:?}"),
            ),
            Some(_) => {}
        }
    }

    // -- V1: executable test binaries exist ----------------------------------

    fn check_executables(&mut self) {
        for case in &self.reg.cases {
            let binary = &case.executable.binary;
            if !binary_exists(self.repo_root, binary) {
                self.push(
                    "V1",
                    &case.id,
                    format!(
                        "executable references test binary {binary:?}, but no \
                         crates/*/tests/{binary}.rs exists"
                    ),
                );
            }
        }
    }

    // -- V3: test case with no behaviors -------------------------------------

    fn check_case_behaviors(&mut self) {
        for case in &self.reg.cases {
            if case.behaviors.is_empty() {
                self.push(
                    "V3",
                    &case.id,
                    "test case is linked to no behavior (orphan case)",
                );
            }
        }
    }

    // -- V4: source unit with empty behaviors and no outOfScope --------------

    fn check_source_scope(&mut self) {
        for src in &self.reg.sources {
            if src.behaviors.is_empty() && src.out_of_scope.is_none() {
                self.push(
                    "V4",
                    &src.id,
                    "source unit has no behaviors and no `outOfScope` classification",
                );
            }
        }
    }

    // -- V5: in-active-profile behavior with no case, waiver, or gap ---------

    fn check_coverage(&mut self) {
        let Some(profile) = self.active_profile else {
            return; // No active profile → nothing is "in profile" to require coverage.
        };
        for bhv in &self.reg.behaviors {
            if !applies_in_profile(bhv, profile) {
                continue;
            }
            let id = bhv.id.as_str();
            let covered = self.covered_by_case.contains(id)
                || self.covered_by_waiver.contains(id)
                || self.covered_by_gap.contains(id);
            if !covered {
                self.push(
                    "V5",
                    id,
                    format!(
                        "behavior is applicable in the active profile {:?} but has no \
                         test case, waiver, or gap",
                        profile.id
                    ),
                );
            }
        }
    }

    // -- V6: waiver expiry (ISO lexicographic; boundary expires==today passes) --

    fn check_waiver_expiry(&mut self, today: &str) {
        for waiver in &self.reg.waivers {
            // `added`/`expires` MUST be canonical `YYYY-MM-DD`. The expiry test is a
            // lexicographic string compare, which is only sound for zero-padded ISO
            // dates: a non-canonical date (e.g. `"2026-7-1"`) sorts wrong and would
            // let an actually-expired waiver slip past validation — and, through the
            // `certify` gate, past the release. Reject it explicitly rather than
            // silently mis-evaluating expiry (constitution IV: fail fast).
            if !is_canonical_date(&waiver.added) {
                self.push(
                    "V6",
                    &waiver.id,
                    format!(
                        "waiver `added` is not a canonical YYYY-MM-DD date: {:?}",
                        waiver.added
                    ),
                );
            }
            if !is_canonical_date(&waiver.expires) {
                self.push(
                    "V6",
                    &waiver.id,
                    format!(
                        "waiver `expires` is not a canonical YYYY-MM-DD date: {:?}",
                        waiver.expires
                    ),
                );
            } else if waiver.expires.as_str() < today {
                self.push(
                    "V6",
                    &waiver.id,
                    format!(
                        "waiver expired: expires {:?} is earlier than today {today:?}",
                        waiver.expires
                    ),
                );
            }
        }
    }

    // -- V7: source revision pin vs verifiedAgainst repo file ----------------

    fn check_pins(&mut self) {
        for rev in &self.reg.revisions {
            let Some(rel) = &rev.verified_against else {
                continue;
            };
            let path = self.repo_root.join(rel);
            match read_verified_pin(&path) {
                Ok(version) if version == rev.pin => {}
                Ok(version) => self.push(
                    "V7",
                    &rev.id,
                    format!(
                        "pin {:?} disagrees with {rel:?} version {version:?}",
                        rev.pin
                    ),
                ),
                Err(cause) => self.push(
                    "V7",
                    &rev.id,
                    format!("cannot verify pin against {rel:?}: {cause}"),
                ),
            }
        }
    }

    // -- V8: disposition contradictions (rules R1–R8) ------------------------

    fn check_contradictions(&mut self) {
        for bhv in &self.reg.behaviors {
            self.check_behavior_rules(bhv);
        }
        // Extension↔decision consistency (part of V8): every behavior linked from a
        // DeaconExtension must carry `decision: deacon-extension`.
        for ext in &self.reg.extensions {
            for target in &ext.behaviors {
                if let Some(bhv) = self.behaviors.get(target.as_str()) {
                    if bhv.decision != Decision::DeaconExtension {
                        self.push(
                            "V8",
                            &ext.id,
                            format!(
                                "extension links behavior {:?}, which has decision {} \
                                 (extensions require decision `deacon-extension`)",
                                bhv.id,
                                decision_name(bhv.decision),
                            ),
                        );
                    }
                }
            }
        }
    }

    fn check_behavior_rules(&mut self, bhv: &BehaviorUnit) {
        let id = bhv.id.as_str();
        let spec = bhv.spec;
        let reference = bhv.reference;
        let decision = bhv.decision;
        let has_case = self.covered_by_case.contains(id);
        let has_waiver = self.covered_by_waiver.contains(id);
        let has_gap = self.covered_by_gap.contains(id);
        let gap_only = has_gap && !has_case && !has_waiver;
        let in_profile = self
            .active_profile
            .is_some_and(|p| applies_in_profile(bhv, p));

        // R1: `unresolved-gap` contradicts (spec conformant AND reference aligned).
        if decision == Decision::UnresolvedGap
            && spec == SpecStatus::Conformant
            && reference == ReferenceStatus::Aligned
        {
            self.push(
                "V8",
                id,
                "R1: decision `unresolved-gap` contradicts spec `conformant` + reference `aligned`",
            );
        }
        // R2: `deacon-extension` requires spec ∈ {unspecified, not-applicable}.
        if decision == Decision::DeaconExtension
            && !matches!(spec, SpecStatus::Unspecified | SpecStatus::NotApplicable)
        {
            self.push(
                "V8",
                id,
                format!(
                    "R2: decision `deacon-extension` requires spec `unspecified` or \
                     `not-applicable`, found {}",
                    spec_name(spec)
                ),
            );
        }
        // R3: `intentional-divergence` contradicts reference `aligned`.
        if decision == Decision::IntentionalDivergence && reference == ReferenceStatus::Aligned {
            self.push(
                "V8",
                id,
                "R3: decision `intentional-divergence` contradicts reference `aligned`",
            );
        }
        // R4: reference `unknown` on an in-profile behavior requires `unresolved-gap`.
        if in_profile
            && reference == ReferenceStatus::Unknown
            && decision != Decision::UnresolvedGap
        {
            self.push(
                "V8",
                id,
                format!(
                    "R4: in-profile reference `unknown` requires decision `unresolved-gap`, \
                     found {}",
                    decision_name(decision)
                ),
            );
        }
        // R5: `follow-spec` requires spec `conformant`.
        if decision == Decision::FollowSpec && spec != SpecStatus::Conformant {
            self.push(
                "V8",
                id,
                format!(
                    "R5: decision `follow-spec` requires spec `conformant`, found {}",
                    spec_name(spec)
                ),
            );
        }
        // R6: `align-with-reference` requires reference `aligned`.
        if decision == Decision::AlignWithReference && reference != ReferenceStatus::Aligned {
            self.push(
                "V8",
                id,
                format!(
                    "R6: decision `align-with-reference` requires reference `aligned`, found {}",
                    reference_name(reference)
                ),
            );
        }
        // R7: a behavior whose only structural coverage is a gap requires
        // `unresolved-gap`.
        if gap_only && decision != Decision::UnresolvedGap {
            self.push(
                "V8",
                id,
                format!(
                    "R7: gap-only coverage requires decision `unresolved-gap`, found {}",
                    decision_name(decision)
                ),
            );
        }
        // R8: an in-profile behavior with no case AND no waiver requires reference
        // `unknown` (statuses are verified claims, not aspirations).
        //
        // Deacon extensions are EXEMPT: for a `deacon-extension`, `not-applicable`
        // is the correct reference status (the reference CLI has no concept of the
        // behavior — it is a classification, not an unverified claim), so forcing
        // `unknown` would be wrong. This exemption is also belt-and-suspenders: R2
        // already constrains an extension's spec, and R7 blocks gap-only extensions,
        // so any VALID in-profile extension is already case- or waiver-backed (which
        // makes R8's antecedent false regardless of the exemption). Encoded here so
        // the rule reads consistently and never double-reports.
        if in_profile
            && !has_case
            && !has_waiver
            && reference != ReferenceStatus::Unknown
            && decision != Decision::DeaconExtension
        {
            self.push(
                "V8",
                id,
                format!(
                    "R8: in-profile behavior with no case and no waiver requires reference \
                     `unknown`, found {}",
                    reference_name(reference)
                ),
            );
        }
    }

    // -- V9: outcome referencing an undeclared observable channel ------------

    fn check_channels(&mut self) {
        for case in &self.reg.cases {
            for outcome in &case.outcomes {
                if !self.channel_ids.contains(outcome.channel.as_str()) {
                    self.push(
                        "V9",
                        &case.id,
                        format!(
                            "outcome references undeclared observable channel {:?}",
                            outcome.channel
                        ),
                    );
                }
            }
        }
    }

    // -- V10: case context vs linked behavior applicability intersection -----

    fn check_context_intersection(&mut self) {
        for case in &self.reg.cases {
            for target in &case.behaviors {
                let Some(bhv) = self.behaviors.get(target.as_str()) else {
                    continue; // dangling behavior ref is a V1 concern.
                };
                if let Some(dim) = conflicting_dimension(&case.context, &bhv.applicability) {
                    self.push(
                        "V10",
                        &case.id,
                        format!(
                            "case context and behavior {:?} applicability have an empty \
                             intersection on dimension {dim:?}",
                            bhv.id
                        ),
                    );
                }
            }
        }
    }
}

/// Whether a behavior applies in a profile's context: every applicability condition
/// must be satisfied by the profile's assignment (empty applicability = everywhere).
/// A condition on a dimension the profile does not assign is treated as unsatisfied.
pub fn applies_in_profile(behavior: &BehaviorUnit, profile: &CertificationProfile) -> bool {
    behavior.applicability.iter().all(|cond| {
        profile
            .context
            .get(&cond.dimension)
            .is_some_and(|assigned| cond.values.iter().any(|v| v == assigned))
    })
}

/// The first dimension on which two condition sets have an empty value intersection,
/// or `None` if they can be jointly satisfied. A dimension constrained by only one
/// side is unconstrained on the other (so it never conflicts); a conflict requires
/// BOTH sides to constrain the same dimension to disjoint value sets. This is the
/// applicability/context intersection evaluator shared with coverage (T007).
pub fn conflicting_dimension(a: &[Condition], b: &[Condition]) -> Option<String> {
    let a_map = conditions_to_map(a);
    let b_map = conditions_to_map(b);
    // Scan dimensions in a stable (sorted) order so that, when a case conflicts on
    // more than one dimension, the reported dimension is deterministic rather than
    // dependent on HashMap iteration order.
    let mut dims: Vec<&String> = a_map.keys().collect();
    dims.sort();
    for dim in dims {
        if let Some(b_values) = b_map.get(dim) {
            if a_map[dim].is_disjoint(b_values) {
                return Some(dim.clone());
            }
        }
    }
    None
}

/// Collapse a condition list into dimension → value-set (unioning repeated
/// dimensions), for intersection testing.
fn conditions_to_map(conditions: &[Condition]) -> HashMap<String, HashSet<String>> {
    let mut map: HashMap<String, HashSet<String>> = HashMap::new();
    for cond in conditions {
        let entry = map.entry(cond.dimension.clone()).or_default();
        for v in &cond.values {
            entry.insert(v.clone());
        }
    }
    map
}

/// Whether a nextest test binary `binary` exists as a source file
/// `crates/<crate>/tests/<binary>.rs` under `repo_root` (research Decision 9,
/// mirroring `parity-harness::registry::check_test_files`). Uses `PathBuf` joins
/// only — separator-agnostic, so the check behaves identically on Windows.
fn binary_exists(repo_root: &Path, binary: &str) -> bool {
    let crates_dir = repo_root.join("crates");
    let Ok(entries) = std::fs::read_dir(&crates_dir) else {
        return false;
    };
    let file_name = format!("{binary}.rs");
    for entry in entries.flatten() {
        let candidate = entry.path().join("tests").join(&file_name);
        if candidate.is_file() {
            return true;
        }
    }
    false
}

/// Whether `s` is a canonical ISO `YYYY-MM-DD` calendar date: it parses as a real
/// date AND round-trips to the identical (zero-padded) string. Canonicality is what
/// makes the V6 lexicographic expiry comparison sound; a parseable-but-non-canonical
/// spelling (or a non-existent date like `2026-02-30`) is rejected.
fn is_canonical_date(s: &str) -> bool {
    s.parse::<jiff::civil::Date>()
        .is_ok_and(|d| d.to_string() == s)
}

/// Read the pinned version from a `verifiedAgainst` repo file. The only such file in
/// the seed is `fixtures/parity-corpus/oracle.json` (`{ "package", "version" }`), so
/// the machine-readable pin is its top-level string `"version"` (research/T009).
fn read_verified_pin(path: &Path) -> Result<String, String> {
    let raw = std::fs::read_to_string(path).map_err(|e| format!("could not read file: {e}"))?;
    let value: serde_json::Value =
        serde_json::from_str(&raw).map_err(|e| format!("malformed JSON: {e}"))?;
    match value.get("version").and_then(|v| v.as_str()) {
        Some(version) => Ok(version.to_string()),
        None => Err("no top-level string `version` field".to_string()),
    }
}

fn record_type_name(ty: RecordType) -> &'static str {
    match ty {
        RecordType::Revision => "source-revision",
        RecordType::Source => "source-unit",
        RecordType::Dimension => "dimension",
        RecordType::Channel => "channel",
        RecordType::Profile => "profile",
        RecordType::Behavior => "behavior",
        RecordType::Case => "test-case",
        RecordType::Gap => "gap",
        RecordType::Waiver => "waiver",
        RecordType::Extension => "extension",
    }
}

fn spec_name(spec: SpecStatus) -> &'static str {
    match spec {
        SpecStatus::Conformant => "conformant",
        SpecStatus::Nonconformant => "nonconformant",
        SpecStatus::Unspecified => "unspecified",
        SpecStatus::NotApplicable => "not-applicable",
    }
}

fn reference_name(reference: ReferenceStatus) -> &'static str {
    match reference {
        ReferenceStatus::Aligned => "aligned",
        ReferenceStatus::Divergent => "divergent",
        ReferenceStatus::Unknown => "unknown",
        ReferenceStatus::NotApplicable => "not-applicable",
    }
}

fn decision_name(decision: Decision) -> &'static str {
    match decision {
        Decision::FollowSpec => "follow-spec",
        Decision::AlignWithReference => "align-with-reference",
        Decision::DeaconExtension => "deacon-extension",
        Decision::IntentionalDivergence => "intentional-divergence",
        Decision::UnresolvedGap => "unresolved-gap",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{
        CertificationProfile, Condition, Decision, ReferenceStatus, SpecStatus, TestCase,
    };
    use indexmap::IndexMap;

    fn behavior(
        id: &str,
        spec: SpecStatus,
        reference: ReferenceStatus,
        decision: Decision,
        applicability: Vec<Condition>,
    ) -> BehaviorUnit {
        BehaviorUnit {
            id: id.to_string(),
            area: "test".to_string(),
            statement: "s".to_string(),
            applicability,
            spec,
            reference,
            decision,
            notes: None,
        }
    }

    fn cond(dimension: &str, values: &[&str]) -> Condition {
        Condition {
            dimension: dimension.to_string(),
            values: values.iter().map(|s| s.to_string()).collect(),
        }
    }

    fn active_profile() -> CertificationProfile {
        let mut context = IndexMap::new();
        context.insert("dim-runtime".to_string(), "docker".to_string());
        CertificationProfile {
            id: "prof-x".to_string(),
            context,
            active: true,
        }
    }

    #[test]
    fn code_rank_orders_v10_after_v2() {
        assert!(code_rank("SCHEMA") < code_rank("V1"));
        assert!(code_rank("V2") < code_rank("V10"));
        assert!(code_rank("V9") < code_rank("V10"));
    }

    #[test]
    fn applies_in_profile_handles_empty_and_disjoint() {
        let profile = active_profile();
        let everywhere = behavior(
            "bhv-a",
            SpecStatus::Conformant,
            ReferenceStatus::Aligned,
            Decision::FollowSpec,
            vec![],
        );
        assert!(applies_in_profile(&everywhere, &profile));

        let podman = behavior(
            "bhv-b",
            SpecStatus::Conformant,
            ReferenceStatus::Aligned,
            Decision::FollowSpec,
            vec![cond("dim-runtime", &["podman"])],
        );
        assert!(!applies_in_profile(&podman, &profile));

        let docker = behavior(
            "bhv-c",
            SpecStatus::Conformant,
            ReferenceStatus::Aligned,
            Decision::FollowSpec,
            vec![cond("dim-runtime", &["docker"])],
        );
        assert!(applies_in_profile(&docker, &profile));
    }

    #[test]
    fn conflicting_dimension_only_when_both_constrain_disjointly() {
        // Disjoint on a shared dimension → conflict.
        let a = vec![cond("dim-runtime", &["docker"])];
        let b = vec![cond("dim-runtime", &["podman"])];
        assert_eq!(
            conflicting_dimension(&a, &b).as_deref(),
            Some("dim-runtime")
        );

        // One side unconstrained on the dimension → no conflict.
        let empty: Vec<Condition> = vec![];
        assert!(conflicting_dimension(&a, &empty).is_none());

        // Overlapping values → no conflict.
        let c = vec![cond("dim-runtime", &["docker", "podman"])];
        assert!(conflicting_dimension(&a, &c).is_none());

        // Different dimensions → no conflict.
        let d = vec![cond("dim-os", &["linux"])];
        assert!(conflicting_dimension(&a, &d).is_none());
    }

    #[test]
    fn v2_flags_duplicate_and_prefix_mismatch() {
        let mut reg = Registry::default();
        // A behavior whose id prefix denotes a case → prefix↔type mismatch.
        reg.behaviors.push(behavior(
            "case-misprefixed",
            SpecStatus::Conformant,
            ReferenceStatus::Aligned,
            Decision::FollowSpec,
            vec![],
        ));
        let out = run(&reg, "2026-07-19", Path::new("/nonexistent-root"));
        assert!(
            out.iter()
                .any(|v| v.code == "V2" && v.record == "case-misprefixed"),
            "expected a V2 prefix-mismatch, got {out:?}"
        );
    }

    #[test]
    fn v6_boundary_expires_equals_today_passes() {
        use crate::model::{Expect, Scope, Waiver};
        let waiver = Waiver {
            id: "wvr-x".to_string(),
            behaviors: vec![],
            scope: Scope::CorpusCase {
                corpus: "errors".to_string(),
                case: "malformed-json".to_string(),
            },
            expect: Expect::DeaconStricter { signal: None },
            rationale: "r".to_string(),
            added: "2026-01-01".to_string(),
            expires: "2026-07-19".to_string(),
            config: None,
        };
        let mut reg = Registry::default();
        reg.waivers.push(waiver);

        // expires == today → passes (no V6).
        let same = run(&reg, "2026-07-19", Path::new("/nonexistent-root"));
        assert!(!same.iter().any(|v| v.code == "V6"), "boundary must pass");

        // expires < today → V6.
        let later = run(&reg, "2026-07-20", Path::new("/nonexistent-root"));
        assert!(
            later.iter().any(|v| v.code == "V6" && v.record == "wvr-x"),
            "expired waiver must be V6, got {later:?}"
        );
    }

    #[test]
    fn r8_exempts_extensions_but_flags_plain_behaviors() {
        let profile = active_profile();

        // A plain behavior in-profile, no case/waiver, reference not `unknown`
        // → R8 fires.
        let plain = behavior(
            "bhv-plain",
            SpecStatus::Conformant,
            ReferenceStatus::Aligned,
            Decision::FollowSpec,
            vec![],
        );
        let mut reg = Registry::default();
        reg.profiles.push(profile.clone());
        reg.behaviors.push(plain);
        let out = run(&reg, "2026-07-19", Path::new("/nonexistent-root"));
        assert!(
            out.iter()
                .any(|v| v.code == "V8" && v.message.contains("R8")),
            "plain uncovered in-profile behavior must trip R8, got {out:?}"
        );

        // An extension behavior with reference `not-applicable`, same coverage
        // state → R8 exempt (no R8), even though V5 would still flag missing
        // coverage separately.
        let ext = behavior(
            "bhv-ext",
            SpecStatus::Unspecified,
            ReferenceStatus::NotApplicable,
            Decision::DeaconExtension,
            vec![],
        );
        let mut reg2 = Registry::default();
        reg2.profiles.push(profile);
        reg2.behaviors.push(ext);
        let out2 = run(&reg2, "2026-07-19", Path::new("/nonexistent-root"));
        assert!(
            !out2
                .iter()
                .any(|v| v.code == "V8" && v.message.contains("R8")),
            "extension must be exempt from R8, got {out2:?}"
        );
    }

    #[test]
    fn v9_flags_undeclared_channel() {
        let mut reg = Registry::default();
        reg.cases.push(TestCase {
            id: "case-x".to_string(),
            behaviors: vec!["bhv-a".to_string()],
            context: vec![],
            executable: crate::model::Executable {
                binary: "no_such_binary".to_string(),
                test: None,
                corpus: None,
                case: None,
            },
            outcomes: vec![crate::model::ExpectedOutcome {
                channel: "chan-ghost".to_string(),
                expectation: "x".to_string(),
            }],
        });
        let out = run(&reg, "2026-07-19", Path::new("/nonexistent-root"));
        assert!(
            out.iter().any(|v| v.code == "V9" && v.record == "case-x"),
            "undeclared channel must be V9, got {out:?}"
        );
    }

    #[test]
    fn empty_registry_has_no_violations() {
        let reg = Registry::default();
        let out = run(&reg, "2026-07-19", Path::new("/nonexistent-root"));
        assert!(out.is_empty(), "empty registry is valid, got {out:?}");
    }
}
