//! The findings queue — records, strict loader, atomic writer, signature-keyed upsert,
//! and the **D1**/**D5** violation classes
//! (025-exploratory-parity-discovery, data-model.md §§ 3–4,
//! contracts/findings-queue.md, T016–T021).
//!
//! ## Where this lives, and why it matters
//!
//! `conformance/discovery/` is a **sibling** of `conformance/registry/`, not a child.
//! [`crate::load::Registry::load`] enumerates *named* subdirectories under the registry
//! root and has no wildcard walk, so nothing here can be reached by the registry loader
//! or, transitively, by `certify`. The guarantee that an unreviewed finding can never
//! influence a release gate is therefore a property of the directory layout rather than
//! a rule someone must remember — which matters precisely because the failure mode is
//! silent: a finding quietly joining the certification denominator (research D6).
//!
//! Exactly one reference crosses the boundary, and it points **out**:
//! [`Finding::promoted_to`] → `case-<id>`. Nothing in the registry points back.
//!
//! ## Duplicate findings are unrepresentable
//!
//! A finding's `id` is *derived* from its signature ([`Signature::finding_id`]), so
//! FR-030's "two findings with the same signature are one finding" is a property of the
//! identity function rather than an invariant the merge logic has to maintain — and can
//! therefore violate. [`upsert_finding`] appends a witness to the existing record; it
//! cannot create a second one.

use indexmap::IndexMap;
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;
use std::path::{Path, PathBuf};

use crate::DiscoveryError;
use crate::load::{LoadError, SchemaError, deserialize_located, read_file};
use crate::model::{RevisionKind, SourceRevision};

use super::signature::Signature;

// ---------------------------------------------------------------------------
// T016 — Finding, Witness, FindingState, Classification (data-model.md § 3)
// ---------------------------------------------------------------------------

/// The closed classification set (FR-028). Exactly one, once a finding is triaged.
///
/// The last two are **non-promotable** (FR-035) because they describe a defect in the
/// discovery machinery, not a behavior of either implementation: resolving them changes
/// the normalizer or the generator, and promoting one would record a claim about deacon
/// or the reference that the evidence does not support.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Classification {
    /// deacon is wrong; the reference and/or the spec is right. Promotes to a behavior
    /// plus a fix.
    DeaconRegression,
    /// The reference diverges from the spec; deacon is right. Promotes to a behavior
    /// plus a waiver.
    ReferenceQuirk,
    /// The spec does not decide the question. Promotes to a behavior with
    /// `spec: unspecified`.
    SpecAmbiguity,
    /// deacon lacks the capability entirely. Promotes to a `gap-` record.
    UnsupportedBehavior,
    /// The comparison machinery manufactured or hid the difference. **Not promotable.**
    NormalizerDefect,
    /// The generated input was invalid in a way the harness cannot express.
    /// **Not promotable.**
    FixtureDefect,
}

impl Classification {
    /// The stable wire spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            Classification::DeaconRegression => "deacon-regression",
            Classification::ReferenceQuirk => "reference-quirk",
            Classification::SpecAmbiguity => "spec-ambiguity",
            Classification::UnsupportedBehavior => "unsupported-behavior",
            Classification::NormalizerDefect => "normalizer-defect",
            Classification::FixtureDefect => "fixture-defect",
        }
    }

    /// Whether a finding carrying this classification may ever reach
    /// [`FindingState::Promoted`] (FR-035).
    pub fn is_promotable(self) -> bool {
        !matches!(
            self,
            Classification::NormalizerDefect | Classification::FixtureDefect
        )
    }

    /// Every classification, in declaration order — the CLI's `--classification` value
    /// set and the report's bucket order.
    pub fn all() -> &'static [Classification] {
        &[
            Classification::DeaconRegression,
            Classification::ReferenceQuirk,
            Classification::SpecAmbiguity,
            Classification::UnsupportedBehavior,
            Classification::NormalizerDefect,
            Classification::FixtureDefect,
        ]
    }

    /// Parse a wire spelling, returning `None` on anything else — never a default.
    pub fn parse(s: &str) -> Option<Classification> {
        Classification::all()
            .iter()
            .copied()
            .find(|c| c.as_str() == s)
    }
}

/// The finding lifecycle (data-model.md § 3's state machine).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum FindingState {
    /// Admitted by a campaign, nobody has looked yet. **Counted, never implicit**
    /// (FR-029) — "not yet looked at" must never read as "nothing found".
    Untriaged,
    /// A reviewer assigned a classification.
    Triaged,
    /// A reviewer split a signature-merged finding whose witnesses had different causes.
    /// The parent becomes an inert ancestor: it keeps its witnesses as historical record,
    /// stops accepting new ones, and surrenders classification to its children — a split
    /// exists precisely because one classification could not describe them all
    /// (invariant Q10).
    Split,
    /// Carried by a real registry case. Terminal; `promotedTo` is set and must resolve.
    Promoted,
    /// Stopped reproducing. **A state, not a deletion** (FR-033): the disappearance is
    /// information — it may mean a fix landed, or it may mean the generator stopped
    /// reaching the input, and deleting the record destroys the ability to tell those
    /// apart. A later campaign that reproduces it moves it back to `triaged`, keeping
    /// its classification.
    NoLongerReproducing,
}

impl FindingState {
    /// The stable wire spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            FindingState::Untriaged => "untriaged",
            FindingState::Triaged => "triaged",
            FindingState::Split => "split",
            FindingState::Promoted => "promoted",
            FindingState::NoLongerReproducing => "no-longer-reproducing",
        }
    }

    /// Every state, in lifecycle order — the report's bucket order.
    pub fn all() -> &'static [FindingState] {
        &[
            FindingState::Untriaged,
            FindingState::Triaged,
            FindingState::Split,
            FindingState::Promoted,
            FindingState::NoLongerReproducing,
        ]
    }

    /// Whether `self → next` is a transition data-model.md § 3's state machine declares
    /// (**T069**).
    ///
    /// The permitted set is exactly the arrows in that diagram, and the three arrows it
    /// does **not** draw are each absent for a reason worth stating, because each looks
    /// plausible enough that someone will eventually add it:
    ///
    /// - **`untriaged → no-longer-reproducing`** is absent. `no-longer-reproducing`
    ///   invalidates a *judgement*, and an untriaged finding has none — there is nothing
    ///   for the disappearance to invalidate. Worse, the state requires a classification
    ///   (**D2**), so allowing the arrow would manufacture a violation out of a campaign
    ///   merely not re-observing something nobody had looked at. An untriaged finding that
    ///   stops reproducing simply stays untriaged, which is the truth: it is still "not
    ///   yet looked at".
    /// - **`no-longer-reproducing → promoted`** is absent. Promotion authors a case
    ///   asserting a difference the implementations exhibit *now*; a finding whose
    ///   difference stopped reproducing has no current evidence, so promoting it would put
    ///   a case in the registry that the next run cannot reproduce. Reviving it first (a
    ///   campaign observing it again) is exactly the evidence promotion needs.
    /// - **`untriaged → split`** is absent. A split exists because one *classification*
    ///   could not describe several witnesses; before anyone has classified anything there
    ///   is no judgement to separate.
    ///
    /// `promoted` and `split` are terminal. Re-classifying a finding *within* a state is
    /// not a transition at all and does not consult this function — see
    /// [`Finding::triage`].
    pub fn may_transition_to(self, next: FindingState) -> bool {
        matches!(
            (self, next),
            (FindingState::Untriaged, FindingState::Triaged)
                | (FindingState::Triaged, FindingState::Split)
                | (FindingState::Triaged, FindingState::Promoted)
                | (FindingState::Triaged, FindingState::NoLongerReproducing)
                | (FindingState::NoLongerReproducing, FindingState::Triaged)
        )
    }
}

/// Why a state change or a split was refused (**T069**/**T070**).
///
/// A rejected transition is an ordinary, expected outcome — a reviewer asking to promote
/// a `normalizer-defect`, or to split a finding with a single witness — so it is a typed
/// error rather than a panic, and every variant names the remedy rather than only the
/// rule.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum TransitionError {
    /// The state machine declares no arrow from `from` to `to`.
    #[error(
        "finding `{finding}` cannot move from `{from}` to `{to}`: the lifecycle declares \
         no such transition. Remedy: reach `{to}` through the declared path, or leave the \
         finding where it is — a state reached out of order asserts an event that did not \
         happen."
    )]
    NotPermitted {
        finding: String,
        from: &'static str,
        to: &'static str,
    },

    /// A state that requires a classification was requested for a finding without one.
    #[error(
        "finding `{finding}` has no classification, and state `{to}` requires exactly one \
         (FR-028). Remedy: `discovery triage {finding} --classification <c>` first."
    )]
    MissingClassification { finding: String, to: &'static str },

    /// Promotion of a `normalizer-defect` / `fixture-defect` finding (FR-035).
    #[error(
        "finding `{finding}` is classified `{classification}`, which is not promotable \
         (FR-035): it describes a defect in the discovery or comparison machinery, not a \
         behavior of either implementation. Remedy: fix the normalizer or the generator — \
         promoting it would record a claim about deacon or the reference that the evidence \
         does not support."
    )]
    NonPromotable {
        finding: String,
        classification: &'static str,
    },

    /// A split was asked of a finding with fewer than two witnesses.
    #[error(
        "finding `{finding}` carries {witnesses} witness(es); a split separates witnesses \
         whose causes differ, so there is nothing to separate. Remedy: wait for a second \
         witness, or triage the finding as it stands."
    )]
    NothingToSplit { finding: String, witnesses: usize },

    /// The named finding is not in the queue.
    #[error("no finding `{finding}` in the queue")]
    UnknownFinding { finding: String },
}

/// The two sides' concrete observed values for one witness.
///
/// **Evidence, not identity.** Concrete values never enter the signature — including
/// them would split one defect across every generated value and make deduplication do
/// nothing (research D3) — so they live here, where a reviewer can read them.
///
/// A `null` in either field means *the side produced nothing at this path*. An observed
/// JSON `null` renders identically, which is a deliberate and harmless conflation: the
/// presence distinction is carried by the signature's `valueShapeClass`
/// (`present-absent` versus `type-changed`), so nothing that matters is lost here.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ObservedValues {
    /// What deacon produced at the signature's path.
    #[serde(default)]
    pub deacon: Option<Value>,
    /// What the reference produced at the signature's path.
    #[serde(default)]
    pub reference: Option<Value>,
}

/// One observation of a finding (data-model.md § 3).
///
/// Witnesses are retained per finding (FR-032) so a merge can be reviewed and, if the
/// reviewer disagrees, reversed by a split.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Witness {
    /// `wit-<hash8>` over `campaignId ‖ candidateId`.
    pub id: String,
    /// The campaign that observed it — must resolve in `campaigns.json` (**D1**).
    pub campaign_id: String,
    /// The candidate input that produced it (`cnd-<hash8>`).
    pub candidate_id: String,
    /// The reduced fixture.
    pub minimal_input: Value,
    /// `false` when the shrink budget was exhausted (FR-022) — a partially reduced input
    /// is **never** silently presented as minimal.
    pub is_minimal: bool,
    /// The ordered catalogue step names applied during reduction.
    #[serde(default)]
    pub reduction_steps: Vec<String>,
    /// The concrete values each side produced.
    #[serde(default)]
    pub observed_values: ObservedValues,
    /// The `mop-` operators that produced the candidate (FR-009 attribution).
    #[serde(default)]
    pub mutation_operators: Vec<String>,
}

impl Witness {
    /// Derive the `wit-<hash8>` id from `campaignId ‖ candidateId`.
    pub fn derived_id(campaign_id: &str, candidate_id: &str) -> String {
        format!("wit-{}", super::hash8(&[campaign_id, candidate_id]))
    }
}

/// One distinct signature and everything known about it (data-model.md § 3).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Finding {
    /// `fnd-<hash8>`, derived from `signature.id` — 1:1 with the signature, which is
    /// what makes a duplicate finding unrepresentable (data-model.md § 1).
    pub id: String,
    /// The deduplication key.
    pub signature: Signature,
    /// At least one, declaration-ordered by first observation (**D1** otherwise).
    #[serde(deserialize_with = "non_empty_witnesses")]
    pub witnesses: Vec<Witness>,
    /// `null` while untriaged — the visible bucket (FR-029).
    #[serde(default)]
    pub classification: Option<Classification>,
    /// The lifecycle state.
    pub state: FindingState,
    /// The campaign that first admitted it — must resolve (**D1**).
    pub first_observed: String,
    /// The most recent campaign that reproduced it — must resolve (**D1**).
    pub last_observed: String,
    /// The registry case carrying it; set only in state `promoted` (**D3**).
    #[serde(default)]
    pub promoted_to: Option<String>,
    /// Provenance when a reviewer split a merged finding. The deduplication rule must
    /// never re-merge a split lineage (FR-032), or a reviewer's judgement is silently
    /// reverted by the next campaign and the split becomes unrepeatable work.
    #[serde(default)]
    pub split_from: Option<String>,
    /// Reviewer prose. Excluded from every hash — annotating a finding must never
    /// re-key it.
    #[serde(default)]
    pub notes: String,
}

impl Finding {
    /// A newly admitted finding: untriaged, unclassified, one witness.
    pub fn newly_admitted(signature: Signature, witness: Witness, campaign_id: &str) -> Finding {
        Finding {
            id: signature.finding_id(),
            signature,
            witnesses: vec![witness],
            classification: None,
            state: FindingState::Untriaged,
            first_observed: campaign_id.to_string(),
            last_observed: campaign_id.to_string(),
            promoted_to: None,
            split_from: None,
            notes: String::new(),
        }
    }

    /// Whether this finding is in a state that requires a classification (`triaged` or
    /// later, excluding `split`, whose classification belongs to its children — Q10).
    pub fn requires_classification(&self) -> bool {
        state_requires_classification(self.state)
    }

    /// A split child's `fnd-<hash8>`, over `parentId ‖ witnessId…` (**T070**).
    ///
    /// A child **cannot** use [`Signature::finding_id`], and the reason is structural
    /// rather than stylistic: a split separates witnesses, not signatures, so every child
    /// carries its parent's signature verbatim. Deriving from the signature would give the
    /// parent and all its children one id — the duplicate the derivation exists to make
    /// unrepresentable.
    ///
    /// Anchoring instead on `parent ‖ the witnesses this child claims` keeps the property
    /// that matters: the id is a function of what the child *is*, so re-authoring the same
    /// split reproduces the same ids (and therefore the same triage state), while moving a
    /// witness between children correctly re-keys both. Witness ids are hashed in the order
    /// given, and [`split_finding`] preserves the parent's witness order, so the result is
    /// stable rather than dependent on iteration order.
    pub fn derive_child_id(parent_id: &str, witness_ids: &[&str]) -> String {
        let mut parts: Vec<&str> = Vec::with_capacity(witness_ids.len() + 1);
        parts.push(parent_id);
        parts.extend_from_slice(witness_ids);
        format!("fnd-{}", super::hash8(&parts))
    }

    /// This finding's id as [`Finding::derive_child_id`] computes it from its own parent
    /// and witnesses — `None` when it is not a split child.
    pub fn derived_child_id(&self) -> Option<String> {
        let parent = self.split_from.as_deref()?;
        let witnesses: Vec<&str> = self.witnesses.iter().map(|w| w.id.as_str()).collect();
        Some(Finding::derive_child_id(parent, &witnesses))
    }

    /// Record a reviewer's classification (**T069**), advancing `untriaged → triaged`.
    ///
    /// Re-classifying a finding that is already `triaged` or `no-longer-reproducing` is
    /// **not** a transition and deliberately leaves the state alone: only a campaign that
    /// actually reproduces a finding may move it out of `no-longer-reproducing`
    /// (contracts/findings-queue.md, "Reproduction lifecycle"). Advancing it here would
    /// assert an observation nothing made and would silently empty the FR-033 bucket the
    /// report exists to show.
    ///
    /// Refused for `promoted` (terminal) and `split` (an inert ancestor that surrendered
    /// classification to its children — Q10).
    pub fn triage(
        &mut self,
        classification: Classification,
        notes: Option<&str>,
    ) -> Result<FindingState, TransitionError> {
        if matches!(self.state, FindingState::Promoted | FindingState::Split) {
            return Err(TransitionError::NotPermitted {
                finding: self.id.clone(),
                from: self.state.as_str(),
                to: FindingState::Triaged.as_str(),
            });
        }
        if self.state == FindingState::Untriaged {
            // Asserted rather than assumed: if the table ever stopped declaring this
            // arrow, triage must fail loudly instead of quietly writing a state the
            // lifecycle no longer recognizes.
            if !self.state.may_transition_to(FindingState::Triaged) {
                return Err(TransitionError::NotPermitted {
                    finding: self.id.clone(),
                    from: self.state.as_str(),
                    to: FindingState::Triaged.as_str(),
                });
            }
            self.state = FindingState::Triaged;
        }
        self.classification = Some(classification);
        if let Some(notes) = notes {
            self.notes = notes.to_string();
        }
        Ok(self.state)
    }

    /// Move a triaged finding to `promoted`, naming the registry case that now carries it.
    ///
    /// Refuses a non-promotable classification **by construction** (FR-035) rather than
    /// leaving it to the checker: `check` runs over committed data, so a promotion that
    /// only failed there would already have been written, and the record it wrote is the
    /// one claiming coverage that cannot exist.
    ///
    /// Whether `case_id` resolves to a real case is **D3**'s question (US5, T080) — this
    /// method cannot answer it without the registry, and inventing a partial check here
    /// would make the real one look redundant.
    pub fn promote(&mut self, case_id: &str) -> Result<(), TransitionError> {
        let Some(classification) = self.classification else {
            return Err(TransitionError::MissingClassification {
                finding: self.id.clone(),
                to: FindingState::Promoted.as_str(),
            });
        };
        if !classification.is_promotable() {
            return Err(TransitionError::NonPromotable {
                finding: self.id.clone(),
                classification: classification.as_str(),
            });
        }
        if !self.state.may_transition_to(FindingState::Promoted) {
            return Err(TransitionError::NotPermitted {
                finding: self.id.clone(),
                from: self.state.as_str(),
                to: FindingState::Promoted.as_str(),
            });
        }
        self.state = FindingState::Promoted;
        self.promoted_to = Some(case_id.to_string());
        Ok(())
    }

    /// Record that this finding's difference stopped reproducing (FR-033).
    ///
    /// **A state, not a deletion.** The disappearance is information: it may mean a fix
    /// landed, or it may mean the generator stopped reaching the input, and only the
    /// retained record tells those apart. [`upsert_finding`] revives it to `triaged`,
    /// keeping its classification, if a later campaign reproduces it.
    pub fn mark_no_longer_reproducing(&mut self) -> Result<(), TransitionError> {
        if self.classification.is_none() {
            return Err(TransitionError::MissingClassification {
                finding: self.id.clone(),
                to: FindingState::NoLongerReproducing.as_str(),
            });
        }
        if !self
            .state
            .may_transition_to(FindingState::NoLongerReproducing)
        {
            return Err(TransitionError::NotPermitted {
                finding: self.id.clone(),
                from: self.state.as_str(),
                to: FindingState::NoLongerReproducing.as_str(),
            });
        }
        self.state = FindingState::NoLongerReproducing;
        Ok(())
    }
}

/// Whether a state requires exactly one classification (Q4/Q10).
///
/// `split` is deliberately excluded: a split ancestor surrendered its classification to
/// its children, because a split exists precisely because one classification could not
/// describe them all.
fn state_requires_classification(state: FindingState) -> bool {
    matches!(
        state,
        FindingState::Triaged | FindingState::Promoted | FindingState::NoLongerReproducing
    )
}

/// Reject an empty witness list at deserialize time (invariant Q2).
///
/// Enforced here, at the shape that reads the file, rather than only in `check`: a
/// finding with no witness is a claim with no evidence, and it should not be
/// representable in memory at all.
fn non_empty_witnesses<'de, D>(de: D) -> Result<Vec<Witness>, D::Error>
where
    D: Deserializer<'de>,
{
    let value = Vec::<Witness>::deserialize(de)?;
    if value.is_empty() {
        return Err(serde::de::Error::custom(
            "a finding must carry at least one witness — a finding with no witness is a \
             claim with no evidence behind it",
        ));
    }
    Ok(value)
}

// ---------------------------------------------------------------------------
// T017 — Campaign, PinnedInputSet, Budget, CampaignOutcome (data-model.md § 4)
// ---------------------------------------------------------------------------

/// Which lane ran a campaign.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CampaignLane {
    /// A scheduled (nightly or weekly) run.
    Scheduled,
    /// An explicitly invoked run.
    Invoked,
}

impl CampaignLane {
    /// The stable wire spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            CampaignLane::Scheduled => "scheduled",
            CampaignLane::Invoked => "invoked",
        }
    }
}

/// The four campaign tiers and, implicitly, their prerequisites (research D10).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CampaignTier {
    /// Deacon-only relation evaluation. Needs **no** oracle, Docker, or network.
    Metamorphic,
    /// deacon vs the pinned oracle over configuration resolution. The **nightly**
    /// scheduled tier.
    ConfigDifferential,
    /// The same comparison with containers brought up. Invoked-only: at the 5-minute
    /// per-candidate ceiling a handful of candidates would consume the whole scheduled
    /// window, so it gets its own budget rather than starving the fast tier.
    ContainerDifferential,
    /// The pinned real-world workspace corpus. The **weekly** network-backed tier — an
    /// ecological canary that runs only on request cannot warn anyone.
    Corpus,
}

impl CampaignTier {
    /// The stable wire spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            CampaignTier::Metamorphic => "metamorphic",
            CampaignTier::ConfigDifferential => "config-differential",
            CampaignTier::ContainerDifferential => "container-differential",
            CampaignTier::Corpus => "corpus",
        }
    }

    /// Every tier, in declaration order.
    pub fn all() -> &'static [CampaignTier] {
        &[
            CampaignTier::Metamorphic,
            CampaignTier::ConfigDifferential,
            CampaignTier::ContainerDifferential,
            CampaignTier::Corpus,
        ]
    }

    /// Whether this tier requires the verified pinned oracle.
    pub fn requires_oracle(self) -> bool {
        !matches!(self, CampaignTier::Metamorphic)
    }
}

/// All **seven** elements of the pinned input set (FR-002, data-model.md § 4).
///
/// Every element is mandatory. A finding is a claim about a specific pinned pair of
/// implementations, and an element left unrecorded is a dimension along which the claim
/// silently stops being checkable.
///
/// The last three are not registry revisions but properties of this repository's own
/// code, and they are separate fields for a reason:
///
/// - **`grammarVersion`** binds the campaign to the constraint inventory. A re-vendored
///   schema pin regenerates the inventory, which changes this string, which correctly
///   invalidates every finding bound to the old value with no separate bookkeeping.
/// - **`generatorVersion`** covers the two things that determine output but are neither
///   a grammar nor a mutation: the pseudorandom stream's algorithm identity (FR-001
///   depends on it) and the reduction catalogue's *order* (FR-020 depends on it).
/// - **`mutationCatalogVersion`** names the mutation **operator set** and nothing else.
///   Folding either of the above into it would name them for something they are not, so
///   a deliberate change to reduction order would look like a change to the operators.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PinnedInputSet {
    /// The vendored schema revision pin (`conformance/schemas/<pin>`) — **D5**.
    pub schema_pin: String,
    /// The vendored spec-prose revision pin (`conformance/spec/<pin>`) — **D5**.
    pub prose_pin: String,
    /// The exact, verified oracle version (FR-003) — **D5**.
    pub oracle_version: String,
    /// `NORMALIZER_VERSION` at the time of the run.
    pub normalizer_version: String,
    /// The constraint inventory's `revision` (research D1).
    pub grammar_version: String,
    /// The mutation operator set's identity.
    pub mutation_catalog_version: String,
    /// The PRNG algorithm identity plus the reduction-catalogue order (research D2/D5).
    pub generator_version: String,
}

/// The default scheduled wall-clock budget, in seconds (research D10).
pub const DEFAULT_WALL_CLOCK_SECONDS: u64 = 1800;
/// The default per-candidate ceiling for the hermetic tiers, in seconds.
pub const DEFAULT_PER_CANDIDATE_SECONDS_HERMETIC: u64 = 60;
/// The default per-candidate ceiling for the container-backed tier, in seconds.
pub const DEFAULT_PER_CANDIDATE_SECONDS_CONTAINER: u64 = 300;
/// The default admission cap: newly distinct signatures admitted per campaign.
///
/// Set from **reviewer throughput**, not machine capacity (research D10): a nightly run
/// that admits more than a couple of dozen genuinely new signatures has produced a
/// backlog nobody clears before the next run. The excess is *reported*
/// ([`CampaignOutcome::signatures_suppressed`]), never discarded silently — so a
/// campaign that keeps hitting the cap is itself a visible signal that something
/// systemic is diverging.
pub const DEFAULT_ADMISSION_CAP: u64 = 25;

/// A campaign's declared budget (data-model.md § 4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Budget {
    /// Wall-clock ceiling for the whole campaign.
    pub wall_clock_seconds: u64,
    /// Ceiling for one candidate; a candidate that exceeds it is discarded **and
    /// counted**, never left to hang the run.
    pub per_candidate_seconds: u64,
    /// Shrink steps allowed per finding before the best reduction is emitted with
    /// `isMinimal: false`.
    pub shrink_steps_per_finding: u64,
    /// Newly distinct signatures admitted per campaign; the excess is reported.
    pub admission_cap: u64,
}

/// What a campaign did (data-model.md § 4).
///
/// **Volume is reported even when nothing was found** (FR-062). A campaign that found
/// nothing and a campaign that never ran are completely different facts, and without
/// `candidatesGenerated` / `candidatesExecuted` they are indistinguishable from the
/// outside — which would make "no findings" the most comfortable possible way for the
/// machinery to be broken.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CampaignOutcome {
    /// Candidates the generator produced.
    pub candidates_generated: u64,
    /// Candidates actually executed against an implementation.
    pub candidates_executed: u64,
    /// Candidates discarded as unsafe before execution (FR-011).
    pub candidates_discarded_unsafe: u64,
    /// Candidates that failed at document parsing — the SC-002 numerator. Above 10% of
    /// `candidatesGenerated`, the campaign explored the parser rather than the tool.
    pub parse_stage_failures: u64,
    /// Whether the wall-clock budget ran out (FR-005).
    pub budget_exhausted: bool,
    /// The fraction of the declared space covered, reported when the budget was
    /// exhausted (FR-005).
    pub space_covered_fraction: f64,
    /// Applications per mutation category. **All eleven keys are always present**,
    /// including zeroes (FR-010): a category absent from the map is indistinguishable
    /// from a category that was never applied, and FR-010 requires zero to be reported
    /// as an explicit generation deficiency — which needs the key there. `IndexMap`
    /// keeps the catalogue's declaration order (constitution VI).
    pub mutation_applications: IndexMap<String, u64>,
    /// Distinct signatures observed.
    pub signatures_observed: u64,
    /// Distinct signatures admitted to the queue.
    pub signatures_admitted: u64,
    /// Distinct signatures the admission cap suppressed (FR-034b) — **never silent**.
    pub signatures_suppressed: u64,
}

/// Provenance for one run (data-model.md § 4). Append-only: a campaign record is never
/// rewritten, because a finding names the campaign that observed it and a rewritten
/// campaign would retroactively change what that finding claims.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Campaign {
    /// `cmp-<hash8>`, derived by [`Campaign::derive_id`] — see that function for what the
    /// id hashes and why the tier is part of it.
    pub id: String,
    /// The recorded seed, hex (FR-001). **Never defaulted** at the CLI: a defaulted seed
    /// would let a campaign run without its reproducibility input being a conscious
    /// choice.
    pub seed: String,
    /// Which lane ran it.
    pub lane: CampaignLane,
    /// Which tier it ran.
    pub tier: CampaignTier,
    /// The certification profile the run happened under — a `prof-` id declared in
    /// `profiles.json` (**D1** when it does not resolve).
    ///
    /// A campaign is a claim about *this* environment: the same seed against the same
    /// pinned pair on `linux/amd64` under Docker and on `linux/amd64` under Podman are two
    /// different observations, and the profile is what the registry already uses to say
    /// which. Recording it also completes the id's substance — see [`Campaign::derive_id`].
    pub profile: String,
    /// The seven pinned inputs.
    pub pinned_input_set: PinnedInputSet,
    /// The declared budget.
    pub budget: Budget,
    /// What it did.
    pub outcome: CampaignOutcome,
}

impl Campaign {
    /// `cmp-<hash8>` over `seed ‖ canonical(pinnedInputSet) ‖ lane ‖ profile ‖ tier`.
    ///
    /// # The four documented components, and the fifth
    ///
    /// data-model.md § 1 lists **four**: `seed ‖ canonical(pinnedInputSet) ‖ lane ‖
    /// profile`. The `tier` is included here as a fifth, deliberately and visibly, because
    /// the four-component form cannot distinguish two campaigns that genuinely exist:
    /// `--seed 0x1 --tier metamorphic` and `--seed 0x1 --tier config-differential`, run in
    /// the same lane under the same profile against the same pins, would share an id. Two
    /// records with one id is a duplicate-id **D1** violation, so the append-only history
    /// could not hold both — and the second run would either be rejected or silently
    /// overwrite the first's provenance, taking every finding that names it along.
    ///
    /// Substance-anchoring exists so that a record which *is* the same thing keeps its id
    /// and a record which is a different thing gets a different one. A tier is a different
    /// thing: it decides which implementations are compared, over which observable
    /// channels, under which prerequisites. Excluding it would make the id anchor to less
    /// than the record's substance, which is the one failure the derivation is for.
    ///
    /// The `canonical(pinnedInputSet)` term is `serde_json`'s rendering of the struct,
    /// whose field order is fixed by the declaration — so it is canonical by construction
    /// rather than by a sorting pass that could be forgotten.
    pub fn derive_id(
        seed: &str,
        pinned_input_set: &PinnedInputSet,
        lane: CampaignLane,
        profile: &str,
        tier: CampaignTier,
    ) -> String {
        let pins = serde_json::to_string(pinned_input_set)
            .unwrap_or_else(|e| unreachable!("a pinned input set always serializes: {e}"));
        format!(
            "cmp-{}",
            super::hash8(&[seed, &pins, lane.as_str(), profile, tier.as_str()])
        )
    }

    /// Recompute this campaign's id from its own fields.
    ///
    /// The identity check **D1** relies on: without it a mis-assigned campaign id is
    /// undetectable, and an undetectable mis-assignment is worse than a missing id — every
    /// finding that names the campaign inherits provenance the record does not actually
    /// carry.
    pub fn derived_id(&self) -> String {
        Campaign::derive_id(
            &self.seed,
            &self.pinned_input_set,
            self.lane,
            &self.profile,
            self.tier,
        )
    }
}

// ---------------------------------------------------------------------------
// T018 — the strict-JSON loader
// ---------------------------------------------------------------------------

/// The `findings.json` envelope.
///
/// `records` is **mandatory**, not `#[serde(default)]`. The writer always emits it, so
/// defaulting buys nothing and costs exactly the failure mode FR-029 exists to prevent: a
/// truncated file would load as an empty queue and validate clean, and "the data was
/// lost" would read as "nothing was found".
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FindingsFile {
    /// Schema version of the file format — rejected at load unless it is
    /// [`SCHEMA_VERSION`], so a future v2 file can never be read under v1 semantics and
    /// silently rewritten as v1.
    #[serde(deserialize_with = "supported_schema_version")]
    pub schema_version: u32,
    /// The findings, in admission order.
    pub records: Vec<Finding>,
}

impl Default for FindingsFile {
    fn default() -> Self {
        FindingsFile {
            schema_version: SCHEMA_VERSION,
            records: Vec::new(),
        }
    }
}

/// The `campaigns.json` envelope. `records` is mandatory for the same reason as
/// [`FindingsFile::records`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CampaignsFile {
    /// Schema version of the file format — see [`FindingsFile::schema_version`].
    #[serde(deserialize_with = "supported_schema_version")]
    pub schema_version: u32,
    /// The campaigns, append-only in run order.
    pub records: Vec<Campaign>,
}

impl Default for CampaignsFile {
    fn default() -> Self {
        CampaignsFile {
            schema_version: SCHEMA_VERSION,
            records: Vec::new(),
        }
    }
}

/// The current discovery data-root schema version.
pub const SCHEMA_VERSION: u32 = 1;

/// Reject a `schemaVersion` this build does not understand.
///
/// Loading a future version under today's semantics would be bad enough on its own; the
/// real hazard is the *write* that follows, because every writer stamps
/// [`SCHEMA_VERSION`]. A `triage` against a v2 file would therefore silently downgrade
/// it, discarding whatever v2 added. Refusing to read is the only way to guarantee not
/// destroying it.
fn supported_schema_version<'de, D>(de: D) -> Result<u32, D::Error>
where
    D: Deserializer<'de>,
{
    let value = u32::deserialize(de)?;
    if value != SCHEMA_VERSION {
        return Err(serde::de::Error::custom(format!(
            "unsupported schemaVersion {value}: this build reads and writes version \
             {SCHEMA_VERSION}, and writing would stamp {SCHEMA_VERSION} over it"
        )));
    }
    Ok(value)
}

/// The loaded discovery data root — all **three** collection files.
///
/// `corpus.json` is loaded here alongside the queue and the campaign history (US7,
/// T105/T106) precisely so the **D4** immutable-reference class runs wherever [`check`]
/// runs: hermetically, in the fast lane, on every pull request. A manifest validated only
/// by the network-backed lane would be validated on the one run per week that can least
/// afford to discover a mutable pin.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct DiscoveryData {
    /// The findings queue, in file order.
    pub findings: Vec<Finding>,
    /// The campaign history, in file order.
    pub campaigns: Vec<Campaign>,
    /// The pinned real-world corpus manifest, in file order.
    pub corpus: Vec<super::corpus::CorpusEntry>,
    /// The canary pins (026, US5), in file order. A sibling of the queue rather than a
    /// registry record, so no loader path from `certify` can reach them (FR-017a).
    pub canary: Vec<CanaryPin>,
}

/// Paths of the three discovery data files under `dir`.
pub fn findings_path(dir: &Path) -> PathBuf {
    dir.join("findings.json")
}

/// Path of the campaign history under `dir`.
pub fn campaigns_path(dir: &Path) -> PathBuf {
    dir.join("campaigns.json")
}

/// Path of the corpus manifest under `dir` (loaded by US7).
pub fn corpus_path(dir: &Path) -> PathBuf {
    dir.join("corpus.json")
}

impl DiscoveryData {
    /// Load the discovery data root at `dir`, collecting **all** file errors in one pass
    /// (the same discipline as [`crate::load::Registry::load`] — a validator that stops
    /// at the first bad file makes fixing a batch an iterative guessing game).
    ///
    /// A missing file is an EMPTY collection, not an error: a fixture root, or a
    /// repository before the first campaign, legitimately has none. A file that is
    /// **present but malformed** is a located [`LoadError::Schema`] — never silently
    /// empty (constitution IV).
    pub fn load(dir: &Path) -> Result<DiscoveryData, LoadError> {
        let mut errors: Vec<SchemaError> = Vec::new();

        let findings_file = findings_path(dir);
        let campaigns_file = campaigns_path(dir);
        let corpus_file = corpus_path(dir);
        let findings = load_file::<FindingsFile>(&findings_file, &mut errors)
            .map(|f| f.records)
            .unwrap_or_default();
        let campaigns = load_file::<CampaignsFile>(&campaigns_file, &mut errors)
            .map(|f| f.records)
            .unwrap_or_default();
        let corpus = load_file::<super::corpus::CorpusFile>(&corpus_file, &mut errors)
            .map(|f| f.records)
            .unwrap_or_default();

        // Duplicate ids are rejected HERE, not only by `check`. Every by-id lookup
        // ([`DiscoveryData::finding_mut`], the CLI's triage/split/scaffold) takes the
        // FIRST match, so a duplicate would let `triage` mutate one record while the
        // other survives untouched — a write that appears to succeed and silently does
        // not. A checker that catches it after the fact is too late.
        errors.extend(duplicate_id_errors(
            &findings_file,
            "finding",
            findings.iter().map(|f| f.id.as_str()),
        ));
        errors.extend(duplicate_id_errors(
            &campaigns_file,
            "campaign",
            campaigns.iter().map(|c| c.id.as_str()),
        ));
        errors.extend(duplicate_id_errors(
            &corpus_file,
            "corpus entry",
            corpus.iter().map(|e| e.id.as_str()),
        ));

        if !errors.is_empty() {
            return Err(LoadError::Schema(errors));
        }
        // Canary pins load alongside the queue because they share its root and its
        // isolation guarantee. A load failure here is a D-class error, not a registry
        // one — the same reason the checker is D6 rather than a V-class.
        let canary = load_canary(dir).map_err(|e| {
            LoadError::Schema(vec![crate::load::SchemaError {
                file: canary_path(dir),
                location: None,
                message: e.to_string(),
            }])
        })?;
        Ok(DiscoveryData {
            findings,
            campaigns,
            corpus,
            canary,
        })
    }

    /// Load the workspace's default discovery data root.
    pub fn load_default() -> Result<DiscoveryData, LoadError> {
        DiscoveryData::load(&crate::default_discovery_dir())
    }

    /// Find a finding by id.
    pub fn finding(&self, id: &str) -> Option<&Finding> {
        self.findings.iter().find(|f| f.id == id)
    }

    /// Find a finding by id, mutably.
    pub fn finding_mut(&mut self, id: &str) -> Option<&mut Finding> {
        self.findings.iter_mut().find(|f| f.id == id)
    }

    /// Find a campaign by id.
    pub fn campaign(&self, id: &str) -> Option<&Campaign> {
        self.campaigns.iter().find(|c| c.id == id)
    }

    /// Find a corpus entry by id.
    pub fn corpus_entry(&self, id: &str) -> Option<&super::corpus::CorpusEntry> {
        self.corpus.iter().find(|e| e.id == id)
    }
}

/// Collect duplicate-id records as located schema errors.
fn duplicate_id_errors<'a>(
    path: &Path,
    kind: &str,
    ids: impl Iterator<Item = &'a str>,
) -> Vec<SchemaError> {
    let mut seen: std::collections::BTreeSet<&str> = std::collections::BTreeSet::new();
    let mut out = Vec::new();
    for id in ids {
        if !seen.insert(id) {
            out.push(SchemaError {
                file: path.to_path_buf(),
                location: Some(id.to_string()),
                message: format!(
                    "duplicate {kind} id `{id}` — every by-id lookup takes the first \
                     match, so a duplicate makes a write appear to succeed while the \
                     other record survives untouched"
                ),
            });
        }
    }
    out
}

/// Read and strictly deserialize one collection file.
///
/// Only a genuine `NotFound` yields `None`. `Path::exists` was the wrong test: it returns
/// `false` for **any** error, so a present-but-unreadable file (a permission problem, a
/// broken symlink) would read as "this repository has not run a campaign yet" instead of
/// as the located error this function promises (constitution IV — no silent fallback).
fn load_file<T: serde::de::DeserializeOwned>(
    path: &Path,
    errors: &mut Vec<SchemaError>,
) -> Option<T> {
    match std::fs::metadata(path) {
        Ok(_) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return None,
        Err(e) => {
            errors.push(SchemaError {
                file: path.to_path_buf(),
                location: None,
                message: format!("could not stat file: {e}"),
            });
            return None;
        }
    }
    match read_file(path).and_then(|raw| deserialize_located::<T>(path, &raw)) {
        Ok(value) => Some(value),
        Err(e) => {
            errors.push(e);
            None
        }
    }
}

// ---------------------------------------------------------------------------
// T019 — the atomic writer
// ---------------------------------------------------------------------------

/// Render a findings file in its canonical, byte-stable form (2-space pretty JSON,
/// trailing newline) — the same rendering every machine-owned artifact in this crate
/// uses, so a queue update produces a reviewable git diff rather than a reformat.
pub fn render_findings(file: &FindingsFile) -> String {
    render(file)
}

/// Render a campaigns file in its canonical, byte-stable form.
pub fn render_campaigns(file: &CampaignsFile) -> String {
    render(file)
}

fn render<T: Serialize>(value: &T) -> String {
    let mut out = serde_json::to_string_pretty(value)
        .unwrap_or_else(|e| unreachable!("discovery record serialization is infallible: {e}"));
    out.push('\n');
    out
}

/// Atomically write the findings queue to `dir/findings.json`.
///
/// Delegates to the single [`crate::atomic_write`] primitive (unique temp file +
/// `fs::rename`), so a shorter payload over a longer one can never leave trailing
/// bytes — the flaky-JSON failure mode a plain `fs::write` has.
pub fn write_findings(dir: &Path, findings: &[Finding]) -> std::io::Result<()> {
    let file = FindingsFile {
        schema_version: SCHEMA_VERSION,
        records: findings.to_vec(),
    };
    crate::atomic_write(&findings_path(dir), &render_findings(&file))
}

/// Atomically write the campaign history to `dir/campaigns.json`.
///
/// Refuses to write a campaign whose `spaceCoveredFraction` is not finite. `serde_json`
/// renders NaN and ±Infinity as **bare `null`** and does **not** error, and `null` is not
/// a valid `f64` on the way back in — so a single division by a zero
/// `candidatesGenerated` (an aborted campaign is exactly that shape) would produce a
/// `campaigns.json` that writes cleanly and can never be loaded again, taking the whole
/// queue's provenance with it. Failing the write is recoverable; writing the file is not.
pub fn write_campaigns(dir: &Path, campaigns: &[Campaign]) -> std::io::Result<()> {
    if let Some(bad) = campaigns
        .iter()
        .find(|c| !c.outcome.space_covered_fraction.is_finite())
    {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!(
                "campaign `{}` has a non-finite spaceCoveredFraction ({}); refusing to \
                 write. serde_json renders it as bare `null`, which never loads back — \
                 the file would be permanently unreadable. Report 0.0 for an unmeasurable \
                 fraction, or fix the divisor.",
                bad.id, bad.outcome.space_covered_fraction
            ),
        ));
    }
    let file = CampaignsFile {
        schema_version: SCHEMA_VERSION,
        records: campaigns.to_vec(),
    };
    crate::atomic_write(&campaigns_path(dir), &render_campaigns(&file))
}

// ---------------------------------------------------------------------------
// T020 — signature-keyed upsert
// ---------------------------------------------------------------------------

/// What an [`upsert_finding`] call did — reported so a campaign can distinguish "we
/// found something new" from "we re-observed something known", which is the difference
/// between a queue that reflects distinct problems and one that reflects campaign volume.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Upsert {
    /// A new finding was inserted.
    Inserted,
    /// A witness was appended to an existing finding.
    WitnessAppended,
    /// The finding already carried this exact witness (the same campaign re-observing
    /// the same candidate); nothing changed.
    AlreadyWitnessed,
    /// The signature belongs to a **split lineage**, so nothing was merged and nothing was
    /// created (FR-032).
    ///
    /// Reported distinctly from [`Upsert::AlreadyWitnessed`] because the two say different
    /// things to a reader of the campaign log: "we have seen this exact observation before"
    /// versus "a reviewer decided this signature covers several causes, and attributing a
    /// new observation to one of them is a judgement only a reviewer can make".
    SplitLineage,
}

/// Insert a new finding for `signature`, or append `witness` to the existing one
/// (FR-030).
///
/// The merge key is the **signature**, and the finding id is derived from it, so a
/// second record for the same signature is unrepresentable rather than merely
/// discouraged.
///
/// Two non-merge rules are honored here because both are load-bearing:
///
/// - **A split lineage is never re-merged** (FR-032). A finding in state
///   [`FindingState::Split`] is an inert ancestor; a new observation of that signature
///   must not append to it, or the next campaign silently reverts the reviewer's
///   judgement and the split becomes unrepeatable work. Such an observation is reported
///   as [`Upsert::AlreadyWitnessed`] against the ancestor — it is genuinely not new, and
///   attributing it to one of the children is a judgement only a reviewer can make.
/// - **Distinct signatures stay distinct** even when they map to the same behavior
///   (FR-031). Nothing here consults behaviors, which is what guarantees it.
///
/// Re-observing a finding also refreshes [`Finding::last_observed`] and revives a
/// [`FindingState::NoLongerReproducing`] record to `triaged` **keeping its
/// classification** — re-triaging a finding a reviewer already judged would be wasted
/// work (contracts/findings-queue.md, "Reproduction lifecycle").
pub fn upsert_finding(
    findings: &mut Vec<Finding>,
    signature: Signature,
    witness: Witness,
    campaign_id: &str,
) -> Upsert {
    let id = signature.finding_id();

    let Some(existing) = findings.iter_mut().find(|f| f.id == id) else {
        // A split lineage is never re-merged (FR-032) — including when the ANCESTOR is
        // gone and only children remain. Without this clause the lookup above misses (a
        // child's id is derived from its parent and witnesses, never from the signature),
        // a fresh merged record is created under the ancestor's id, and the reviewer's
        // decision that these witnesses have different causes is silently undone by the
        // next campaign. Checking the children's own signature is what makes the rule hold
        // on the lineage rather than on one record's continued existence.
        if findings
            .iter()
            .any(|f| f.split_from.is_some() && f.signature.id == signature.id)
        {
            return Upsert::SplitLineage;
        }
        findings.push(Finding::newly_admitted(signature, witness, campaign_id));
        return Upsert::Inserted;
    };

    if existing.state == FindingState::Split {
        // An inert ancestor accepts no new witnesses (invariant Q10). Attributing the
        // observation to one of its children is a judgement only a reviewer can make.
        return Upsert::SplitLineage;
    }
    if existing.witnesses.iter().any(|w| w.id == witness.id) {
        return Upsert::AlreadyWitnessed;
    }

    existing.witnesses.push(witness);
    existing.last_observed = campaign_id.to_string();
    if existing.state == FindingState::NoLongerReproducing {
        existing.state = FindingState::Triaged;
    }
    Upsert::WitnessAppended
}

// ---------------------------------------------------------------------------
// T070 — split with `splitFrom` lineage
// ---------------------------------------------------------------------------

/// What a [`split_finding`] call produced: the ancestor and the children it now has.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Split {
    /// The finding that became an inert ancestor.
    pub parent: String,
    /// The children, in the parent's own witness order. Always ≥ 2 (invariant Q10).
    pub children: Vec<String>,
}

/// Split a signature-merged finding whose witnesses turn out to have different causes
/// (FR-032, invariant Q10).
///
/// # What the split does
///
/// The parent becomes an **inert ancestor**: it keeps its witnesses as historical record,
/// accepts no new ones ([`upsert_finding`] enforces that), and surrenders its
/// classification, because a split exists precisely because one classification could not
/// describe them all. A parent that kept a classification would assert the judgement the
/// split rejected.
///
/// # Why one child per witness
///
/// `discovery split <fnd-id>` takes no grouping argument (contracts/discovery-cli.md), so
/// the only partition derivable from the reviewer's actual statement — "these witnesses
/// have different causes" — is the finest one. Guessing a coarser grouping would re-assert
/// a merge the reviewer just rejected, and it would have to guess *which* witnesses share
/// a cause, which is the very judgement the split was invoked to record. A reviewer who
/// concludes two children share a cause triages them identically; nothing is lost, whereas
/// a wrong merge is not recoverable without re-splitting.
///
/// Each child carries its parent's signature verbatim (a split separates witnesses, not
/// signatures), takes its own [`Finding::derive_child_id`], starts `untriaged` with no
/// classification, and names the parent in `splitFrom`.
///
/// # Errors
///
/// - [`TransitionError::UnknownFinding`] when nothing carries that id.
/// - [`TransitionError::NotPermitted`] when the finding is already `split` or `promoted`,
///   or is not in a state the lifecycle lets a split leave (see
///   [`FindingState::may_transition_to`]).
/// - [`TransitionError::NothingToSplit`] with fewer than two witnesses.
///
/// On any error the queue is left **untouched**: every check runs before the first
/// mutation, so a refused split cannot leave a parent marked inert with no children — a
/// shape that is both a **D2** and a Q10 violation and that nothing would then repair.
pub fn split_finding(
    findings: &mut Vec<Finding>,
    parent_id: &str,
) -> Result<Split, TransitionError> {
    let Some(index) = findings.iter().position(|f| f.id == parent_id) else {
        return Err(TransitionError::UnknownFinding {
            finding: parent_id.to_string(),
        });
    };
    let parent = &findings[index];

    // Witness arity first, deliberately. It is the more fundamental objection — a finding
    // with one witness has nothing to separate whatever state it is in — so reporting it
    // first tells a reviewer the thing they can act on rather than sending them to triage a
    // finding that still could not be split afterwards.
    if parent.witnesses.len() < 2 {
        return Err(TransitionError::NothingToSplit {
            finding: parent.id.clone(),
            witnesses: parent.witnesses.len(),
        });
    }
    if !parent.state.may_transition_to(FindingState::Split) {
        return Err(TransitionError::NotPermitted {
            finding: parent.id.clone(),
            from: parent.state.as_str(),
            to: FindingState::Split.as_str(),
        });
    }

    let signature = parent.signature.clone();
    let children: Vec<Finding> = parent
        .witnesses
        .iter()
        .map(|witness| Finding {
            id: Finding::derive_child_id(parent_id, &[witness.id.as_str()]),
            signature: signature.clone(),
            witnesses: vec![witness.clone()],
            classification: None,
            state: FindingState::Untriaged,
            // The child's provenance is its own witness's campaign, not the parent's:
            // `firstObserved` / `lastObserved` assert an observation of THIS finding, and
            // the parent's may name a campaign that witnessed a sibling instead.
            first_observed: witness.campaign_id.clone(),
            last_observed: witness.campaign_id.clone(),
            promoted_to: None,
            split_from: Some(parent_id.to_string()),
            notes: String::new(),
        })
        .collect();

    let parent = &mut findings[index];
    parent.state = FindingState::Split;
    parent.classification = None;

    let split = Split {
        parent: parent_id.to_string(),
        children: children.iter().map(|c| c.id.clone()).collect(),
    };
    findings.extend(children);
    Ok(split)
}

// ---------------------------------------------------------------------------
// T021 — violation classes D1 and D5
// ---------------------------------------------------------------------------

/// Everything `discovery check` needs from the **registry** to resolve the queue's
/// outward references.
///
/// A borrowed view rather than a whole [`crate::load::Registry`] so the check is
/// explicit about the only four things it reads — the declared channel set, the recorded
/// revisions, the declared profiles, and the declared case ids — and so a test can supply
/// them without building a registry.
///
/// Note the direction: the discovery check reads the registry, never the reverse. That
/// asymmetry is what makes the queue unreachable from `certify`.
#[derive(Debug, Clone, Default)]
pub struct RegistryView<'a> {
    /// Declared observable channel ids (`channels.json`).
    pub channels: Vec<&'a str>,
    /// Recorded source revisions (`revisions.json`).
    pub revisions: Vec<&'a SourceRevision>,
    /// Declared certification profile ids (`profiles.json`) — what a campaign's
    /// [`Campaign::profile`] must resolve to.
    pub profiles: Vec<&'a str>,
    /// Declared case ids (`cases/<area>.json`) — what a promoted finding's
    /// [`Finding::promoted_to`] must resolve to (**D3**).
    pub cases: Vec<&'a str>,
}

impl<'a> RegistryView<'a> {
    /// Build the view from a loaded registry.
    pub fn from_registry(registry: &'a crate::load::Registry) -> RegistryView<'a> {
        RegistryView {
            channels: registry.channels.iter().map(|c| c.id.as_str()).collect(),
            revisions: registry.revisions.iter().collect(),
            profiles: registry.profiles.iter().map(|p| p.id.as_str()).collect(),
            cases: registry.cases.iter().map(|c| c.id.as_str()).collect(),
        }
    }

    fn declares_channel(&self, channel: &str) -> bool {
        self.channels.contains(&channel)
    }

    fn declares_profile(&self, profile: &str) -> bool {
        self.profiles.contains(&profile)
    }

    fn declares_case(&self, case: &str) -> bool {
        self.cases.contains(&case)
    }

    /// Whether a revision of `kind` with this `pin` is recorded.
    ///
    /// Matched on the **pin**, not the id, because a `pinnedInputSet` element records
    /// the pin (`113500f4`, `0.87.0`) rather than the `rev-` id. Accepting the id too
    /// would let two spellings of the same claim diverge.
    fn records_pin(&self, kind: RevisionKind, pin: &str) -> bool {
        self.revisions
            .iter()
            .any(|r| r.kind == kind && r.pin == pin)
    }
}

/// Run the discovery-side violation classes over a loaded data root.
///
/// Reports **all** violations in one pass, matching `validate` — a checker that stops at
/// the first problem makes fixing a batch an iterative guessing game.
///
/// Emits **D1** – **D5**:
///
/// | Class | Checked here |
/// |---|---|
/// | **D1** | derived-id mismatch (signature, finding, **split child**, witness, **campaign**); duplicate id; empty `witnesses`; a signature naming an undeclared channel; a `firstObserved`/`lastObserved` that does not resolve **or is not backed by a witness**; a witness naming an unresolvable campaign; a `splitFrom` that is self-referential or unresolvable; a `split` ancestor with fewer than two children; a campaign `profile` absent from `profiles.json`; a non-finite `spaceCoveredFraction`; an empty seed; an empty repo-owned pinned-input element |
/// | **D2** | a `triaged`/`promoted`/`no-longer-reproducing` finding with no classification; an `untriaged` or `split` finding carrying one; a `promoted` finding classified `normalizer-defect` / `fixture-defect` |
/// | **D3** | a `promoted` finding with no `promotedTo`, or one naming a case absent from the registry; a `promotedTo` carried in any state **other** than `promoted` |
/// | **D4** | a corpus entry whose `commit` is not a 40-hex object name; a malformed `contentDigest`; a corpus id that does not derive from its own substance; a duplicate corpus id or name |
/// | **D5** | a `schemaPin` / `prosePin` / `oracleVersion` naming a revision absent from `revisions.json` |
///
/// **D4**'s second clause — a digest recorded and then removed — is a property of a
/// *change* rather than of a file, so it lives in
/// [`corpus::check_drift`](super::corpus::check_drift), which takes a baseline explicitly.
///
/// D2 is checked here *and* refused by [`Finding::promote`], deliberately: `check` runs
/// over committed data, so a promotion that only failed here would already have been
/// written, and the record it wrote is the one claiming coverage that cannot exist.
///
/// Empty `witnesses` is listed under D1 above and is *also* rejected at deserialize time
/// ([`non_empty_witnesses`]). That is deliberate belt-and-braces: the loader rejection
/// makes the state unrepresentable in memory, and the D1 clause makes a
/// programmatically-built queue (a campaign in flight) fail the same way rather than
/// only failing on the next load.
pub fn check(data: &DiscoveryData, registry: &RegistryView<'_>) -> Vec<DiscoveryError> {
    let mut violations = Vec::new();
    check_findings(data, registry, &mut violations);
    check_classifications(data, &mut violations);
    check_promotions(data, registry, &mut violations);
    check_campaigns(data, registry, &mut violations);
    violations.extend(super::corpus::check(&data.corpus));
    violations.extend(check_canary_pins(&data.canary));
    violations
}

/// **D6 — canary-pin integrity** (026-continuous-conformance-certification, US5).
///
/// A **D**-class, not a V-class, and the numbering is the point: D-classes police the
/// discovery root and block a pull request only on the integrity of the queue itself;
/// V-classes police the registry and several feed `certify`. A V-numbered canary check
/// would put canary state on a code path that reaches the release gate, which is exactly
/// what FR-017a forbids. The class boundary follows the root boundary (research D7).
///
/// | Sub-case | Guards |
/// |---|---|
/// | mutable revision | a branch name, moving tag, or distribution tag — a canary run against a moving target cannot be re-observed, so its findings can never be triaged |
/// | duplicate id | two pins sharing one identity |
/// | derived id | an id that disagrees with its `target ‖ revision` substance |
/// | unknown target | a pin naming something neither implementation nor specification |
pub fn check_canary_pins(pins: &[CanaryPin]) -> Vec<DiscoveryError> {
    let mut violations = Vec::new();
    let mut seen: std::collections::BTreeSet<&str> = std::collections::BTreeSet::new();

    for pin in pins {
        if !seen.insert(pin.id.as_str()) {
            violations.push(DiscoveryError::MalformedRecord {
                record: pin.id.clone(),
                cause: "duplicate canary-pin id — two pins cannot share one identity".to_string(),
            });
        }
        let derived = pin.derived_id();
        if pin.id != derived {
            violations.push(DiscoveryError::MalformedRecord {
                record: pin.id.clone(),
                cause: format!("id is not derived from its substance (expected `{derived}`)"),
            });
        }
        if !revision_is_immutable(&pin.revision) {
            violations.push(DiscoveryError::MalformedRecord {
                record: pin.id.clone(),
                cause: format!(
                    "revision `{}` is mutable. Only a full 40-hex commit identifier or an \
                     exact published version is acceptable (FR-018): a canary run against a \
                     moving target cannot be re-observed, so anything it finds can never be \
                     triaged into a durable record.",
                    pin.revision
                ),
            });
        }
    }
    violations
}

/// Whether a canary revision is immutable: a 40-hex commit, or an exact published version
/// (`major.minor.patch`, optionally with a pre-release or build suffix).
fn revision_is_immutable(revision: &str) -> bool {
    is_full_commit(revision) || is_exact_version(revision)
}

/// A full 40-hex commit identifier. Nothing shorter: an abbreviated hash is not guaranteed
/// unique in perpetuity, so it is not an immutable name for a revision.
fn is_full_commit(revision: &str) -> bool {
    revision.len() == 40 && revision.chars().all(|c| c.is_ascii_hexdigit())
}

/// An exact published version: `MAJOR.MINOR.PATCH`, optionally with a pre-release or build
/// suffix (`0.88.0-rc.1`, `0.88.0+build.5`).
///
/// The three core parts MUST be numeric. A looser "three dot-separated parts" test accepted
/// a branch named `release-1.2.3` — which splits into `release-1`, `2`, `3` — and a branch
/// is exactly the mutable target this check exists to reject.
fn is_exact_version(revision: &str) -> bool {
    let core = revision
        .split_once('-')
        .map(|(core, _pre)| core)
        .unwrap_or(revision);
    let core = core.split_once('+').map(|(c, _build)| c).unwrap_or(core);

    let parts: Vec<&str> = core.split('.').collect();
    parts.len() == 3
        && parts
            .iter()
            .all(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_digit()))
}

/// One canary pin: an upstream **development** revision the non-blocking canary lane
/// compares against (data-model.md §6).
///
/// Lives in the discovery root, never in `revisions.json`. A `rev-canary-*` record there
/// would be loaded by `certify`, and canary state would then be able to change a release
/// verdict (FR-017a).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CanaryPin {
    pub id: String,
    pub target: CanaryTarget,
    pub revision: String,
    pub url: String,
    pub added: String,
}

impl CanaryPin {
    /// The substance-anchored id this record must carry.
    pub fn derived_id(&self) -> String {
        derive_canary_id(self.target, &self.revision)
    }
}

/// What a canary pin points at.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CanaryTarget {
    ReferenceCli,
    Spec,
}

impl CanaryTarget {
    /// The wire name, used in derived ids.
    pub fn as_str(self) -> &'static str {
        match self {
            CanaryTarget::ReferenceCli => "cli",
            CanaryTarget::Spec => "spec",
        }
    }
}

/// Compute a canary pin's derived id.
pub fn derive_canary_id(target: CanaryTarget, revision: &str) -> String {
    format!(
        "cnr-{}-{}",
        target.as_str(),
        super::hash8(&[target.as_str(), revision])
    )
}

/// The `canary.json` document.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CanaryFile {
    #[serde(default)]
    pub schema_version: u32,
    #[serde(default)]
    pub records: Vec<CanaryPin>,
}

/// Path of the canary pins under `dir`.
pub fn canary_path(dir: &Path) -> PathBuf {
    dir.join("canary.json")
}

/// Load the canary pins from a discovery root. A missing file yields none, so a fixture
/// root validates without one.
pub fn load_canary(dir: &Path) -> Result<Vec<CanaryPin>, DiscoveryError> {
    let path = canary_path(dir);
    if !path.is_file() {
        return Ok(Vec::new());
    }
    let raw = std::fs::read_to_string(&path).map_err(|e| DiscoveryError::MalformedRecord {
        record: path.display().to_string(),
        cause: e.to_string(),
    })?;
    let file: CanaryFile =
        serde_json::from_str(&raw).map_err(|e| DiscoveryError::MalformedRecord {
            record: path.display().to_string(),
            cause: e.to_string(),
        })?;
    Ok(file.records)
}

/// D1 over the findings queue.
fn check_findings(
    data: &DiscoveryData,
    registry: &RegistryView<'_>,
    out: &mut Vec<DiscoveryError>,
) {
    let mut seen: std::collections::BTreeSet<&str> = std::collections::BTreeSet::new();

    for finding in &data.findings {
        // Identity: the id must be derivable from the signature, and the signature's id
        // from its own fields. A hand-edited record that broke either would silently
        // stop deduplicating — two records for one signature, which the derived id
        // exists to make impossible.
        let derived_signature_id = finding.signature.derived_id();
        if finding.signature.id != derived_signature_id {
            out.push(DiscoveryError::MalformedRecord {
                record: finding.id.clone(),
                cause: format!(
                    "signature id `{}` does not match its substance (expected `{derived_signature_id}`)",
                    finding.signature.id
                ),
            });
        }
        // Two id rules, one per lineage position (**T070**).
        //
        // A finding the DEDUPLICATION owns takes its id from its signature, which is what
        // makes two findings with one signature unrepresentable. A split CHILD cannot: it
        // carries its parent's signature verbatim (a split separates witnesses, not
        // signatures), so signature-derivation would give the parent and every child one
        // id — exactly the duplicate the derivation exists to prevent. A child therefore
        // anchors on `parent ‖ its own witnesses`, which keeps the same property (the id is
        // a function of what the record is) without the collision.
        //
        // Both are checked. An unchecked derivation is a comment: a hand-edited child id
        // would silently detach the record from its lineage, and re-authoring the same
        // split would then produce a second copy under the correct id.
        match finding.derived_child_id() {
            None => {
                let derived_finding_id = finding.signature.finding_id();
                if finding.id != derived_finding_id {
                    out.push(DiscoveryError::MalformedRecord {
                        record: finding.id.clone(),
                        cause: format!(
                            "finding id does not match its signature (expected \
                             `{derived_finding_id}`) — the id is derived precisely so two \
                             findings cannot share a signature"
                        ),
                    });
                }
            }
            Some(derived_child_id) => {
                if finding.id != derived_child_id {
                    out.push(DiscoveryError::MalformedRecord {
                        record: finding.id.clone(),
                        cause: format!(
                            "split-child id does not match `splitFrom ‖ its witness ids` \
                             (expected `{derived_child_id}`) — a child that is not keyed to \
                             its own lineage detaches from it, and re-authoring the same \
                             split then produces a second copy"
                        ),
                    });
                }
            }
        }
        if !seen.insert(finding.id.as_str()) {
            out.push(DiscoveryError::MalformedRecord {
                record: finding.id.clone(),
                cause: "duplicate finding id".to_string(),
            });
        }

        // Q2: at least one witness.
        if finding.witnesses.is_empty() {
            out.push(DiscoveryError::MalformedRecord {
                record: finding.id.clone(),
                cause: "no witnesses — a finding with no witness is a claim with no evidence"
                    .to_string(),
            });
        }

        // Q3: the signature's channel is declared.
        if !registry.declares_channel(&finding.signature.channel) {
            out.push(DiscoveryError::UnknownChannel {
                record: finding.id.clone(),
                channel: finding.signature.channel.clone(),
            });
        }

        // Q8: firstObserved / lastObserved resolve — and are actually WITNESSED.
        //
        // Existence alone is too weak. `firstObserved` is "the campaign that first
        // admitted it" and `lastObserved` is "the most recent campaign that reproduced
        // it": both descriptions entail a witness from that campaign. A record naming a
        // real but unwitnessed campaign is precisely the "claims provenance it does not
        // have" the error text describes, and it is the shape a careless hand edit
        // produces (bump `lastObserved`, forget the witness).
        for (field, campaign_id) in [
            ("firstObserved", &finding.first_observed),
            ("lastObserved", &finding.last_observed),
        ] {
            if data.campaign(campaign_id).is_none() {
                out.push(DiscoveryError::UnresolvableReference {
                    record: finding.id.clone(),
                    kind: format!("{field} campaign"),
                    reference: campaign_id.clone(),
                });
            } else if !finding
                .witnesses
                .iter()
                .any(|w| &w.campaign_id == campaign_id)
            {
                out.push(DiscoveryError::MalformedRecord {
                    record: finding.id.clone(),
                    cause: format!(
                        "`{field}` names campaign `{campaign_id}`, which witnessed nothing \
                         for this finding — the field asserts an observation, so it must be \
                         backed by a witness"
                    ),
                });
            }
        }

        // Each witness's campaign resolves, and its id matches its substance.
        for witness in &finding.witnesses {
            if data.campaign(&witness.campaign_id).is_none() {
                out.push(DiscoveryError::UnresolvableReference {
                    record: format!("{}/{}", finding.id, witness.id),
                    kind: "witness campaign".to_string(),
                    reference: witness.campaign_id.clone(),
                });
            }
            let derived = Witness::derived_id(&witness.campaign_id, &witness.candidate_id);
            if witness.id != derived {
                out.push(DiscoveryError::MalformedRecord {
                    record: format!("{}/{}", finding.id, witness.id),
                    cause: format!(
                        "witness id does not match `campaignId ‖ candidateId` (expected `{derived}`)"
                    ),
                });
            }
        }

        // splitFrom, when set, must name a real OTHER finding. A self-reference resolves
        // trivially and would otherwise pass, leaving a finding that is its own ancestor
        // — a lineage the deduplication rule would then refuse to merge into itself.
        if let Some(parent) = &finding.split_from {
            if parent == &finding.id {
                out.push(DiscoveryError::MalformedRecord {
                    record: finding.id.clone(),
                    cause: "`splitFrom` names the finding itself — a split lineage has a \
                            parent and children, never one record that is both"
                        .to_string(),
                });
            } else if data.finding(parent).is_none() {
                out.push(DiscoveryError::UnresolvableReference {
                    record: finding.id.clone(),
                    kind: "splitFrom finding".to_string(),
                    reference: parent.clone(),
                });
            }
        }

        // Q10: a split ancestor has at least two children naming it. One child is not a
        // split — it is the same finding with a new id — and zero children is the shape a
        // half-finished split leaves behind: a parent that has surrendered its
        // classification and accepts no new witnesses, with nothing carrying its evidence
        // forward. [`split_finding`] cannot produce either, so reaching them means a hand
        // edit, which is exactly what a checker is for.
        if finding.state == FindingState::Split {
            let children = data
                .findings
                .iter()
                .filter(|f| f.split_from.as_deref() == Some(finding.id.as_str()))
                .count();
            if children < 2 {
                out.push(DiscoveryError::MalformedRecord {
                    record: finding.id.clone(),
                    cause: format!(
                        "state `split` with {children} child(ren); a split ancestor needs \
                         at least two findings naming it in `splitFrom`, because a split \
                         exists precisely to separate witnesses one classification could \
                         not describe"
                    ),
                });
            }
        }
    }
}

/// **D2** — classification arity and promotability (**T068**).
///
/// Q4 and Q6 of contracts/findings-queue.md, plus the half of Q10 that is about
/// classification rather than lineage. All four shapes are one class because they are one
/// defect: the queue asserting a judgement nobody made, or holding one that cannot lead
/// anywhere.
///
/// **"More than one classification" is unrepresentable**, not unchecked: [`Finding`]
/// carries `Option<Classification>`, and a JSON array where a string belongs is rejected by
/// the strict loader before this ever runs. The arity rule therefore reduces here to its
/// only reachable violation — *zero* where exactly one is required. That is the stronger
/// outcome, not a gap: the invariant holds by construction on the side a checker would
/// otherwise have to police.
fn check_classifications(data: &DiscoveryData, out: &mut Vec<DiscoveryError>) {
    for finding in &data.findings {
        match (finding.state, finding.classification) {
            // Q4 — exactly one, once triaged or later.
            (state, None) if state_requires_classification(state) => {
                out.push(DiscoveryError::ClassificationArity {
                    record: finding.id.clone(),
                    cause: format!(
                        "state `{}` requires exactly one classification and the record \
                         carries none",
                        state.as_str()
                    ),
                });
            }
            // Q4 — `null` only while untriaged.
            (FindingState::Untriaged, Some(classification)) => {
                out.push(DiscoveryError::ClassificationArity {
                    record: finding.id.clone(),
                    cause: format!(
                        "state `untriaged` carries classification `{}` — untriaged means \
                         nobody has looked, so a classification here makes the FR-029 \
                         bucket count something that has in fact been judged",
                        classification.as_str()
                    ),
                });
            }
            // Q10 — a split ancestor surrenders its classification to its children.
            (FindingState::Split, Some(classification)) => {
                out.push(DiscoveryError::ClassificationArity {
                    record: finding.id.clone(),
                    cause: format!(
                        "state `split` carries classification `{}` — a split exists \
                         precisely because one classification could not describe all its \
                         witnesses, so a parent that keeps one asserts the judgement the \
                         split rejected",
                        classification.as_str()
                    ),
                });
            }
            _ => {}
        }
        // Q6 — the two machinery classifications can never reach `promoted` (FR-035).
        // Checked independently of the arity arms above so a promoted record with a
        // non-promotable classification is reported for what it is rather than passing
        // because it satisfied the arity rule.
        if finding.state == FindingState::Promoted
            && let Some(classification) = finding.classification
            && !classification.is_promotable()
        {
            out.push(DiscoveryError::ClassificationArity {
                record: finding.id.clone(),
                cause: format!(
                    "state `promoted` with classification `{}`, which is not promotable \
                     (FR-035): it describes a defect in the discovery or comparison \
                     machinery, not a behavior of either implementation, so no registry \
                     case can legitimately carry it",
                    classification.as_str()
                ),
            });
        }
    }
}

/// **D3** — promotion resolution (**T080**, FR-042, data-model.md § 3).
///
/// Q7 of contracts/findings-queue.md: a `promoted` finding names the registry case that
/// now carries it, and that case exists. Three shapes, one class, because all three are
/// the same defect — **the queue claiming coverage that does not exist**:
///
/// - **`promoted` with no `promotedTo`.** The state says a case carries this finding and
///   the record declines to say which, so the claim cannot be checked by anyone. FR-042
///   exists precisely so a promoted finding is not rediscovered and re-triaged, and a
///   promotion naming nothing gives a reviewer no way to reach the case that would tell
///   them the work is done.
/// - **`promotedTo` naming a case the registry does not declare.** The most dangerous
///   shape, and the reason this class needs the registry at all: it reads as covered
///   from inside the queue while nothing anywhere executes the finding. A case deleted
///   or renamed after the promotion produces exactly this, which is why it is checked
///   against the loaded registry on every run rather than only at the moment of
///   promotion.
/// - **`promotedTo` in any other state.** data-model.md § 3 declares the field set *only*
///   in state `promoted`. A `triaged` record carrying one asserts a promotion that never
///   happened; a `no-longer-reproducing` one asserts a case still carries a difference
///   the queue has recorded as gone. Both mislead in the direction of claiming more
///   coverage than exists, which is the direction that matters.
///
/// The check is deliberately **one-way**: it reads the registry's declared case ids and
/// writes nothing. Discovery never authors a case (FR-036), so the only thing D3 can do
/// about an unresolvable promotion is name it.
fn check_promotions(
    data: &DiscoveryData,
    registry: &RegistryView<'_>,
    out: &mut Vec<DiscoveryError>,
) {
    for finding in &data.findings {
        match (finding.state, finding.promoted_to.as_deref()) {
            (FindingState::Promoted, None) => {
                out.push(DiscoveryError::PromotionUnresolved {
                    record: finding.id.clone(),
                    cause: "state `promoted` carries no `promotedTo` — the state asserts \
                            that a registry case now carries this finding, so the record \
                            must name which one or the claim cannot be checked"
                        .to_string(),
                });
            }
            (FindingState::Promoted, Some(case)) if !registry.declares_case(case) => {
                out.push(DiscoveryError::PromotionUnresolved {
                    record: finding.id.clone(),
                    cause: format!(
                        "`promotedTo` names case `{case}`, which is not declared in \
                         `conformance/registry/cases/`. The finding reads as covered from \
                         inside the queue while nothing executes it — the one shape a \
                         promotion must never take"
                    ),
                });
            }
            (state, Some(case)) if state != FindingState::Promoted => {
                out.push(DiscoveryError::PromotionUnresolved {
                    record: finding.id.clone(),
                    cause: format!(
                        "state `{}` carries `promotedTo` `{case}` — the field is set only \
                         in state `promoted`, and carrying one anywhere else asserts a \
                         promotion that did not happen",
                        state.as_str()
                    ),
                });
            }
            _ => {}
        }
    }
}

/// D1 + D5 over the campaign history.
fn check_campaigns(
    data: &DiscoveryData,
    registry: &RegistryView<'_>,
    out: &mut Vec<DiscoveryError>,
) {
    let mut seen: std::collections::BTreeSet<&str> = std::collections::BTreeSet::new();

    for campaign in &data.campaigns {
        if !seen.insert(campaign.id.as_str()) {
            out.push(DiscoveryError::MalformedRecord {
                record: campaign.id.clone(),
                cause: "duplicate campaign id".to_string(),
            });
        }
        // Identity: the id must be derivable from the record's own substance. Without
        // this, a mis-assigned campaign id is undetectable — and every finding naming that
        // campaign silently inherits provenance the record does not carry.
        let derived = campaign.derived_id();
        if campaign.id != derived {
            out.push(DiscoveryError::MalformedRecord {
                record: campaign.id.clone(),
                cause: format!(
                    "campaign id does not match its substance `seed ‖ \
                     canonical(pinnedInputSet) ‖ lane ‖ profile ‖ tier` (expected \
                     `{derived}`)"
                ),
            });
        }
        // The profile is what says *which environment* the claim is about; a profile
        // nothing declares is a claim about nowhere.
        if !registry.declares_profile(&campaign.profile) {
            out.push(DiscoveryError::UnresolvableReference {
                record: campaign.id.clone(),
                kind: "certification profile".to_string(),
                reference: campaign.profile.clone(),
            });
        }
        if campaign.seed.trim().is_empty() {
            out.push(DiscoveryError::MalformedRecord {
                record: campaign.id.clone(),
                cause: "empty seed — the seed is the reproducibility input and is never \
                        defaulted or inferred (FR-001)"
                    .to_string(),
            });
        }
        // Caught here as well as at the write (see `write_campaigns`), so a record built
        // in memory by a campaign in flight fails the same way a committed one would.
        if !campaign.outcome.space_covered_fraction.is_finite() {
            out.push(DiscoveryError::MalformedRecord {
                record: campaign.id.clone(),
                cause: format!(
                    "non-finite spaceCoveredFraction ({}) — it serializes as bare `null`, \
                     which never loads back",
                    campaign.outcome.space_covered_fraction
                ),
            });
        }
        out.extend(check_pinned_input_set(
            &campaign.id,
            &campaign.pinned_input_set,
            registry,
        ));
    }
}

/// **D5** — every pinned-input-set element that names a *registry revision* must resolve
/// in `revisions.json`.
///
/// Three of the seven elements name revisions: the schema pin, the prose pin, and the
/// oracle version. The other four (`normalizerVersion`, `grammarVersion`,
/// `mutationCatalogVersion`, `generatorVersion`) are properties of this repository's own
/// code rather than upstream revisions, so they are *required to be non-empty* here but
/// are not looked up — inventing a `rev-` record for them would claim an upstream
/// provenance they do not have.
pub fn check_pinned_input_set(
    record: &str,
    pins: &PinnedInputSet,
    registry: &RegistryView<'_>,
) -> Vec<DiscoveryError> {
    let mut out = Vec::new();

    for (element, value, kind) in [
        ("schemaPin", &pins.schema_pin, RevisionKind::Schema),
        ("prosePin", &pins.prose_pin, RevisionKind::Spec),
        ("oracleVersion", &pins.oracle_version, RevisionKind::Oracle),
    ] {
        if !registry.records_pin(kind, value) {
            out.push(DiscoveryError::StalePin {
                record: record.to_string(),
                element: element.to_string(),
                value: value.clone(),
            });
        }
    }

    for (element, value) in [
        ("normalizerVersion", &pins.normalizer_version),
        ("grammarVersion", &pins.grammar_version),
        ("mutationCatalogVersion", &pins.mutation_catalog_version),
        ("generatorVersion", &pins.generator_version),
    ] {
        if value.trim().is_empty() {
            out.push(DiscoveryError::MalformedRecord {
                record: record.to_string(),
                cause: format!(
                    "pinned input `{element}` is empty — all seven elements are mandatory \
                     (FR-002); an unrecorded element is a dimension along which the finding \
                     silently stops being checkable"
                ),
            });
        }
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::discovery::signature::{Divergence, DivergenceKind};
    use serde_json::json;

    fn revisions() -> Vec<SourceRevision> {
        vec![
            SourceRevision {
                id: "rev-schema-113500f4".into(),
                kind: RevisionKind::Schema,
                pin: "113500f4".into(),
                url: String::new(),
                verified_against: None,
            },
            SourceRevision {
                id: "rev-spec-113500f4".into(),
                kind: RevisionKind::Spec,
                pin: "113500f4".into(),
                url: String::new(),
                verified_against: None,
            },
            SourceRevision {
                id: "rev-oracle-0-87-0".into(),
                kind: RevisionKind::Oracle,
                pin: "0.87.0".into(),
                url: String::new(),
                verified_against: None,
            },
        ]
    }

    /// The certification profile every test campaign runs under.
    const TEST_PROFILE: &str = "prof-linux-amd64-docker-0870";

    /// The one case id the synthetic registry view declares — what a promotion may
    /// legitimately name (**D3**).
    const TEST_CASE: &str = "case-readconfig-decl-image";

    fn view<'a>(revisions: &'a [SourceRevision]) -> RegistryView<'a> {
        RegistryView {
            channels: vec!["chan-structured-output", "chan-exit-code"],
            revisions: revisions.iter().collect(),
            profiles: vec![TEST_PROFILE],
            cases: vec![TEST_CASE],
        }
    }

    fn pins() -> PinnedInputSet {
        PinnedInputSet {
            schema_pin: "113500f4".into(),
            prose_pin: "113500f4".into(),
            oracle_version: "0.87.0".into(),
            normalizer_version: "6".into(),
            grammar_version: "rev-schema-113500f4".into(),
            mutation_catalog_version: "v1".into(),
            generator_version: "splitmix64-seed+xoshiro256starstar/v1".into(),
        }
    }

    /// A **self-consistent** campaign: its id is derived from its own substance, so
    /// `check` sees the record the derivation promises rather than a hand-picked id that
    /// would (correctly) fail the D1 identity clause. `tag` varies the seed, which is what
    /// makes two calls two different campaigns.
    fn campaign(tag: &str) -> Campaign {
        let seed = format!("0x5eed{tag}");
        let pinned_input_set = pins();
        let lane = CampaignLane::Invoked;
        let tier = CampaignTier::ConfigDifferential;
        Campaign {
            id: Campaign::derive_id(&seed, &pinned_input_set, lane, TEST_PROFILE, tier),
            seed,
            lane,
            tier,
            profile: TEST_PROFILE.to_string(),
            pinned_input_set,
            budget: Budget {
                wall_clock_seconds: DEFAULT_WALL_CLOCK_SECONDS,
                per_candidate_seconds: DEFAULT_PER_CANDIDATE_SECONDS_HERMETIC,
                shrink_steps_per_finding: 64,
                admission_cap: DEFAULT_ADMISSION_CAP,
            },
            outcome: CampaignOutcome {
                candidates_generated: 10,
                candidates_executed: 10,
                candidates_discarded_unsafe: 0,
                parse_stage_failures: 0,
                budget_exhausted: false,
                space_covered_fraction: 0.0,
                mutation_applications: IndexMap::new(),
                signatures_observed: 1,
                signatures_admitted: 1,
                signatures_suppressed: 0,
            },
        }
    }

    /// The derived id of `campaign(tag)` — what a witness or a `firstObserved` must name.
    fn cid(tag: &str) -> String {
        campaign(tag).id
    }

    fn signature(path: &str) -> Signature {
        let d = json!("vscode");
        let r = json!("root");
        Signature::derive(
            "chan-structured-output",
            &Divergence {
                kind: DivergenceKind::Value,
                path,
                deacon: Some(&d),
                reference: Some(&r),
            },
        )
    }

    fn witness(campaign_id: &str, candidate_id: &str) -> Witness {
        Witness {
            id: Witness::derived_id(campaign_id, candidate_id),
            campaign_id: campaign_id.into(),
            candidate_id: candidate_id.into(),
            minimal_input: json!({ "image": "alpine:3.18" }),
            is_minimal: true,
            reduction_steps: vec!["drop-optional-key".into()],
            observed_values: ObservedValues {
                deacon: Some(json!("vscode")),
                reference: Some(json!("root")),
            },
            mutation_operators: vec!["mop-wrong-type".into()],
        }
    }

    fn clean_data() -> DiscoveryData {
        let sig = signature("configuration.remoteUser");
        let w = witness(&cid("a"), "cnd-11111111");
        DiscoveryData {
            findings: vec![Finding::newly_admitted(sig, w, &cid("a"))],
            campaigns: vec![campaign("a")],
            corpus: Vec::new(),
            canary: Vec::new(),
        }
    }

    // --- T016/T017 record models ------------------------------------------

    #[test]
    fn the_contract_example_record_loads() {
        // contracts/findings-queue.md § Record schema, with ids made self-consistent.
        let sig = signature("configuration.remoteUser");
        let w = witness(&cid("a"), "cnd-11111111");
        let raw = format!(
            r#"{{
              "schemaVersion": 1,
              "records": [
                {{
                  "id": "{}",
                  "signature": {},
                  "witnesses": [{}],
                  "classification": "deacon-regression",
                  "state": "triaged",
                  "firstObserved": "{}",
                  "lastObserved": "{}",
                  "promotedTo": null,
                  "splitFrom": null,
                  "notes": ""
                }}
              ]
            }}"#,
            sig.finding_id(),
            serde_json::to_string(&sig).unwrap(),
            serde_json::to_string(&w).unwrap(),
            cid("a"),
            cid("a"),
        );
        let file: FindingsFile = serde_json::from_str(&raw).expect("the contract example loads");
        let finding = &file.records[0];
        assert_eq!(finding.state, FindingState::Triaged);
        assert_eq!(
            finding.classification,
            Some(Classification::DeaconRegression)
        );
        assert_eq!(finding.witnesses.len(), 1);
    }

    #[test]
    fn unknown_fields_are_rejected_at_load() {
        for raw in [
            r#"{"schemaVersion":1,"records":[],"extra":1}"#,
            r#"{"schemaVersion":1,"records":[{"id":"fnd-1","typo":true}]}"#,
        ] {
            let err = serde_json::from_str::<FindingsFile>(raw)
                .expect_err("strict JSON must reject unknown fields");
            let msg = err.to_string();
            assert!(
                msg.contains("extra") || msg.contains("typo") || msg.contains("unknown field"),
                "the diagnosis must name the offending field, got: {msg}"
            );
        }
        let err = serde_json::from_str::<CampaignsFile>(
            r#"{"schemaVersion":1,"records":[],"surprise":true}"#,
        )
        .expect_err("strict JSON must reject unknown fields");
        assert!(err.to_string().contains("surprise"));
    }

    #[test]
    fn a_truncated_file_does_not_load_as_an_empty_queue() {
        // The failure mode FR-029 exists to prevent: "the data was lost" must never read
        // as "nothing was found". The writer always emits `records`, so its absence is
        // damage, not a default.
        let err = serde_json::from_str::<FindingsFile>(r#"{"schemaVersion":1}"#)
            .expect_err("a records-less findings file must not load");
        assert!(err.to_string().contains("records"), "got: {err}");
        let err = serde_json::from_str::<CampaignsFile>(r#"{"schemaVersion":1}"#)
            .expect_err("a records-less campaigns file must not load");
        assert!(err.to_string().contains("records"), "got: {err}");
    }

    #[test]
    fn an_unsupported_schema_version_is_refused_rather_than_downgraded() {
        // Reading a v2 file under v1 semantics would be bad; the real hazard is the write
        // that follows, since every writer stamps SCHEMA_VERSION and would silently
        // downgrade the file. Refusing to read is the only way to guarantee not
        // destroying it.
        let err = serde_json::from_str::<FindingsFile>(r#"{"schemaVersion":2,"records":[]}"#)
            .expect_err("a future schema version must not load");
        let msg = err.to_string();
        assert!(msg.contains("unsupported schemaVersion 2"), "got: {msg}");
        assert!(msg.contains("writing would stamp"), "got: {msg}");
    }

    #[test]
    fn duplicate_ids_are_rejected_at_load_not_only_by_check() {
        // Every by-id lookup takes the FIRST match, so a duplicate makes `triage` mutate
        // one record while the other survives untouched — a write that appears to succeed
        // and silently does not. Catching it only in `check` is too late.
        let dir = tempfile::tempdir().expect("tempdir");
        let data = clean_data();
        let mut findings = data.findings.clone();
        findings.push(findings[0].clone());
        write_findings(dir.path(), &findings).expect("write");
        write_campaigns(dir.path(), &data.campaigns).expect("write");

        let err = DiscoveryData::load(dir.path()).expect_err("duplicates must not load");
        let msg = err.to_string();
        assert!(msg.contains("duplicate finding id"), "got: {msg}");
        assert!(
            msg.contains("first match"),
            "the message must name the consequence"
        );
    }

    // Unix-only: the simulation depends on `ENOTDIR`, which has no Windows equivalent —
    // Windows maps a stat under a non-directory to `ErrorKind::NotFound`, so the probe
    // takes `load_file`'s legitimate "the file is absent" branch and never reaches the
    // error path this test is about. The PRODUCTION guarantee is platform-independent
    // (`load_file` treats only `NotFound` as absent and turns every other stat error into
    // a `SchemaError`); it is the way of provoking a non-`NotFound` stat error that is
    // Unix-specific.
    #[cfg(unix)]
    #[test]
    fn an_unreadable_file_is_not_mistaken_for_an_absent_one() {
        // `Path::exists` returns false for ANY error, so a present-but-unreadable file
        // would read as "this repository has not run a campaign yet". Simulate the
        // stat failure with a path whose parent is a FILE, which yields ENOTDIR.
        let dir = tempfile::tempdir().expect("tempdir");
        let blocker = dir.path().join("blocker");
        std::fs::write(&blocker, "not a directory").expect("write");
        let err = DiscoveryData::load(&blocker).expect_err("an unusable root must not load empty");
        let msg = err.to_string();
        assert!(msg.contains("findings.json"), "got: {msg}");
    }

    #[test]
    fn an_empty_witness_list_is_rejected_at_deserialize_time() {
        let sig = signature("p");
        let raw = format!(
            r#"{{"schemaVersion":1,"records":[{{"id":"{}","signature":{},"witnesses":[],
                "state":"untriaged","firstObserved":"cmp-a","lastObserved":"cmp-a"}}]}}"#,
            sig.finding_id(),
            serde_json::to_string(&sig).unwrap()
        );
        let err = serde_json::from_str::<FindingsFile>(&raw)
            .expect_err("a witness-less finding must not be representable");
        assert!(err.to_string().contains("at least one witness"));
    }

    #[test]
    fn classification_promotability_is_a_closed_property() {
        assert!(Classification::DeaconRegression.is_promotable());
        assert!(Classification::ReferenceQuirk.is_promotable());
        assert!(Classification::SpecAmbiguity.is_promotable());
        assert!(Classification::UnsupportedBehavior.is_promotable());
        assert!(
            !Classification::NormalizerDefect.is_promotable(),
            "a normalizer defect is a defect in the machinery, not a behavior"
        );
        assert!(!Classification::FixtureDefect.is_promotable());
        for c in Classification::all() {
            assert_eq!(Classification::parse(c.as_str()), Some(*c));
        }
        assert_eq!(Classification::parse("probably-fine"), None);
    }

    #[test]
    fn campaign_records_round_trip_through_strict_json() {
        let c = campaign("a");
        let raw = serde_json::to_string_pretty(&c).expect("serializes");
        assert!(raw.contains("\"tier\": \"config-differential\""));
        assert!(raw.contains("\"lane\": \"invoked\""));
        assert!(raw.contains("\"perCandidateSeconds\": 60"));
        let back: Campaign = serde_json::from_str(&raw).expect("round-trips");
        assert_eq!(back, c);
        assert!(CampaignTier::ConfigDifferential.requires_oracle());
        assert!(
            !CampaignTier::Metamorphic.requires_oracle(),
            "the metamorphic tier needs no oracle, Docker, or network (research D12)"
        );
    }

    // --- T018 loader -------------------------------------------------------

    /// The committed root loads and every finding in it is anchored to a campaign that
    /// actually ran.
    ///
    /// This used to assert the root was EMPTY, which was true only because no campaign had
    /// ever produced a committable queue — the drift path wrote witness ids the loader
    /// rejected, so `discovery check` failed on anything a shrink touched. With that fixed
    /// the queue is populated, and an emptiness assertion would forbid the very thing this
    /// feature exists to do. What is worth pinning is the invariant that survives
    /// population: a finding is only as good as the run that witnessed it, so a witness
    /// naming a campaign the history does not contain is a record nobody can re-examine.
    #[test]
    fn every_committed_finding_is_anchored_to_a_campaign_that_ran() {
        let data = DiscoveryData::load_default().expect("the committed discovery root must load");
        for finding in &data.findings {
            assert!(
                !finding.witnesses.is_empty(),
                "{} claims a difference with nothing to show for it",
                finding.id
            );
            for witness in &finding.witnesses {
                assert!(
                    data.campaign(&witness.campaign_id).is_some(),
                    "{}/{} names campaign {}, which is absent from campaigns.json",
                    finding.id,
                    witness.id,
                    witness.campaign_id
                );
            }
        }
    }

    #[test]
    fn a_missing_root_is_empty_but_a_malformed_file_is_a_located_error() {
        let dir = tempfile::tempdir().expect("tempdir");
        let empty = DiscoveryData::load(dir.path()).expect("a missing file is an empty collection");
        assert_eq!(empty, DiscoveryData::default());

        std::fs::write(findings_path(dir.path()), "{ not json").expect("write");
        let err =
            DiscoveryData::load(dir.path()).expect_err("a malformed file must not load empty");
        let msg = err.to_string();
        assert!(msg.contains("findings.json"), "got: {msg}");
    }

    #[test]
    fn every_malformed_file_is_reported_in_one_pass() {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::write(findings_path(dir.path()), "{ nope").expect("write");
        std::fs::write(campaigns_path(dir.path()), "[ also nope").expect("write");
        let err = DiscoveryData::load(dir.path()).expect_err("both files are malformed");
        let msg = err.to_string();
        assert!(
            msg.contains("findings.json") && msg.contains("campaigns.json"),
            "a checker that stops at the first bad file makes fixing a batch a guessing \
             game; got: {msg}"
        );
    }

    // --- T019 atomic writer -----------------------------------------------

    #[test]
    fn the_writer_is_atomic_byte_stable_and_round_trips() {
        let dir = tempfile::tempdir().expect("tempdir");
        let data = clean_data();

        write_findings(dir.path(), &data.findings).expect("write findings");
        write_campaigns(dir.path(), &data.campaigns).expect("write campaigns");

        let back = DiscoveryData::load(dir.path()).expect("re-loads");
        assert_eq!(back, data, "the write/read round trip must be lossless");

        // Byte-stable: writing the same content twice produces identical bytes, and a
        // shorter payload leaves no trailing bytes from the longer one.
        let first = std::fs::read_to_string(findings_path(dir.path())).expect("read");
        write_findings(dir.path(), &data.findings).expect("rewrite");
        assert_eq!(
            first,
            std::fs::read_to_string(findings_path(dir.path())).unwrap()
        );
        write_findings(dir.path(), &[]).expect("truncate");
        let short = std::fs::read_to_string(findings_path(dir.path())).expect("read");
        assert_eq!(short, "{\n  \"schemaVersion\": 1,\n  \"records\": []\n}\n");

        // No temp files survive a successful write.
        let leftovers: Vec<_> = std::fs::read_dir(dir.path())
            .unwrap()
            .filter_map(Result::ok)
            .filter(|e| e.file_name().to_string_lossy().contains(".tmp."))
            .collect();
        assert!(leftovers.is_empty(), "temp files must be renamed away");
    }

    #[test]
    fn the_rendered_empty_file_matches_the_committed_data_root() {
        // `findings.json` and `campaigns.json` are no longer empty — the first campaign
        // whose output the loader accepted populated them — so the claim takes the same
        // form it already took for `corpus.json` below: the committed file IS the canonical
        // rendering of its own records. That is what keeps the NEXT campaign's write a
        // content diff rather than a whole-file reformat, which is the property this test
        // exists for, restated over content rather than over emptiness.
        let discovery_dir = crate::default_discovery_dir();

        let committed = std::fs::read_to_string(discovery_dir.join("findings.json"))
            .expect("findings.json is readable");
        let parsed: FindingsFile =
            serde_json::from_str(&committed).expect("findings.json parses as strict JSON");
        assert!(
            !parsed.records.is_empty(),
            "the queue is populated; an empty one would make this assertion vacuous rather \
             than satisfied"
        );
        assert_eq!(
            committed,
            render_findings(&parsed),
            "findings.json must match the canonical rendering"
        );

        let committed = std::fs::read_to_string(discovery_dir.join("campaigns.json"))
            .expect("campaigns.json is readable");
        let parsed: CampaignsFile =
            serde_json::from_str(&committed).expect("campaigns.json parses as strict JSON");
        assert!(
            !parsed.records.is_empty(),
            "a populated queue implies at least one campaign ran; findings with no campaign \
             history would be unanchored"
        );
        assert_eq!(
            committed,
            render_campaigns(&parsed),
            "campaigns.json must match the canonical rendering"
        );

        // `corpus.json` is no longer empty (US7 T106 populated it with the 33 pinned
        // entries), so the claim is the same one in the form it can still take: the
        // committed file IS the canonical rendering of its own records. That is what makes
        // a digest recorded at first materialization a one-line diff rather than a
        // whole-file reformat — the property this test exists for, restated over content
        // rather than over emptiness.
        let corpus_path = crate::default_discovery_dir().join("corpus.json");
        let committed = std::fs::read_to_string(&corpus_path).expect("corpus.json");
        let parsed: crate::discovery::corpus::CorpusFile =
            serde_json::from_str(&committed).expect("corpus.json parses");
        assert!(
            !parsed.records.is_empty(),
            "the corpus manifest is populated; an empty one would make this assertion \
             vacuous rather than satisfied"
        );
        assert_eq!(
            committed,
            crate::discovery::corpus::render(&parsed),
            "corpus.json must match the canonical rendering"
        );
    }

    // --- T020 upsert -------------------------------------------------------

    #[test]
    fn upsert_inserts_once_and_then_appends_witnesses() {
        let mut findings: Vec<Finding> = Vec::new();
        let sig = signature("configuration.remoteUser");

        assert_eq!(
            upsert_finding(
                &mut findings,
                sig.clone(),
                witness(&cid("a"), "cnd-1"),
                &cid("a")
            ),
            Upsert::Inserted
        );
        assert_eq!(findings.len(), 1);

        // A different campaign observing the SAME signature appends rather than
        // inserting — the queue reflects distinct problems, not campaign volume.
        assert_eq!(
            upsert_finding(
                &mut findings,
                sig.clone(),
                witness(&cid("b"), "cnd-2"),
                &cid("b")
            ),
            Upsert::WitnessAppended
        );
        assert_eq!(findings.len(), 1, "a duplicate finding is unrepresentable");
        assert_eq!(findings[0].witnesses.len(), 2);
        assert_eq!(findings[0].first_observed, cid("a"));
        assert_eq!(findings[0].last_observed, cid("b"));

        // Re-observing the exact same (campaign, candidate) changes nothing.
        assert_eq!(
            upsert_finding(
                &mut findings,
                sig.clone(),
                witness(&cid("b"), "cnd-2"),
                &cid("b")
            ),
            Upsert::AlreadyWitnessed
        );
        assert_eq!(findings[0].witnesses.len(), 2);

        // A DIFFERENT signature stays a different finding.
        assert_eq!(
            upsert_finding(
                &mut findings,
                signature("configuration.remoteEnv"),
                witness(&cid("b"), "cnd-3"),
                &cid("b")
            ),
            Upsert::Inserted
        );
        assert_eq!(findings.len(), 2);
    }

    #[test]
    fn re_observation_revives_a_no_longer_reproducing_finding_keeping_its_classification() {
        let mut findings = vec![Finding::newly_admitted(
            signature("p"),
            witness(&cid("a"), "cnd-1"),
            &cid("a"),
        )];
        findings[0].state = FindingState::NoLongerReproducing;
        findings[0].classification = Some(Classification::DeaconRegression);

        upsert_finding(
            &mut findings,
            signature("p"),
            witness(&cid("b"), "cnd-2"),
            &cid("b"),
        );
        assert_eq!(findings[0].state, FindingState::Triaged);
        assert_eq!(
            findings[0].classification,
            Some(Classification::DeaconRegression),
            "re-triaging a finding a reviewer already judged would be wasted work"
        );
    }

    #[test]
    fn a_split_ancestor_never_accepts_a_new_witness() {
        let mut findings = vec![Finding::newly_admitted(
            signature("p"),
            witness(&cid("a"), "cnd-1"),
            &cid("a"),
        )];
        findings[0].state = FindingState::Split;

        assert_eq!(
            upsert_finding(
                &mut findings,
                signature("p"),
                witness(&cid("b"), "cnd-2"),
                &cid("b")
            ),
            Upsert::SplitLineage,
            "re-merging a split lineage silently reverts a reviewer's judgement (FR-032)"
        );
        assert_eq!(findings[0].witnesses.len(), 1);
        assert_eq!(findings[0].last_observed, cid("a"));
    }

    // --- T070 split lineage -------------------------------------------------

    #[test]
    fn splitting_produces_one_lineage_keyed_child_per_witness() {
        let mut findings = vec![Finding::newly_admitted(
            signature("p"),
            witness(&cid("a"), "cnd-1"),
            &cid("a"),
        )];
        // Two witnesses and a reviewer's classification: the shape a split acts on.
        upsert_finding(
            &mut findings,
            signature("p"),
            witness(&cid("b"), "cnd-2"),
            &cid("b"),
        );
        findings[0]
            .triage(Classification::DeaconRegression, None)
            .expect("triage");

        let parent_id = findings[0].id.clone();
        let split = split_finding(&mut findings, &parent_id).expect("split");
        assert_eq!(split.children.len(), 2);

        // The parent is an inert ancestor: witnesses retained, classification surrendered.
        let parent = findings.iter().find(|f| f.id == parent_id).expect("parent");
        assert_eq!(parent.state, FindingState::Split);
        assert_eq!(parent.classification, None);
        assert_eq!(parent.witnesses.len(), 2);

        for (child_id, source) in split.children.iter().zip(parent.witnesses.clone()) {
            let child = findings.iter().find(|f| &f.id == child_id).expect("child");
            assert_eq!(child.split_from.as_deref(), Some(parent_id.as_str()));
            assert_eq!(child.state, FindingState::Untriaged);
            assert_eq!(child.classification, None);
            assert_eq!(child.witnesses, vec![source.clone()]);
            assert_eq!(child.first_observed, source.campaign_id);
            assert_eq!(child.last_observed, source.campaign_id);
            // Keyed to its own lineage, and NOT to the signature it shares with its parent.
            assert_eq!(child.derived_child_id().as_deref(), Some(child_id.as_str()));
            assert_ne!(child.id, child.signature.finding_id());
        }
        assert_ne!(split.children[0], split.children[1]);
    }

    #[test]
    fn a_refused_split_leaves_the_queue_untouched() {
        // Every check runs before the first mutation, so a refusal cannot leave a parent
        // marked inert with no children — a shape that is both a D2 and a Q10 violation
        // and that nothing would then repair.
        let mut findings = vec![Finding::newly_admitted(
            signature("p"),
            witness(&cid("a"), "cnd-1"),
            &cid("a"),
        )];
        let before = findings.clone();
        let id = findings[0].id.clone();

        let err = split_finding(&mut findings, &id).expect_err("one witness cannot be split");
        assert!(matches!(err, TransitionError::NothingToSplit { .. }));
        assert!(err.to_string().contains("nothing to separate"));
        assert_eq!(findings, before);

        assert!(matches!(
            split_finding(&mut findings, "fnd-nowhere"),
            Err(TransitionError::UnknownFinding { .. })
        ));
        assert_eq!(findings, before);
    }

    #[test]
    fn a_split_lineage_is_never_re_merged_even_after_the_ancestor_is_gone() {
        // The clause that makes FR-032 hold on the LINEAGE rather than on one record's
        // continued existence. Without it the id lookup misses (a child is keyed to its
        // parent, not to the signature), a fresh merged record is created under the
        // ancestor's id, and the reviewer's decision is silently undone.
        let mut findings = vec![Finding::newly_admitted(
            signature("p"),
            witness(&cid("a"), "cnd-1"),
            &cid("a"),
        )];
        upsert_finding(
            &mut findings,
            signature("p"),
            witness(&cid("b"), "cnd-2"),
            &cid("b"),
        );
        let parent_id = findings[0].id.clone();
        findings[0]
            .triage(Classification::DeaconRegression, None)
            .expect("triage");
        split_finding(&mut findings, &parent_id).expect("split");

        findings.retain(|f| f.id != parent_id);
        assert_eq!(
            upsert_finding(
                &mut findings,
                signature("p"),
                witness(&cid("a"), "cnd-9"),
                &cid("a")
            ),
            Upsert::SplitLineage
        );
        assert_eq!(findings.len(), 2, "no merged record may be resurrected");
    }

    // --- T069 state machine -------------------------------------------------

    #[test]
    fn the_transition_table_is_exactly_the_declared_state_machine() {
        use FindingState::*;
        let permitted = [
            (Untriaged, Triaged),
            (Triaged, Split),
            (Triaged, Promoted),
            (Triaged, NoLongerReproducing),
            (NoLongerReproducing, Triaged),
        ];
        for &from in FindingState::all() {
            for &to in FindingState::all() {
                assert_eq!(
                    from.may_transition_to(to),
                    permitted.contains(&(from, to)),
                    "{} -> {}",
                    from.as_str(),
                    to.as_str()
                );
            }
        }
        // The three arrows that look plausible and are absent on purpose.
        assert!(!Untriaged.may_transition_to(NoLongerReproducing));
        assert!(!Untriaged.may_transition_to(Split));
        assert!(!NoLongerReproducing.may_transition_to(Promoted));
        // Terminal states go nowhere.
        for &to in FindingState::all() {
            assert!(!Promoted.may_transition_to(to));
            assert!(!Split.may_transition_to(to));
        }
    }

    #[test]
    fn triage_advances_only_out_of_untriaged_and_never_out_of_a_terminal_state() {
        let mut finding =
            Finding::newly_admitted(signature("p"), witness(&cid("a"), "cnd-1"), &cid("a"));

        assert_eq!(
            finding
                .triage(Classification::DeaconRegression, Some("because"))
                .expect("first triage"),
            FindingState::Triaged
        );
        assert_eq!(finding.notes, "because");

        // Re-classifying is not a transition: the state stays put.
        assert_eq!(
            finding.triage(Classification::SpecAmbiguity, None).unwrap(),
            FindingState::Triaged
        );
        assert_eq!(finding.classification, Some(Classification::SpecAmbiguity));

        // And a no-longer-reproducing finding is classifiable WITHOUT being revived: only
        // a campaign that reproduces it may do that.
        finding
            .mark_no_longer_reproducing()
            .expect("stops reproducing");
        assert_eq!(
            finding
                .triage(Classification::DeaconRegression, None)
                .unwrap(),
            FindingState::NoLongerReproducing
        );

        for terminal in [FindingState::Promoted, FindingState::Split] {
            finding.state = terminal;
            let err = finding
                .triage(Classification::DeaconRegression, None)
                .expect_err("a terminal state is not the reviewer's to re-classify");
            assert!(matches!(err, TransitionError::NotPermitted { .. }));
        }
    }

    #[test]
    fn promotion_requires_a_classification_and_refuses_the_non_promotable_two() {
        let mut finding =
            Finding::newly_admitted(signature("p"), witness(&cid("a"), "cnd-1"), &cid("a"));

        // Untriaged: no classification at all.
        assert!(matches!(
            finding.promote("case-x"),
            Err(TransitionError::MissingClassification { .. })
        ));

        for non_promotable in [
            Classification::NormalizerDefect,
            Classification::FixtureDefect,
        ] {
            finding.state = FindingState::Untriaged;
            finding.classification = None;
            finding.triage(non_promotable, None).expect("triage");
            let err = finding
                .promote("case-x")
                .expect_err("a machinery defect is not a behavior of either implementation");
            assert!(matches!(err, TransitionError::NonPromotable { .. }));
            assert!(err.to_string().contains("not promotable"));
            assert_eq!(
                finding.state,
                FindingState::Triaged,
                "a refused promotion must not move the finding"
            );
            assert_eq!(finding.promoted_to, None);
        }

        finding
            .triage(Classification::DeaconRegression, None)
            .expect("triage");
        finding.promote("case-real").expect("promotable");
        assert_eq!(finding.state, FindingState::Promoted);
        assert_eq!(finding.promoted_to.as_deref(), Some("case-real"));

        // Promoted is terminal.
        assert!(matches!(
            finding.promote("case-other"),
            Err(TransitionError::NotPermitted { .. })
        ));
    }

    #[test]
    fn an_untriaged_finding_that_stops_reproducing_stays_untriaged() {
        // The state requires a classification (D2), so allowing the arrow would manufacture
        // a violation out of a campaign merely not re-observing something nobody had looked
        // at.
        let mut finding =
            Finding::newly_admitted(signature("p"), witness(&cid("a"), "cnd-1"), &cid("a"));
        let err = finding
            .mark_no_longer_reproducing()
            .expect_err("untriaged has no judgement for the disappearance to invalidate");
        assert!(matches!(err, TransitionError::MissingClassification { .. }));
        assert_eq!(finding.state, FindingState::Untriaged);
    }

    // --- T068 D2 ------------------------------------------------------------

    #[test]
    fn d2_a_triaged_or_later_finding_must_carry_a_classification() {
        let revs = revisions();
        for state in [
            FindingState::Triaged,
            FindingState::Promoted,
            FindingState::NoLongerReproducing,
        ] {
            let mut data = clean_data();
            data.findings[0].state = state;
            data.findings[0].classification = None;
            let violations = check(&data, &view(&revs));
            assert!(
                violations
                    .iter()
                    .any(|v| v.class() == "D2" && v.to_string().contains("carries none")),
                "state {}: {violations:?}",
                state.as_str()
            );
        }
    }

    #[test]
    fn d2_an_untriaged_or_split_finding_must_not_carry_one() {
        let revs = revisions();

        let mut data = clean_data();
        data.findings[0].classification = Some(Classification::DeaconRegression);
        assert!(
            check(&data, &view(&revs))
                .iter()
                .any(|v| v.class() == "D2" && v.to_string().contains("nobody has looked")),
            "an untriaged finding carrying a classification makes the FR-029 bucket lie"
        );

        let mut data = clean_data();
        data.findings[0].state = FindingState::Split;
        data.findings[0].classification = Some(Classification::DeaconRegression);
        assert!(
            check(&data, &view(&revs))
                .iter()
                .any(|v| v.class() == "D2"
                    && v.to_string().contains("the judgement the split rejected")),
            "a split parent that keeps a classification asserts what the split rejected"
        );
    }

    #[test]
    fn d2_a_promoted_finding_may_not_be_classified_normalizer_or_fixture_defect() {
        let revs = revisions();
        for non_promotable in [
            Classification::NormalizerDefect,
            Classification::FixtureDefect,
        ] {
            let mut data = clean_data();
            data.findings[0].state = FindingState::Promoted;
            data.findings[0].classification = Some(non_promotable);
            data.findings[0].promoted_to = Some("case-anything".into());
            let violations = check(&data, &view(&revs));
            assert!(
                violations
                    .iter()
                    .any(|v| v.class() == "D2" && v.to_string().contains("not promotable")),
                "{}: {violations:?}",
                non_promotable.as_str()
            );
        }
    }

    // --- T080 D3 ------------------------------------------------------------

    /// A promotion the registry *can* back is clean — the positive control, without which
    /// every assertion below would pass on a check that rejected all promotions.
    #[test]
    fn d3_a_promotion_naming_a_declared_case_is_clean() {
        let revs = revisions();
        let mut data = clean_data();
        data.findings[0].state = FindingState::Promoted;
        data.findings[0].classification = Some(Classification::DeaconRegression);
        data.findings[0].promoted_to = Some(TEST_CASE.to_string());
        assert_eq!(check(&data, &view(&revs)), Vec::new());
    }

    #[test]
    fn d3_a_promoted_finding_must_name_a_case() {
        let revs = revisions();
        let mut data = clean_data();
        data.findings[0].state = FindingState::Promoted;
        data.findings[0].classification = Some(Classification::DeaconRegression);
        data.findings[0].promoted_to = None;
        let violations = check(&data, &view(&revs));
        assert!(
            violations
                .iter()
                .any(|v| v.class() == "D3" && v.to_string().contains("carries no `promotedTo`")),
            "a promotion that names nothing is a claim nobody can check: {violations:?}"
        );
    }

    #[test]
    fn d3_a_promotion_naming_an_unknown_case_is_coverage_that_does_not_exist() {
        let revs = revisions();
        let mut data = clean_data();
        data.findings[0].state = FindingState::Promoted;
        data.findings[0].classification = Some(Classification::DeaconRegression);
        data.findings[0].promoted_to = Some("case-that-was-deleted".to_string());
        let violations = check(&data, &view(&revs));
        assert!(
            violations.iter().any(|v| v.class() == "D3"
                && v.to_string().contains("case-that-was-deleted")
                && v.to_string().contains("not declared")),
            "the diagnosis must NAME the case that does not resolve: {violations:?}"
        );
    }

    /// `promotedTo` outside state `promoted` asserts a promotion that did not happen —
    /// and it always errs toward claiming MORE coverage than exists, which is the
    /// direction that matters.
    #[test]
    fn d3_promoted_to_is_set_only_in_state_promoted() {
        let revs = revisions();
        for (state, classification) in [
            (FindingState::Untriaged, None),
            (
                FindingState::Triaged,
                Some(Classification::DeaconRegression),
            ),
            (
                FindingState::NoLongerReproducing,
                Some(Classification::DeaconRegression),
            ),
        ] {
            let mut data = clean_data();
            data.findings[0].state = state;
            data.findings[0].classification = classification;
            // A case that DOES resolve, so the only objection left is the state.
            data.findings[0].promoted_to = Some(TEST_CASE.to_string());
            let violations = check(&data, &view(&revs));
            assert!(
                violations
                    .iter()
                    .any(|v| v.class() == "D3"
                        && v.to_string().contains("set only in state `promoted`")),
                "state {}: {violations:?}",
                state.as_str()
            );
        }
    }

    #[test]
    fn d1_a_split_ancestor_needs_at_least_two_children() {
        let revs = revisions();
        let mut data = clean_data();
        data.findings[0].state = FindingState::Split;
        let violations = check(&data, &view(&revs));
        assert!(
            violations
                .iter()
                .any(|v| v.class() == "D1" && v.to_string().contains("0 child(ren)")),
            "{violations:?}"
        );
    }

    // --- T021 D1 / D5 ------------------------------------------------------

    #[test]
    fn a_clean_data_root_reports_no_violations() {
        let revs = revisions();
        assert_eq!(check(&clean_data(), &view(&revs)), Vec::new());
    }

    #[test]
    fn the_committed_data_root_validates_clean() {
        let registry = crate::load::Registry::load(&crate::default_registry_dir())
            .expect("the committed registry must load");
        let data = DiscoveryData::load_default().expect("the committed discovery root must load");
        let violations = check(&data, &RegistryView::from_registry(&registry));
        assert!(violations.is_empty(), "{violations:?}");
    }

    #[test]
    fn d1_undeclared_channel() {
        let revs = revisions();
        let mut data = clean_data();
        data.findings[0].signature.channel = "chan-invented".into();
        // Re-derive so the identity checks do not also fire and mask the point.
        data.findings[0].signature.id = data.findings[0].signature.derived_id();
        data.findings[0].id = data.findings[0].signature.finding_id();

        let violations = check(&data, &view(&revs));
        assert_eq!(violations.len(), 1, "{violations:?}");
        assert_eq!(violations[0].class(), "D1");
        assert!(violations[0].to_string().contains("chan-invented"));
    }

    #[test]
    fn d1_unresolvable_campaign_reference() {
        let revs = revisions();
        let mut data = clean_data();
        data.findings[0].last_observed = "cmp-ghost".into();

        let violations = check(&data, &view(&revs));
        assert!(
            violations
                .iter()
                .any(|v| v.class() == "D1" && v.to_string().contains("cmp-ghost")),
            "{violations:?}"
        );
    }

    #[test]
    fn d1_empty_witnesses_and_broken_identity() {
        let revs = revisions();
        let mut data = clean_data();
        data.findings[0].witnesses.clear();
        data.findings[0].id = "fnd-deadbeef".into();

        let violations = check(&data, &view(&revs));
        let text: Vec<String> = violations.iter().map(|v| v.to_string()).collect();
        assert!(
            text.iter().any(|m| m.contains("no witnesses")),
            "expected the empty-witness violation in {text:?}"
        );
        assert!(
            text.iter()
                .any(|m| m.contains("does not match its signature")),
            "expected the derived-id violation in {text:?}"
        );
        assert!(violations.iter().all(|v| v.class() == "D1"));
    }

    #[test]
    fn d1_duplicate_ids_and_dangling_split_parent() {
        let revs = revisions();
        let mut data = clean_data();
        let dup = data.findings[0].clone();
        data.findings.push(dup);
        data.findings[1].split_from = Some("fnd-nowhere".into());
        data.campaigns.push(campaign("a"));

        let text: Vec<String> = check(&data, &view(&revs))
            .iter()
            .map(|v| v.to_string())
            .collect();
        assert!(
            text.iter().any(|m| m.contains("duplicate finding id")),
            "{text:?}"
        );
        assert!(
            text.iter().any(|m| m.contains("duplicate campaign id")),
            "{text:?}"
        );
        assert!(text.iter().any(|m| m.contains("fnd-nowhere")), "{text:?}");
    }

    #[test]
    fn d1_witness_id_must_match_its_substance() {
        let revs = revisions();
        let mut data = clean_data();
        data.findings[0].witnesses[0].id = "wit-00000000".into();

        let violations = check(&data, &view(&revs));
        assert!(
            violations
                .iter()
                .any(|v| v.to_string().contains("witness id does not match")),
            "{violations:?}"
        );
    }

    #[test]
    fn d1_a_mis_assigned_campaign_id_is_detected() {
        // data-model.md § 1 declares the campaign id derived, but a derivation nothing
        // checks is a comment. Without this clause a mis-assigned id is undetectable, and
        // every finding naming that campaign silently inherits provenance the record does
        // not carry.
        let revs = revisions();
        let mut data = clean_data();
        let real = data.campaigns[0].id.clone();
        let forged = "cmp-deadbeef".to_string();
        data.campaigns[0].id = forged.clone();
        data.findings[0].first_observed = forged.clone();
        data.findings[0].last_observed = forged.clone();
        data.findings[0].witnesses[0].campaign_id = forged.clone();
        data.findings[0].witnesses[0].id = Witness::derived_id(&forged, "cnd-11111111");

        let violations = check(&data, &view(&revs));
        assert!(
            violations.iter().any(|v| v.class() == "D1"
                && v.to_string()
                    .contains("campaign id does not match its substance")
                && v.to_string().contains(&real)),
            "the diagnosis must name the id the substance derives to: {violations:?}"
        );
    }

    #[test]
    fn d1_every_component_of_the_campaign_id_changes_it() {
        // Substance-anchoring means the id changes exactly when the thing does. `tier` is
        // the component data-model.md § 1's four-part list omits: without it, a
        // metamorphic and a config-differential run of the same seed in the same lane
        // under the same profile would share an id, which the append-only history cannot
        // represent (it is a duplicate-id D1).
        let base = campaign("a");
        let id = base.id.clone();

        let mut other_seed = base.clone();
        other_seed.seed = "0xdifferent".into();
        assert_ne!(id, other_seed.derived_id(), "seed");

        let mut other_lane = base.clone();
        other_lane.lane = CampaignLane::Scheduled;
        assert_ne!(id, other_lane.derived_id(), "lane");

        let mut other_profile = base.clone();
        other_profile.profile = "prof-linux-amd64-podman-0870".into();
        assert_ne!(id, other_profile.derived_id(), "profile");

        let mut other_pins = base.clone();
        other_pins.pinned_input_set.oracle_version = "0.86.0".into();
        assert_ne!(id, other_pins.derived_id(), "pinnedInputSet");

        let mut other_tier = base.clone();
        other_tier.tier = CampaignTier::Metamorphic;
        assert_ne!(
            id,
            other_tier.derived_id(),
            "a tier decides which implementations are compared over which channels — two \
             tiers are two different observations and must not share an id"
        );

        // The outcome and the budget are NOT substance: re-running the same campaign and
        // recording different volumes does not make it a different campaign.
        let mut other_outcome = base.clone();
        other_outcome.outcome.candidates_generated += 1;
        other_outcome.budget.wall_clock_seconds += 1;
        assert_eq!(id, other_outcome.derived_id());
    }

    #[test]
    fn d1_a_campaign_profile_must_resolve_in_profiles_json() {
        // The profile says *which environment* the claim is about; a profile nothing
        // declares is a claim about nowhere.
        let revs = revisions();
        let mut data = clean_data();
        data.campaigns[0].profile = "prof-imaginary".into();
        data.campaigns[0].id = data.campaigns[0].derived_id();
        let id = data.campaigns[0].id.clone();
        data.findings[0].first_observed = id.clone();
        data.findings[0].last_observed = id.clone();
        data.findings[0].witnesses[0].campaign_id = id.clone();
        data.findings[0].witnesses[0].id = Witness::derived_id(&id, "cnd-11111111");

        let violations = check(&data, &view(&revs));
        assert!(
            violations.iter().any(|v| v.class() == "D1"
                && v.to_string().contains("certification profile")
                && v.to_string().contains("prof-imaginary")),
            "{violations:?}"
        );
    }

    #[test]
    fn d5_pins_absent_from_revisions_json() {
        let revs = revisions();
        let mut data = clean_data();
        data.campaigns[0].pinned_input_set.schema_pin = "deadbeef".into();
        data.campaigns[0].pinned_input_set.oracle_version = "9.9.9".into();

        let violations = check(&data, &view(&revs));
        let stale: Vec<&DiscoveryError> = violations.iter().filter(|v| v.class() == "D5").collect();
        assert_eq!(stale.len(), 2, "{violations:?}");
        assert!(stale.iter().any(|v| v.to_string().contains("deadbeef")));
        assert!(stale.iter().any(|v| v.to_string().contains("9.9.9")));
        assert!(
            stale[0].to_string().contains("revisions.json"),
            "the remedy must name where the revision belongs"
        );
    }

    #[test]
    fn d5_matches_on_the_pin_not_the_revision_id() {
        // A campaign records the PIN (`113500f4`), not the `rev-` id. Accepting the id
        // too would let two spellings of the same claim diverge.
        let revs = revisions();
        let mut data = clean_data();
        data.campaigns[0].pinned_input_set.schema_pin = "rev-schema-113500f4".into();
        let violations = check(&data, &view(&revs));
        assert!(
            violations.iter().any(|v| v.class() == "D5"),
            "{violations:?}"
        );
    }

    #[test]
    fn a_repo_owned_pin_element_may_not_be_empty() {
        let revs = revisions();
        let mut data = clean_data();
        data.campaigns[0].pinned_input_set.generator_version = "  ".into();

        let violations = check(&data, &view(&revs));
        assert!(
            violations
                .iter()
                .any(|v| v.to_string().contains("generatorVersion") && v.class() == "D1"),
            "all seven elements are mandatory; {violations:?}"
        );
    }

    #[test]
    fn a_non_finite_space_covered_fraction_is_refused_by_both_the_writer_and_check() {
        // serde_json renders NaN/±Inf as bare `null` and does NOT error, and `null` is not
        // a valid f64 coming back — so writing one produces a campaigns.json that can
        // never be loaded again, taking the queue's whole provenance with it. A zero
        // `candidatesGenerated` on an aborted campaign is exactly that shape.
        let dir = tempfile::tempdir().expect("tempdir");
        let mut data = clean_data();
        data.campaigns[0].outcome.space_covered_fraction = f64::NAN;

        let err = write_campaigns(dir.path(), &data.campaigns)
            .expect_err("a non-finite fraction must not be written");
        assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
        assert!(err.to_string().contains("non-finite"), "got: {err}");
        assert!(
            !campaigns_path(dir.path()).exists(),
            "the refusal must leave no file behind"
        );

        // And the in-memory record fails the same way, so a campaign in flight is caught
        // before it ever reaches the writer.
        let revs = revisions();
        let violations = check(&data, &view(&revs));
        assert!(
            violations
                .iter()
                .any(|v| v.to_string().contains("non-finite spaceCoveredFraction")),
            "{violations:?}"
        );
    }

    #[test]
    fn d1_last_observed_must_be_backed_by_a_witness() {
        // Existence alone is too weak: `lastObserved` asserts an OBSERVATION, and a
        // record naming a real but unwitnessed campaign claims provenance it does not
        // have — the shape a careless hand edit produces.
        let revs = revisions();
        let mut data = clean_data();
        data.campaigns.push(campaign("b"));
        data.findings[0].last_observed = cid("b");

        let violations = check(&data, &view(&revs));
        assert!(
            violations
                .iter()
                .any(|v| v.to_string().contains("witnessed nothing for this finding")),
            "{violations:?}"
        );
    }

    #[test]
    fn d1_split_from_may_not_name_the_finding_itself() {
        let revs = revisions();
        let mut data = clean_data();
        let id = data.findings[0].id.clone();
        data.findings[0].split_from = Some(id);

        let violations = check(&data, &view(&revs));
        assert!(
            violations
                .iter()
                .any(|v| v.to_string().contains("names the finding itself")),
            "a self-reference resolves trivially and would otherwise pass: {violations:?}"
        );
    }

    #[test]
    fn a_split_child_is_keyed_to_its_lineage_rather_than_to_its_signature() {
        // A child shares its parent's signature by construction (a split separates
        // witnesses, not signatures), so it cannot also carry the parent's derived id.
        // It is not exempt from identity, though — it anchors on `parent ‖ its witnesses`,
        // which keeps the property that matters without the collision.
        let revs = revisions();
        let mut data = clean_data();
        let mut findings = data.findings.clone();
        data.campaigns.push(campaign("b"));
        upsert_finding(
            &mut findings,
            signature("configuration.remoteUser"),
            witness(&cid("b"), "cnd-22222222"),
            &cid("b"),
        );
        findings[0]
            .triage(Classification::DeaconRegression, None)
            .expect("triage");
        let parent_id = findings[0].id.clone();
        split_finding(&mut findings, &parent_id).expect("split");
        data.findings = findings;

        let violations = check(&data, &view(&revs));
        assert!(
            violations.is_empty(),
            "a split produced by the tooling must validate clean: {violations:?}"
        );

        // A hand-edited child id detaches the record from its lineage and is caught.
        data.findings[1].id = "fnd-handwritten".into();
        let violations = check(&data, &view(&revs));
        assert!(
            violations
                .iter()
                .any(|v| v.to_string().contains("split-child id does not match")),
            "{violations:?}"
        );

        // And a NON-child with a wrong id is still judged against its signature.
        data.findings[1].split_from = None;
        let violations = check(&data, &view(&revs));
        assert!(
            violations
                .iter()
                .any(|v| v.to_string().contains("does not match its signature")),
            "{violations:?}"
        );
    }

    #[test]
    fn an_empty_seed_is_a_violation() {
        let revs = revisions();
        let mut data = clean_data();
        data.campaigns[0].seed = String::new();
        let violations = check(&data, &view(&revs));
        assert!(
            violations
                .iter()
                .any(|v| v.to_string().contains("empty seed")),
            "{violations:?}"
        );
    }
}
