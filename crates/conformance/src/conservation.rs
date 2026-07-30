//! Migration **conservation** accounting — the before-and-after proof that no
//! pre-migration coverage was lost (data-model.md §4, contracts/migration-report.md).
//!
//! # Naming — deliberately not "coverage"
//!
//! This crate already owns [`crate::coverage`], which answers *"is every in-profile
//! behavior covered?"* against the active certification profile. This module answers a
//! different question: *"did anything get lost in the move?"*. Two different questions
//! must not share a name or a module (contracts/cli-commands.md), so the migration
//! accounting lives here and the report is emitted as
//! `target/conformance/migration-report.{json,md}` (derived, git-ignored).
//!
//! Implementation lands with User Story 5 (T070–T075): the accounting computation
//! (migrated / deduplicated / residual / retired / unaccounted), the eight failure
//! conditions, error-path direction preservation, the deterministic Markdown rendering,
//! and the residual queue with its `deletableCarriers` list. User Story 3 additionally
//! places indistinguishable-behavior detection (T052) and the behavior-vs-variant
//! counts (T053) here.
//!
//! Nothing is stubbed in the meantime: an unimplemented command must fail loudly rather
//! than report a green accounting it never computed (constitution IV).

use std::collections::{BTreeMap, BTreeSet};

use crate::load::Registry;
use crate::mapping::{CaseFacts, Disposition, VariantGroup, variant_groups};
use crate::model::{BehaviorUnit, CaseKind};

// ---------------------------------------------------------------------------
// Frozen pre-migration totals (research §1g)
// ---------------------------------------------------------------------------

/// The behavior count at the migration's branch point (research §1g). This is the
/// denominator SC-005 forbids inflating: the migration converts duplicate coverage into
/// **variants of one behavior**, so the case count rises while this number holds or
/// falls.
///
/// Authored once, like the baseline's assertions, and for the same reason: a
/// conservation claim measured against a number the claimant can move is not a claim.
/// Lowering it to make a report pass is the anti-gaming case US5 (T069) guards.
pub const PRE_MIGRATION_BEHAVIORS: usize = 25;

/// Behaviors legitimately DECLARED after the branch point, each paired with the reason it
/// is not a variant of an existing claim.
///
/// [`PRE_MIGRATION_BEHAVIORS`] exists to stop the migration faking progress by giving each
/// new *variant* its own behavior — 24 per-workspace cases proving one claim must count as
/// one behavior, not 24. That guard is about **re-describing** existing coverage.
///
/// It is not about **newly observed** conformance facts, and conflating the two would make
/// the guard forbid the thing the migration is for. `chan-container-state` became an
/// observed channel in 024 Phase 4, which retired a blanket rule that had been hiding
/// whole label namespaces from comparison; what it exposed is a real, previously
/// unrecorded difference between the two CLIs. Refusing to record it would keep the
/// denominator pretty at the cost of the registry being silent about something we now
/// know — the exact trade the whole conformance model rejects.
///
/// This mirrors the reasoning [`PRE_MIGRATION_UNITS`] already applies to baseline units:
/// coverage added after the branch point is not coverage that needed conserving. The
/// mechanism is an ENUMERATED list, not a raised number, for the same reason every other
/// allowance in this repository is a finite list: raising the constant would silently
/// re-arm the guard one notch higher forever, while each entry here is a reviewed diff
/// that must say why it is not a variant. Entries are self-invalidating — an id that no
/// longer resolves in the registry is reported, so a deleted behavior cannot leave its
/// allowance behind (the `waiver.rs` staleness pattern).
pub const POST_BRANCH_BEHAVIORS: &[(&str, &str)] = &[
    (
        "bhv-container-identity-labels",
        "deacon stamps five identity/bookkeeping labels the reference CLI does not set at \
     all (`devcontainer.configHash`, `.config_name`, `.name`, `.source`, \
     `.workspaceHash`), measured directly against the pinned oracle 0.87.0 on \
     fx-up-basic. This is a deacon EXTENSION, not a variant of any pre-migration claim: \
     no existing behavior describes what either CLI labels a container with, because the \
     retired `strip_intentional_labels` rule removed the whole `devcontainer.*` namespace \
     before comparison. The three shared keys are NOT part of this behavior — \
     `devcontainer.metadata` compares byte-equal, and `.local_folder` / `.config_file` \
     differ only by each side's own temp workspace path and are normalized, not tolerated.",
    ),
    (
        "bhv-exec-restored-path-ordering",
        "The relative order of a restored image `PATH` entry against one a Feature contributed \
     through `/etc/profile.d`. Newly RECORDABLE only because the #370 fix created the \
     situation it describes: before it, deacon dropped the image's entries outright, so there \
     were never two contributors to order and nothing pre-migration could have stated this. \
     Not a variant of `bhv-exec-image-path-preserved` — that one is about whether an entry \
     SURVIVES, and it is now conformant and aligned; this is about where a surviving entry \
     SITS, and it is a measured divergence. An implementation can preserve every entry and \
     still order them differently, which is exactly the state deacon is in.",
    ),
    (
        "bhv-container-metadata-label-serialization",
        "The BYTE FORM of the `devcontainer.metadata` label — JSON whitespace and object key \
     order — as opposed to the entries it records. Newly RECORDABLE only because #373 closed \
     the CONTENT difference its sibling `bhv-container-metadata-label-content` described: \
     while the entries themselves disagreed, no comparison could reach the serialization, so \
     nothing pre-migration could have stated it. Not a variant of the content claim — an \
     implementation can record exactly the right entries and still emit them in a different \
     byte form, which is precisely the state deacon is in now.",
    ),
    (
        "bhv-container-keepalive-command",
        "The two CLIs install different shell keep-alive commands. Newly RECORDABLE for the \
     same reason as the labels behavior: the legacy `diff_states` captured `cmd` and \
     deliberately did not compare it, so no behavior described it. Not a variant of any \
     pre-migration claim — nothing else states what either CLI sets as the container \
     command. Classified intentional only after measuring `docker stop` equal (245 ms / \
     exit 0 vs 215 ms / exit 0); the same field's earlier 'cannot matter behaviorally' \
     assumption hid a 10s stall.",
    ),
    (
        "bhv-readconfig-discovery-locations",
        "Which on-disk locations configuration discovery searches, including the negative half (a \
     plain `devcontainer.json` at the workspace root is not one of them). No pre-migration \
     behavior described discovery at all — every corpus fixture put its configuration at the \
     one location every implementation searches — so this is a newly RECORDED fact, not a re- \
     description. It also carries a measured divergence: the pinned oracle does not search \
     the spec's one-level-deep sub-folder form.",
    ),
    (
        "bhv-readconfig-unsupported-enum-rejected",
        "A value outside a CLOSED schema enum. Distinct from the wrong-TYPE rejections already \
     recorded: those are a JSON type mismatch caught by deserialization shape, this is a \
     well-typed string outside a closed value set. An implementation can plausibly get one \
     right and the other wrong, so they are two claims.",
    ),
    (
        "bhv-readconfig-substitution-object-fields",
        "Substitution reaching the OBJECT-shaped template-carrying fields. The pre-migration \
     corpus exercised substitution in scalars only, and `customizations` was missed for \
     exactly that reason (#312) — which is the evidence that this is a distinct claim rather \
     than a variant of the scalar one.",
    ),
    (
        "bhv-readconfig-features-configuration-omitted-when-none",
        "Whether the Features block is REPORTED at all when resolution found none, which is \
     a different claim from what the block contains when it is present. An empty container \
     asserts \"resolution ran and produced nothing\"; saying nothing asserts less, and a \
     reader distinguishes them. Nothing pre-migration described the features-configuration \
     document at all — the corpora compared the merged configuration only — so this is a \
     newly recorded fact rather than a re-description.",
    ),
    (
        "bhv-readconfig-merged-lifecycle-slots",
        "Which lifecycle hook SLOTS the merged document reports, as opposed to what any of \
     them contains. Falsifiable on its own: an implementation that merges every declared \
     hook correctly can still omit the undeclared slots, which makes \"no layer set this \
     hook\" indistinguishable from \"the merge dropped it\" for a reader. Nothing \
     pre-migration described the merged document's key set, only the values under the keys \
     that happened to be present.",
    ),
    (
        "bhv-readconfig-port-attributes-authored-keys",
        "Which attribute keys a port entry reports. A distinct claim from the port \
     attribute VALUES and from the top-level absent-optional serialization already tracked \
     at tasks.md#T111: this one is nested inside an object, where the harness's \
     `drop_absent_optional` rule does not reach, so it was neither described by a behavior \
     nor absorbed by normalization — it was simply invisible until the blanket `prune` \
     rule retired.",
    ),
    (
        "bhv-readconfig-config-file-path",
        "WHICH configuration file the resolution actually used, reported as a path rather \
     than inferred from the values. Separately falsifiable from every content behavior: an \
     implementation can resolve the right file and name the wrong one, or name the right \
     one having merged the wrong chain, and no behavior about the configuration's CONTENT \
     distinguishes those. Newly recordable because the retired `prune` normalization \
     dropped this key outright before comparison, which is why nothing pre-migration \
     described it.",
    ),
    (
        "bhv-readconfig-workspace-folder-subpath",
        "Where the workspace lands INSIDE the container when the workspace folder is nested \
     under the mounted root, and what the default mount target is independently of \
     `workspaceFolder`. No pre-migration behavior described the `workspace` section at all — \
     every corpus fixture sat directly at its own root, so the subpath was always empty and \
     the two values coincided. Not a variant of the tier-1 corpus claim: an implementation \
     that reports every configuration field correctly can still report the wrong container \
     path, which is what deacon did (#383).",
    ),
    (
        "bhv-readconfig-workspace-section-shape",
        "The FIELD SET of the `workspace` section, as distinct from the values in it. \
     Separately falsifiable: an implementation can report the right `workspaceFolder` and \
     still emit fields the reference does not have, which is exactly the state this behavior \
     was recorded from — 104 diverging occurrences on two host-path fields whose VALUES were \
     never in question. Nothing pre-migration described the output envelope, because the \
     retired `prune` normalization dropped absent-on-one-side keys before comparison.",
    ),
    (
        "bhv-readconfig-feature-resolution-order",
        "The RESOLVED Feature set and its order as `read-configuration` reports it, including a \
     `--additional-features` overlay joining that resolution. Nothing pre-migration read the \
     features-configuration document; the corpora compared the merged configuration only.",
    ),
    (
        "bhv-up-feature-install-order",
        "Features actually installed into the created container, in resolved order. The pre- \
     migration `up` coverage is one behavior about a subsequent `exec` observing the \
     environment; it says nothing about whether a Feature ran, which the marker file this \
     behavior's case reads is the first evidence of.",
    ),
    (
        "bhv-up-feature-install-failure",
        "A Feature that cannot be installed failing `up`. Not a variant of the install-ORDER \
     claim: an implementation that installs the right Features in the right order and \
     swallows a failing install script satisfies that one and not this one, and hands back a \
     container that looks complete. Newly recordable because the pre-migration corpora had no \
     container-backed failure tier at all — 024 US4 is that tier.",
    ),
    (
        "bhv-up-restart-reuses-container",
        "Re-entry into an existing container versus first creation. Newly recordable because the \
     metamorphic oracle that can express a RELATIONSHIP between two runs arrived in 022; \
     before it there was no way to state this claim, only to state each run's output.",
    ),
    (
        "bhv-up-changed-config-recreates-container",
        "Re-entry whose CONFIGURATION has changed since the container was created. Not a variant \
     of the reuse claim, and provably so: the two diverge in opposite directions on the same \
     run. deacon reattaches when the document is unchanged and provisions a NEW container \
     when it changed, because its identity includes a configHash; the pinned oracle reattaches \
     in BOTH cases. An implementation satisfying the reuse claim can therefore fail this one, \
     which is what makes it a second claim. Newly recordable in 024 US3: nothing pre-migration \
     re-ran an operation against a mutated configuration at all.",
    ),
    (
        "bhv-exec-image-path-preserved",
        "Whether PATH entries the IMAGE contributed via `ENV PATH` survive into an `exec` \
     session. Not a variant of the exec command-parity claim: every variable in the case \
     resolves identically on both sides and only PATH differs, so an implementation \
     satisfying command parity, argument passing and exit-code propagation still fails this. \
     Newly recordable because the pre-migration exec coverage never ran a command that had to \
     RESOLVE through PATH — it compared output of commands named by absolute path or already \
     on the default PATH.",
    ),
    (
        "bhv-down-missing-config-idempotent",
        "Teardown over a workspace with NO configuration succeeding rather than failing, and \
     still resolving a container recorded for the workspace path. Distinct from the removal \
     claim: that one is about a teardown that has a configuration to scope itself with, this \
     one is about the absence of one, and the two are served by different code paths \
     (identity-from-config versus auto-discovery). Newly recordable for the same reason as \
     the removal claim — `down` had zero cases and the pinned reference has no such command.",
    ),
    (
        "bhv-build-features-extended-image",
        "Which image `build` reports and tags when Features are layered. The one pre-migration \
     `build` behavior is about the build OUTCOME; this is about the identity of the artifact \
     — the distinction a shipped defect already turned on.",
    ),
    (
        "bhv-build-failure-reported",
        "A build-time failure and the STAGE it is attributed to. No pre-migration behavior \
     described a failing build: the corpora's build coverage was entirely on the success \
     path, which is the hole the container-backed error-path tier exists to close.",
    ),
    (
        "bhv-run-user-commands-hook-order",
        "Lifecycle hook ORDER, and a Feature-contributed hook running exactly once. The operation \
     had zero cases before this story, so there is no pre-migration claim for this to be a \
     variant of.",
    ),
    (
        "bhv-run-user-commands-hook-failure",
        "A lifecycle hook that exits non-zero failing the run. Separate from the ordering claim: \
     an implementation that runs hooks in the right order and ignores their exit status \
     satisfies one and not the other.",
    ),
    (
        "bhv-down-removes-container",
        "Teardown removing the workspace's container. `down` had zero cases and the pinned \
     reference has no such command, so nothing pre-migration could have described it.",
    ),
    (
        "bhv-down-compose-project-teardown",
        "Teardown of a Compose PROJECT, identified by the project name and labels `up` derived. \
     Distinct from the single-container claim: the identification path is different, and it \
     is the one that fails silently.",
    ),
    (
        "bhv-doctor-diagnostics-report",
        "Host, platform and runtime diagnostics, and their independence from the workspace \
     configuration. `doctor` had zero cases and the pinned reference has no such command.",
    ),
    (
        "bhv-templates-apply-scaffolds-options",
        "Template option substitution into the scaffolded files. `templates apply` had zero \
     cases; its result is bytes on disk rather than a document, so no pre-migration \
     structured-output claim could have covered it.",
    ),
    (
        "bhv-outdated-reports-feature-versions",
        "Current/wanted/latest version reporting for each configured Feature. `outdated` had \
     ZERO cases before this story, so there is no pre-migration claim this could be a \
     variant of: the pre-migration corpora never invoked the operation, and no behavior \
     described what a version report contains. It is also not a variant of the lockfile \
     behaviors — reporting what is available is a different claim from writing what is \
     pinned, and an implementation can do either without the other.",
    ),
    (
        "bhv-outdated-extends-chain-features",
        "Version reporting over a RESOLVED extends chain, where the pinned oracle reports an \
     empty table. Distinct from the plain reporting claim because it is the one that carries \
     a measured divergence, and because an implementation reporting versions correctly for a \
     single document can still miss an inherited Feature.",
    ),
    (
        "bhv-upgrade-regenerates-lockfile",
        "Lockfile regeneration from the effective configuration, including a Feature a parent \
     link of an extends chain contributed. `upgrade` had ZERO cases before this story, so \
     no pre-migration claim exists for this to re-describe. Not a variant of the `outdated` \
     behaviors either: reporting available versions and WRITING the pinned set are separate \
     operations with separate outputs, and the lockfile is the only one of the two that \
     changes what a later run resolves.",
    ),
    (
        "bhv-upgrade-empty-feature-set",
        "Regenerating a lockfile for a configuration with NO Features. Separate from the general \
     regeneration claim because it is the boundary the pinned oracle FAILS on, so the two \
     have different reference axes and cannot be one record.",
    ),
    (
        "bhv-up-lifecycle-command-forms",
        "A lifecycle hook in the ARRAY (argv) form and in the OBJECT (named commands) form. \
     Nothing pre-migration described either form: the corpora compared resolved \
     CONFIGURATION documents, where a hook is a value to echo, and never ran one. Not a \
     variant of `bhv-run-user-commands-hook-order`, which claims the ORDER hooks run in \
     for a single form — an implementation can order hooks correctly and still drop the \
     second command of an object-form hook.",
    ),
    (
        "bhv-up-lifecycle-command-cwd",
        "The working DIRECTORY a lifecycle hook runs in. Not a variant of \
     `bhv-up-lifecycle-command-forms`, which claims each declared FORM of a hook is \
     executed and says nothing about where it executes from: an implementation can run \
     both forms of every hook out of the image's default directory, satisfying that claim \
     and breaking this one for every hook that resolves a relative path. Newly recordable \
     because the sentence carrying the rule — features-contribute-lifecycle-scripts' \"as \
     with all lifecycle hooks\" clause, the clause inventory's ONLY carrier of it — has no \
     observation until a hook can report its own `pwd` into a bind-mounted file, which \
     needs the container-backed tier 024 built.",
    ),
    (
        "bhv-up-feature-entrypoint-chain",
        "Entrypoints contributed by MULTIPLE Features, chained. Newly recordable in the same \
     way the labels and keep-alive behaviors were: `Config.Entrypoint` was captured and \
     deliberately not compared, so no behavior described it, and the two CLIs build the \
     chain by different mechanisms that only an effect-level observation can relate. Not a \
     variant of `bhv-up-feature-install-order`: installing the right Features in the right \
     order says nothing about whether their entrypoints then run.",
    ),
    (
        "bhv-up-container-env-merge-precedence",
        "Which layer wins when the configuration AND a Feature declare the same `containerEnv` \
     variable. No pre-migration claim covers it — `bhv-readconfig-merged-configuration` \
     compares the merged DOCUMENT, and deacon's document was correct here while the \
     container it created was not, which is exactly the gap a document-only comparison \
     leaves. Recorded after measuring both sides: the defect it found (deacon merged the \
     Feature layer OVER the configuration's) is fixed in the same change.",
    ),
    (
        "bhv-up-path-construction",
        "PATH as CONSTRUCTED in the created container, including a segment a Feature \
     contributes. Newly recordable because `drop_noise_env` removed `PATH` at capture until \
     024 US5, so no case could see it. Not a variant of the containerEnv precedence claim: \
     PATH is the one variable both layers legitimately WRITE TO rather than set, so \
     last-writer-wins is the wrong rule for it and getting precedence right does not imply \
     getting the prepend right.",
    ),
    (
        "bhv-up-effective-user-uid-gid",
        "The effective user of the container process and the UID/GID it resolves to, for a user \
     the image creates and for one a FEATURE creates. `bhv-state-container-parity` compares \
     whatever the observed fixtures happened to contain and none declared a non-root user; \
     more importantly it compares the DECLARATION, while the ids are only observable by \
     running something inside the container, which no pre-migration unit did.",
    ),
    (
        "bhv-up-mount-source-and-shape",
        "A mount's SOURCE as distinct from its SHAPE. Not recordable before 024 US5: the \
     observable-state snapshot carried only `sourceTail`, the bind's leaf component, so two \
     mounts rooted at different host directories compared equal and no claim about sources \
     could be evidenced. `bhv-state-container-parity` covers the shape half only, for the \
     same reason.",
    ),
    (
        "bhv-container-metadata-label-content",
        "What the `devcontainer.metadata` label actually CONTAINS. Newly recordable in 024 US5 \
     for two independent reasons: the label was compared only where a case happened to reach \
     it, and the second of its two differences (substituted absolute paths where the reference \
     keeps the author\'s `${localWorkspaceFolder}` template) was unobservable while a mount \
     source was collapsed to its leaf component. Not a variant of \
     `bhv-container-identity-labels`, which is about labels the reference does not set at all — \
     this one is a label BOTH set, with different contents.",
    ),
    (
        "bhv-compose-project-file-set",
        "How each CLI delivers its generated Compose override, and the two Compose labels \
     derived from the resulting file set. No pre-migration unit compared Compose bookkeeping \
     labels. Not a variant of `bhv-compose-project-name-robust`: an implementation could adopt \
     either CLI\'s project naming and still deliver its override the other way, so the two are \
     independent claims.",
    ),
    (
        "bhv-readconfig-authored-empty-omitted-collapsed",
        "Whether a resolved-configuration result distinguishes an authored `null`, an authored \
     empty collection, and an OMITTED property. Structurally unrecordable before 024 US5: \
     `drop_absent_optional` ran on both sides, so all three states normalized to the same \
     observation and no pre-migration unit could have reported a difference. Not a variant \
     of the readconfig strictness family — those are REJECTIONS of malformed input; this is \
     a fidelity loss on input both sides accept.",
    ),
];

/// The observable-channel count at the branch point (research §1g).
pub const PRE_MIGRATION_CHANNELS: usize = 11;

/// The EXECUTABLE baseline-unit count at the migration's branch point: 91
/// `live-per-case` plus 16 `hermetic-guard` plus 4 `internal-consistency` (research §1).
/// The 33 recorded-only `external-corpus-entry` units are inventoried but never counted
/// as migrated (D8).
///
/// This is the denominator the conservation accounting measures against. It is
/// deliberately NOT "however many units the baseline currently holds": User Story 4
/// (T055/T056) adds fault-injection guard units, so the committed baseline legitimately
/// grows. Coverage ADDED after the branch point is not coverage that needed conserving,
/// and folding it into the denominator would let new tests paper over a lost old one.
pub const PRE_MIGRATION_EXECUTABLE_UNITS: usize = 111;

/// The characterized-exception count at the branch point: 10 `wvr-` + 6 `ext-`
/// (research §1g).
pub const PRE_MIGRATION_EXCEPTIONS: usize = 16;

// ---------------------------------------------------------------------------
// Behavior / variant counts (T053, FR-017)
// ---------------------------------------------------------------------------

/// The behavior-level and variant-level counts the migration report separates
/// (FR-017).
///
/// Reporting one number for "cases" hides the whole point of US3: a migration that turns
/// one coarse pointer case into 24 variants of ONE behavior is working correctly, and a
/// migration that turns it into 24 new behaviors is inflating the denominator. Only
/// separate counts can tell those apart.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DenominatorCounts {
    /// Behaviors declared in the registry.
    pub behaviors: usize,
    /// Behaviors that at least one case reaches.
    pub behaviors_covered: usize,
    /// Behaviors reached by more than one case — i.e. behaviors that have variants.
    pub behaviors_with_variants: usize,
    /// Total cases.
    pub cases: usize,
    /// Declarative cases (migration destinations).
    pub declarative_cases: usize,
    /// Legacy binary-backed pointer cases (pre-migration carriers).
    pub legacy_cases: usize,
    /// Variant instances: for every behavior reached by N > 1 cases, the N cases count
    /// as N variants. A behavior with exactly one case contributes no variant.
    pub variants: usize,
}

impl DenominatorCounts {
    /// Whether the behavior denominator grew beyond the frozen pre-migration total plus
    /// the explicitly accounted post-branch behaviors (SC-005).
    ///
    /// Growth past that allowance means a variant was authored as a new behavior — the
    /// gaming case. Growth WITHIN it is a newly observed conformance fact that named
    /// itself in [`POST_BRANCH_BEHAVIORS`] and said why it is not a variant.
    pub fn denominator_inflated(&self) -> bool {
        self.behaviors > PRE_MIGRATION_BEHAVIORS + POST_BRANCH_BEHAVIORS.len()
    }
}

/// Count behaviors, cases and variants over a loaded registry (T053).
pub fn denominator_counts(registry: &Registry) -> DenominatorCounts {
    let cases = case_facts(registry);
    let groups = variant_groups(&cases);

    let behaviors_covered = groups.len();
    let behaviors_with_variants = groups.iter().filter(|g| g.is_variant_group()).count();
    let variants: usize = groups
        .iter()
        .filter(|g| g.is_variant_group())
        .map(|g| g.cases.len())
        .sum();
    let declarative_cases = cases.iter().filter(|c| c.declarative).count();

    DenominatorCounts {
        behaviors: registry.behaviors.len(),
        behaviors_covered,
        behaviors_with_variants,
        cases: cases.len(),
        declarative_cases,
        legacy_cases: cases.len() - declarative_cases,
        variants,
    }
}

/// Characterized exceptions authored AFTER the migration's branch point, id-sorted.
///
/// An exception qualifies when it characterizes at least one behavior and EVERY behavior
/// it names is accounted in [`POST_BRANCH_BEHAVIORS`]. Such a record describes something
/// the pre-migration system never knew, so it has no pre-migration form for
/// `mapping.json` to preserve, and the "every exception must be dispositioned" rules do
/// not apply to it.
///
/// This lives here, public, because FOUR separate places encode that rule — `validate`'s
/// V21, the migration report's condition 2, and two test binaries — and the first three
/// were each patched inline before the fourth surfaced. Four copies of one predicate is
/// three chances for them to disagree.
///
/// A record naming no behavior, or mixing pre- and post-branch behaviors, is NOT exempt:
/// the conservative reading, so a genuinely pre-migration tolerance can never slip out of
/// the accounting by acquiring a post-branch behavior.
pub fn post_branch_exceptions(registry: &Registry) -> BTreeSet<String> {
    let post_branch: BTreeSet<&str> = POST_BRANCH_BEHAVIORS.iter().map(|(id, _)| *id).collect();
    let qualifies = |behaviors: &[String]| {
        !behaviors.is_empty() && behaviors.iter().all(|b| post_branch.contains(b.as_str()))
    };
    // BOTH exception mechanisms, not only extensions. A `wvr-` authored after the branch
    // point characterizes a difference the pre-migration system never observed, so it has
    // no pre-migration form for `mapping.json` to preserve — exactly the ground on which
    // an `ext-` is exempt. Covering only one of the two would make the exemption depend on
    // which mechanism an author reached for rather than on when the fact was learned.
    registry
        .extensions
        .iter()
        .filter(|e| qualifies(&e.behaviors))
        .map(|e| e.id.clone())
        .chain(
            registry
                .waivers
                .iter()
                .filter(|w| qualifies(&w.behaviors))
                .map(|w| w.id.clone()),
        )
        .collect()
}

/// [`POST_BRANCH_BEHAVIORS`] entries that no longer resolve in the registry, id-sorted.
///
/// An allowance for a behavior that has since been deleted or renamed silently raises the
/// ceiling by one forever, which is precisely the "number the claimant can move" that
/// [`PRE_MIGRATION_BEHAVIORS`] was frozen to prevent. Reported the same way a waiver whose
/// difference stopped reproducing is reported: stale, and named.
pub fn stale_post_branch_behaviors(registry: &Registry) -> Vec<&'static str> {
    let known: BTreeSet<&str> = registry.behaviors.iter().map(|b| b.id.as_str()).collect();
    let mut stale: Vec<&'static str> = POST_BRANCH_BEHAVIORS
        .iter()
        .map(|(id, _)| *id)
        .filter(|id| !known.contains(id))
        .collect();
    stale.sort_unstable();
    stale
}

/// The variant groups of a loaded registry, ID-sorted by behavior (T051/T053).
pub fn registry_variant_groups(registry: &Registry) -> Vec<VariantGroup> {
    variant_groups(&case_facts(registry))
}

/// Lift the registry's cases into the mapping module's vocabulary-free facts. Mirrors
/// `validate::case_facts`; kept here as the single accessor the report uses so the
/// report and the validator agree on what a "case" contributes.
fn case_facts(registry: &Registry) -> Vec<CaseFacts> {
    registry
        .cases
        .iter()
        .map(|case| {
            let declarative = matches!(case.classify(), Ok(CaseKind::Declarative));
            let mut channels: Vec<String> = case
                .expected
                .iter()
                .map(|e| e.channel.clone())
                .chain(case.outcomes.iter().map(|o| o.channel.clone()))
                .collect();
            channels.sort();
            channels.dedup();
            let mut fixtures: Vec<String> = case
                .operations
                .iter()
                .flat_map(|op| op.fixtures.iter().cloned())
                .collect();
            fixtures.sort();
            fixtures.dedup();
            let oracle = match (&case.oracle_type, &case.executable) {
                (Some(kind), _) => format!("{kind:?}"),
                (None, Some(exe)) => format!("binary:{}", exe.binary),
                (None, None) => "unspecified".to_string(),
            };
            let input_shape = if case.operations.is_empty() {
                oracle.clone()
            } else {
                case.operations
                    .iter()
                    .map(|op| {
                        format!(
                            "{} {} [{}]",
                            op.subcommand,
                            op.argv.join(" "),
                            op.fixtures.join(",")
                        )
                    })
                    .collect::<Vec<_>>()
                    .join(" ; ")
            };
            CaseFacts {
                id: case.id.clone(),
                behaviors: case.behaviors.clone(),
                channels,
                fixtures,
                declarative,
                context: case.context.iter().map(|c| format!("{c:?}")).collect(),
                oracle,
                input_shape,
            }
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Indistinguishable-behavior detection (T052, FR-016)
// ---------------------------------------------------------------------------

/// Two behaviors the detector cannot tell apart, reported for merge or explicit
/// differentiation (FR-016).
///
/// This is a **report**, not a violation. A suspected duplicate may be a genuine
/// duplicate that should be merged, or two real claims whose statements happen to read
/// alike and need differentiating prose. A validator cannot decide which; a human can.
/// Blocking on it would push authors toward padding statements with noise words to
/// escape the check, which is worse than the duplication.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SuspectedDuplicate {
    /// The two behavior ids, ID-sorted.
    pub behaviors: (String, String),
    /// Why they look alike.
    pub reason: DuplicateReason,
}

/// Why two behaviors are suspected duplicates.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DuplicateReason {
    /// Their statements normalize to the same substance AND they are reached by the
    /// same case set — nothing distinguishes them at all.
    IdenticalStatementAndCoverage,
    /// Their statements normalize to the same substance; their coverage differs.
    IdenticalStatement,
}

impl DuplicateReason {
    /// The wire spelling, for report rendering.
    pub fn as_str(self) -> &'static str {
        match self {
            DuplicateReason::IdenticalStatementAndCoverage => "identical-statement-and-coverage",
            DuplicateReason::IdenticalStatement => "identical-statement",
        }
    }
}

/// Detect behaviors that are indistinguishable by statement, reporting the pairs for
/// merge or differentiation (T052, FR-016).
///
/// The substance of a statement is its lowercased alphanumeric word bag with structural
/// filler removed, so two statements that differ only in wording, punctuation or word
/// order are recognized as the same claim. Coverage is the set of cases reaching the
/// behavior — when that matches too, there is nothing at all to tell the pair apart.
pub fn suspected_duplicate_behaviors(registry: &Registry) -> Vec<SuspectedDuplicate> {
    let coverage = coverage_by_behavior(registry);

    let mut by_substance: BTreeMap<Vec<String>, Vec<&BehaviorUnit>> = BTreeMap::new();
    for behavior in &registry.behaviors {
        by_substance
            .entry(statement_substance(&behavior.statement))
            .or_default()
            .push(behavior);
    }

    let mut out = Vec::new();
    for (_substance, group) in by_substance {
        if group.len() < 2 {
            continue;
        }
        for (i, a) in group.iter().enumerate() {
            for b in group.iter().skip(i + 1) {
                let empty = BTreeSet::new();
                let a_cases = coverage.get(a.id.as_str()).unwrap_or(&empty);
                let b_cases = coverage.get(b.id.as_str()).unwrap_or(&empty);
                let reason = if a_cases == b_cases {
                    DuplicateReason::IdenticalStatementAndCoverage
                } else {
                    DuplicateReason::IdenticalStatement
                };
                let (first, second) = if a.id <= b.id {
                    (a.id.clone(), b.id.clone())
                } else {
                    (b.id.clone(), a.id.clone())
                };
                out.push(SuspectedDuplicate {
                    behaviors: (first, second),
                    reason,
                });
            }
        }
    }
    out.sort_by(|a, b| a.behaviors.cmp(&b.behaviors));
    out
}

/// Case ids reaching each behavior.
fn coverage_by_behavior(registry: &Registry) -> BTreeMap<&str, BTreeSet<&str>> {
    let mut out: BTreeMap<&str, BTreeSet<&str>> = BTreeMap::new();
    for case in &registry.cases {
        for behavior in &case.behaviors {
            out.entry(behavior.as_str())
                .or_default()
                .insert(case.id.as_str());
        }
    }
    out
}

/// Words that carry no distinguishing substance in a behavior statement. Removing them
/// means "deacon rejects a cyclic extends chain" and "a cyclic extends chain is
/// rejected by deacon" compare equal — which is the point: they are the same claim.
const FILLER_WORDS: &[&str] = &[
    "a", "an", "the", "is", "are", "be", "and", "or", "of", "to", "in", "on", "for", "with",
    "that", "which", "it", "its", "as", "at", "by", "from", "this", "than", "then", "so", "not",
    "no", "any", "all", "into", "when", "via", "using", "there",
];

/// The distinguishing substance of a statement: its sorted, de-duplicated, lowercased,
/// inflection-normalized alphanumeric words minus [`FILLER_WORDS`].
fn statement_substance(statement: &str) -> Vec<String> {
    let mut words: Vec<String> = statement
        .split(|c: char| !c.is_alphanumeric())
        .map(|w| w.to_ascii_lowercase())
        .filter(|w| !w.is_empty() && !FILLER_WORDS.contains(&w.as_str()))
        .map(|w| stem(&w))
        .collect();
    words.sort();
    words.dedup();
    words
}

/// Strip ONE trailing English inflection so active and passive phrasings of the same
/// claim compare equal ("rejects" / "rejected" / "rejecting" → "reject").
///
/// Deliberately crude and deliberately conservative: a minimum stem length of four
/// characters keeps short words intact, and only one suffix is removed. It exists to
/// catch the common voice/tense rewrite, not to be a stemmer. Over-stemming would
/// manufacture false duplicates, which is worse than missing one — a missed duplicate
/// costs a reviewer nothing, while a false one erodes trust in the whole report. It is
/// applied identically to both statements, so it can never make two DIFFERENT claims
/// look alike unless they were already one word apart.
fn stem(word: &str) -> String {
    for suffix in ["ing", "ed", "es", "s"] {
        if let Some(base) = word.strip_suffix(suffix) {
            if base.len() >= 4 {
                return base.to_string();
            }
        }
    }
    word.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn statement_substance_ignores_wording_order_and_voice() {
        assert_eq!(
            statement_substance("deacon rejects a cyclic extends chain."),
            statement_substance("A cyclic extends chain is rejected by deacon!")
        );
        assert_ne!(
            statement_substance("deacon rejects a cyclic extends chain"),
            statement_substance("deacon rejects a missing extends target")
        );
    }

    #[test]
    fn stem_is_conservative_and_removes_one_suffix() {
        for (word, want) in [
            ("rejects", "reject"),
            ("rejected", "reject"),
            ("rejecting", "reject"),
            ("produces", "produc"),
            // Short words stay intact — a four-character stem floor.
            ("is", "is"),
            ("uses", "uses"),
            ("ties", "ties"),
        ] {
            assert_eq!(stem(word), want, "stem({word:?})");
        }
    }

    #[test]
    fn filler_only_statements_collapse_but_substance_survives() {
        assert!(statement_substance("the a an of to").is_empty());
        assert_eq!(
            statement_substance("Build IMAGE build"),
            vec!["build", "image"]
        );
    }

    #[test]
    fn denominator_inflation_is_relative_to_the_frozen_total() {
        let mut counts = DenominatorCounts {
            behaviors: PRE_MIGRATION_BEHAVIORS,
            behaviors_covered: 25,
            behaviors_with_variants: 3,
            cases: 94,
            declarative_cases: 69,
            legacy_cases: 25,
            variants: 60,
        };
        assert!(!counts.denominator_inflated());
        // Within the explicitly accounted allowance: still not inflation.
        counts.behaviors = PRE_MIGRATION_BEHAVIORS + POST_BRANCH_BEHAVIORS.len();
        assert!(!counts.denominator_inflated());
        // One past it: inflation, whatever the allowance currently holds.
        counts.behaviors = PRE_MIGRATION_BEHAVIORS + POST_BRANCH_BEHAVIORS.len() + 1;
        assert!(counts.denominator_inflated());
    }
}

// ---------------------------------------------------------------------------
// Normalization-rule registry (T059/T061, data-model §6) + V24
// ---------------------------------------------------------------------------

/// What a normalization rule does to the value it is scoped to (data-model §6).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuleAction {
    /// Substitutes content in place (e.g. a temp path → a stable token).
    Rewrite,
    /// Reshapes a value into a canonical form (e.g. `"k=v"` strings → an object).
    Canonicalize,
    /// Splits a value so it compares element-wise rather than as one string.
    Segment,
    /// REMOVES named fields. The only action that loses information, and therefore the
    /// only one that requires an enumerated `removes` list and a justification.
    Drop,
}

impl RuleAction {
    /// The wire spelling, for report rendering and diagnostics.
    pub fn as_str(self) -> &'static str {
        match self {
            RuleAction::Rewrite => "rewrite",
            RuleAction::Canonicalize => "canonicalize",
            RuleAction::Segment => "segment",
            RuleAction::Drop => "drop",
        }
    }
}

/// A registered normalization rule (data-model §6, FR-021).
///
/// Registering every rule turns FR-021 from an aspiration into something checkable: a
/// blanket rule cannot be added without failing **V24**, and a reviewer can read the
/// complete list of what the comparison elides in one place.
///
/// **Deviation from data-model §6**: `scopes` is a LIST where §6 writes a single `scope`.
/// Several rules legitimately apply to more than one channel (`path_token` applies to
/// eight), and an enumerated list of channels is strictly narrower than the "all" scope
/// §6 forbids — whereas one registry entry per (rule, channel) pair would triple the
/// registry and split one rule's justification across eight rows. Every scope is still
/// explicit; none may be `all`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NormalizationRule {
    /// The rule's name, matching the function that implements it.
    pub name: &'static str,
    /// Explicit scopes, each `channel:<chan-id>` or `field:<json-pointer>`. Non-empty;
    /// never `all`.
    pub scopes: &'static [&'static str],
    /// What the rule does.
    pub action: RuleAction,
    /// For [`RuleAction::Drop`]: the FINITE, ENUMERATED field names removed. Empty for
    /// every other action — only a drop removes anything.
    pub removes: &'static [&'static str],
    /// Why the rule is sound. Required for a drop, where it must name the specific
    /// fields and why they are not observable behavior.
    pub justification: Option<&'static str>,
    /// Set when the rule is registered but does NOT satisfy FR-021 — recorded honestly,
    /// so the registry is a truthful inventory rather than a list of things that passed.
    ///
    /// A **declared** deficiency is reported by [`declared_non_compliant_rules`] as
    /// tracked debt and does NOT fire V24, for the same reason a residual does not block
    /// certification while a gap does: the problem is admitted, explained and queued.
    /// An **undeclared** blanket rule fires V24 and blocks. Setting this field is a
    /// conspicuous source edit that must carry a reason naming a tracked follow-up
    /// (enforced below), so it cannot be used as a silent escape hatch.
    pub known_non_compliant: Option<&'static str>,
}

/// Every normalization rule the harness applies (T061).
///
/// Ordered by name. Adding a rule means adding a row here; a rule that is not registered
/// is invisible to review, and a rule that is registered but blanket fails **V24**.
pub const NORMALIZATION_RULES: &[NormalizationRule] = &[
    NormalizationRule {
        name: "compose_project_prefix",
        scopes: &["channel:chan-container-state"],
        action: RuleAction::Rewrite,
        removes: &[],
        justification: Some(
            "Strips a Compose project's `<project>_` prefix from a network or volume name \
             so two CLIs that derive different project names still compare on the resource \
             itself (bhv-compose-project-name-robust). Rewrite, never delete. Registered \
             in 024 US5 (T123): it had been applied inside the container-state capture \
             since the observable-state port without ever appearing in this registry, and \
             it discarded the project identity leaving no trace in the evidence. The \
             derived `composeProjectResources.project` field now records the name that was \
             stripped, so the rewrite is auditable from a snapshot alone.",
        ),
        known_non_compliant: None,
    },
    NormalizationRule {
        name: "container_hostname_token",
        scopes: &["channel:chan-container-state"],
        action: RuleAction::Rewrite,
        removes: &[],
        justification: Some(
            "Rewrites a container-id-shaped `HOSTNAME` env value (12 lowercase-hex \
             characters, Docker's default) to `<CONTAINER_HOSTNAME>`, on the `env` entry \
             and the derived `envMap` value. Two containers created by two CLIs always \
             disagree here for a reason that says nothing about either CLI. A hostname the \
             configuration actually set is not 12 hex characters and is left alone. Added \
             in 024 US5 (T123) so `drop_noise_env` no longer has to delete `PATH` and \
             `HOME` to get rid of `HOSTNAME`.",
        ),
        known_non_compliant: None,
    },
    NormalizationRule {
        name: "devcontainer_id_token",
        scopes: &["field:/configuration", "field:/mergedConfiguration"],
        action: RuleAction::Rewrite,
        removes: &[],
        justification: Some(
            "Rewrites the literal `${devcontainerId}` token, and a 12-char lowercase-hex \
             run ONLY inside the enumerated devcontainer-id-bearing fields (containerEnv, \
             mounts, name, remoteEnv, runArgs, workspaceMount), so a container identity \
             computed by one CLI compares equal to the other's unsubstituted template. \
             Replaces the retired blanket `replace_hex12`, which rewrote any 12-hex run \
             anywhere and could collapse two genuinely different digests (023 T063).",
        ),
        known_non_compliant: None,
    },
    NormalizationRule {
        name: "drop_absent_optional",
        scopes: &["field:/configuration", "field:/mergedConfiguration"],
        action: RuleAction::Drop,
        // Mirrors `parity_harness::normalize::ABSENT_OPTIONAL_KEYS`, kept in lockstep by
        // `normalization_rules.rs`.
        removes: &[
            "appPort",
            "build",
            "capAdd",
            "containerEnv",
            "containerUser",
            "customizations",
            "description",
            "dockerComposeFile",
            "dockerFile",
            "elevateIfNeeded",
            "features",
            "forwardPorts",
            "gpu",
            "hostRequirements",
            "image",
            "init",
            "initializeCommand",
            "label",
            "mounts",
            "onAutoForward",
            "onCreateCommand",
            "openPreview",
            "otherPortsAttributes",
            "overrideCommand",
            "overrideFeatureInstallOrder",
            "portsAttributes",
            "postAttachCommand",
            "postCreateCommand",
            "postStartCommand",
            "privileged",
            "protocol",
            "remoteEnv",
            "remoteUser",
            "requireLocalPort",
            "runArgs",
            "runServices",
            "secrets",
            "securityOpt",
            "service",
            "shutdownAction",
            "updateContentCommand",
            "updateRemoteUserUID",
            "userEnvProbe",
            "waitFor",
            "workspaceFolder",
            "workspaceMount",
        ],
        justification: Some(
            "Each named key is a modeled `devcontainer.json` property that deacon \
             serializes unconditionally while the reference omits it when unauthored; the \
             rule removes it ONLY when its value carries no information (null, [], {}, \
             \"\"), so a populated value is always compared. The two documents describe \
             the same resolved configuration in different JSON shapes. Its REACH is \
             bounded to match this scope: the document's own top level plus the two \
             spec-defined nested containers whose properties were part of the measured \
             set (`hostRequirements`, `portsAttributes`). It does NOT descend into \
             arbitrary sub-documents — a `label` or `description` inside \
             `customizations.vscode.settings` is user data and is compared, not elided. \
             NARROWED in 024 US5 (T123) to the SIDE whose defect it compensates: it \
             applies to `/configuration` on deacon's side only, because the reference's \
             `configuration` is an echo of the authored document and an empty value there \
             is authorship information — eliding it on both sides is what made an \
             authored null, an authored empty and an omission indistinguishable (FR-055). \
             It still applies to `/mergedConfiguration` on BOTH sides, where each CLI \
             synthesizes computed empties of its own. This compensates for a deacon \
             serializer defect (it should apply `skip_serializing_if`) and is deleted when \
             that lands — tracked at \
             specs/023-migrate-parity-to-conformance/tasks.md#T111. Replaces the retired \
             blanket `prune` (023 T062, research D3).",
        ),
        known_non_compliant: None,
    },
    NormalizationRule {
        name: "drop_noise_env",
        scopes: &["field:/env"],
        action: RuleAction::Drop,
        removes: &["PATH", "HOME", "HOSTNAME", "TERM", "container"],
        justification: Some(
            "These five environment variables are injected by the container runtime into \
             every container regardless of the CLI that created it, so they carry no \
             cross-CLI outcome meaning. The set is finite and enumerated; the newer \
             `chan-injected-process` channel removes no env var at all. NARROWED in 024 \
             US5 (T123) from CAPTURE to the legacy `diff_states` COMPARISON: the \
             declarative `chan-container-state` observer delegates to the same \
             `container_state` capture, so removing these at capture also removed them \
             from the declarative channel — including `PATH`, which FR-050 requires be \
             compared. Its scope is now the `/env` field of that one legacy comparison; \
             the declarative channel keeps every variable and rewrites the only \
             irreconcilable one via `container_hostname_token`.",
        ),
        known_non_compliant: None,
    },
    NormalizationRule {
        name: "label_semantic",
        scopes: &["channel:chan-image"],
        action: RuleAction::Canonicalize,
        removes: &[],
        justification: Some(
            "Parses labels into a canonical key/value object so they compare semantically \
             rather than as opaque strings. Removes nothing.",
        ),
        known_non_compliant: None,
    },
    NormalizationRule {
        name: "mount_source_canonical",
        scopes: &["channel:chan-process-graph"],
        action: RuleAction::Canonicalize,
        removes: &[],
        justification: Some(
            "Path-substitutes each mount `source` before comparison so two mounts that \
             differ only by a temp workspace path compare equal. Never removes a mount.",
        ),
        known_non_compliant: None,
    },
    NormalizationRule {
        name: "null_preserving",
        scopes: &[
            "channel:chan-structured-output",
            "channel:chan-file-content",
            "channel:chan-container-state",
            "channel:chan-image",
            "channel:chan-injected-process",
            "channel:chan-temporal",
        ],
        action: RuleAction::Canonicalize,
        removes: &[],
        justification: Some(
            "Identity on the value. It is registered — and applied last in every chain — \
             so the preservation guarantee is explicit and auditable: missing, null, empty \
             and defaulted stay distinct, and only a named rule may ever elide a specific \
             field.",
        ),
        known_non_compliant: None,
    },
    NormalizationRule {
        name: "path_env_segmented",
        scopes: &["channel:chan-injected-process"],
        action: RuleAction::Segment,
        removes: &[],
        justification: Some(
            "Splits a PATH-like value into segments so it compares element-by-element \
             instead of as one string. Removes no segment.",
        ),
        known_non_compliant: None,
    },
    NormalizationRule {
        name: "path_token",
        scopes: &[
            "channel:chan-stdout",
            "channel:chan-stderr",
            "channel:chan-structured-output",
            "channel:chan-file-content",
            "channel:chan-filesystem",
            "channel:chan-container-state",
            "channel:chan-image",
            "channel:chan-process-graph",
            "channel:chan-injected-process",
        ],
        action: RuleAction::Rewrite,
        removes: &[],
        justification: Some(
            "Rewrites per-run temp workspace and project paths to stable tokens so two \
             runs in different temp directories compare equal. Rewrite, never delete.",
        ),
        known_non_compliant: None,
    },
    NormalizationRule {
        name: "user_default_root",
        scopes: &[
            "field:/user",
            "channel:chan-container-state",
            "field:/userSpec/name",
        ],
        action: RuleAction::Canonicalize,
        removes: &[],
        justification: Some(
            "An empty Docker `Config.User` means \"the image default\", which for every \
             Linux base this suite pins is root; treating \"\" and \"root\" as the same \
             effective identity keeps a cosmetic spelling difference out of the legacy \
             `diff_states` comparison while a real non-root `remoteUser`/`containerUser` \
             still diverges. Removes nothing, and applies to the legacy comparison only — \
             the declarative `chan-container-state` channel emits `user` verbatim plus the \
             derived `userSpec` — which is why the DECLARATIVE half of the rule is scoped \
             to those two fields on `chan-container-state`: measured at oracle 0.87.0 over \
             a Feature-extended image, deacon leaves `Config.User` empty while the \
             reference\'s generated Dockerfile writes `USER root`, and the two containers \
             run as the same user. Registered in 024 US5 (T123): it was an unregistered \
             comparison-time equivalence, invisible to anyone reading the rule list.",
        ),
        known_non_compliant: None,
    },
    NormalizationRule {
        name: "workspace_basename_token",
        scopes: &["channel:chan-container-state"],
        action: RuleAction::Rewrite,
        removes: &[],
        justification: Some(
            "Rewrites the workspace directory's BASENAME to `<WORKSPACE_NAME>` in \
             container-state evidence. Each side of a differential runs in its own \
             isolated temp workspace, and a config with no explicit `workspaceFolder` \
             derives the container path from that basename — `/workspaces/deacon-conf-aaa` \
             versus `/workspaces/deacon-conf-bbb`. The full-path substitution cannot reach \
             it because the container-side path never contains the host path. Without this \
             rule every container-state comparison would report a mount-destination \
             divergence that is an artifact of the isolation the runner itself imposes. \
             Rewrite, never delete; scoped to this one channel, so no other channel's \
             evidence changes meaning (024 Phase 4).",
        ),
        known_non_compliant: None,
    },
];

/// Scope spellings that are not a scope at all — a rule carrying one of these applies
/// everywhere, which is the blanket rule FR-021 exists to forbid.
const NON_SCOPES: &[&str] = &["all", "*", "any", "global", "everything", "-"];

/// **V24 — unscoped or unjustified normalization rule** (023 T060, FR-021).
///
/// A normalization rule decides what a comparison is allowed to ignore. Left
/// unconstrained it is the single most effective way to make a parity suite pass while
/// proving less, and — unlike a weakened assertion — it is invisible in the test data.
/// So each rule must say exactly where it applies and, if it removes anything, exactly
/// what and why.
///
/// Reports: an empty or `all`-style scope; a scope that is neither `channel:` nor
/// `field:` qualified; a `drop` with no justification or an empty `removes`; a `removes`
/// entry that is open-ended (a prefix, a glob, or a category predicate) rather than a
/// field name; a non-drop rule that nevertheless declares `removes`; and any rule
/// recorded as `known_non_compliant`.
pub fn check_normalization_rules(rules: &[NormalizationRule]) -> Vec<crate::validate::Violation> {
    let mut out = Vec::new();
    let mut push = |name: &str, message: String| {
        out.push(crate::validate::Violation::v24(name, message));
    };

    for rule in rules {
        // A DECLARED deficiency is tracked debt, reported by
        // `declared_non_compliant_rules` — not a V24 blocker (see the field docs). What
        // IS checked is that the declaration is honest: a non-empty reason naming a
        // tracked follow-up, so the field cannot silence the guard cheaply.
        //
        // The declaration excuses EXACTLY ONE property: an unbounded removal SET (an
        // empty or open-ended `removes` on a `drop` rule). That is the FR-021 deficiency
        // the field exists to admit. It does NOT excuse being unscoped, malformed, or
        // unjustified — those are recorded honestly by every rule, deficient or not.
        //
        // Skipping every structural check instead (an early `continue`) turned the field
        // into a general escape hatch: a rule with `scopes: &[]`, `removes: &[]` and no
        // justification passed V24 outright, on the strength of a `.md#` substring. That
        // admits a completely unscoped, unbounded, unexplained removal — the opposite of
        // "recorded honestly rather than dressed up".
        let declared_deficient = rule.known_non_compliant.is_some();
        if let Some(reason) = rule.known_non_compliant {
            if reason.trim().is_empty() || !mentions_tracked_reference(reason) {
                push(
                    rule.name,
                    "is declared `known_non_compliant` without a reason naming a tracked \
                     follow-up (an issue reference or a `specs/…#T123` anchor); an \
                     admitted deficiency must be queued, not parked"
                        .to_string(),
                );
            }
        }

        if rule.scopes.is_empty() {
            push(
                rule.name,
                "declares no scope — an unscoped rule applies everywhere, which is the \
                 blanket rule FR-021 forbids"
                    .to_string(),
            );
        }
        for scope in rule.scopes {
            let lowered = scope.trim().to_ascii_lowercase();
            if lowered.is_empty() || NON_SCOPES.contains(&lowered.as_str()) {
                push(
                    rule.name,
                    format!("scope {scope:?} is not a scope — it applies everywhere"),
                );
            } else if !(lowered.starts_with("channel:") || lowered.starts_with("field:")) {
                push(
                    rule.name,
                    format!(
                        "scope {scope:?} must be `channel:<chan-id>` or \
                         `field:<json-pointer>` (data-model §6)"
                    ),
                );
            }
        }

        match rule.action {
            RuleAction::Drop => {
                // The one property `known_non_compliant` is allowed to admit.
                if rule.removes.is_empty() && !declared_deficient {
                    push(
                        rule.name,
                        "a `drop` rule must enumerate the field names it removes; an \
                         empty list means an unbounded removal set"
                            .to_string(),
                    );
                }
                if rule.justification.is_none_or(|j| j.trim().is_empty()) {
                    push(
                        rule.name,
                        "a `drop` rule requires a justification naming the specific \
                         fields and why they are not observable behavior"
                            .to_string(),
                    );
                }
                for removed in rule.removes {
                    if declared_deficient {
                        break; // an open-ended entry IS the admitted deficiency
                    }
                    if let Some(reason) = open_ended_removal(removed) {
                        push(
                            rule.name,
                            format!(
                                "`removes` entry {removed:?} is open-ended ({reason}); a \
                                 removal set must be a finite list of field names, not a \
                                 category (FR-021)"
                            ),
                        );
                    }
                }
            }
            _ => {
                if !rule.removes.is_empty() {
                    push(
                        rule.name,
                        format!(
                            "action `{}` declares `removes` — only a `drop` removes \
                             anything",
                            rule.action.as_str()
                        ),
                    );
                }
            }
        }
    }
    out
}

/// The rules registered with a DECLARED FR-021 deficiency, name-sorted (T061).
///
/// Reported as tracked debt alongside the residual queue: visible, explained, and
/// pinned to a follow-up, but not a blocker. Narrowing or retiring one of these is real
/// work with a real acceptance criterion — pretending the rule is already compliant is
/// not.
pub fn declared_non_compliant_rules(
    rules: &[NormalizationRule],
) -> Vec<(&'static str, &'static str)> {
    let mut out: Vec<(&'static str, &'static str)> = rules
        .iter()
        .filter_map(|r| r.known_non_compliant.map(|reason| (r.name, reason)))
        .collect();
    out.sort_by_key(|(name, _)| *name);
    out
}

/// Whether a `known_non_compliant` reason names a tracked follow-up — an issue
/// reference, a URL, or a repository task anchor. Mirrors the residual `followUp`
/// discipline so "admitted" always means "queued".
fn mentions_tracked_reference(reason: &str) -> bool {
    reason.contains("http://")
        || reason.contains("https://")
        || reason.contains(".md#")
        || reason.split_whitespace().any(|w| {
            w.starts_with('#')
                && w.trim_start_matches('#')
                    .chars()
                    .any(|c| c.is_ascii_digit())
        })
}

/// Why a `removes` entry is open-ended, or `None` when it names one field.
fn open_ended_removal(entry: &str) -> Option<&'static str> {
    let trimmed = entry.trim();
    if trimmed.is_empty() {
        return Some("empty");
    }
    if trimmed.contains('*') || trimmed.contains('?') {
        return Some("a glob");
    }
    if trimmed.ends_with('.') || trimmed.ends_with('-') || trimmed.ends_with('_') {
        return Some("a prefix");
    }
    const CATEGORIES: &[&str] = &[
        "every", "any", "all", "empty", "null", "noise", "dynamic", "*",
    ];
    let lowered = trimmed.to_ascii_lowercase();
    if CATEGORIES.contains(&lowered.as_str()) {
        return Some("a category predicate");
    }
    None
}

#[cfg(test)]
mod rule_tests {
    use super::*;

    fn compliant() -> NormalizationRule {
        NormalizationRule {
            name: "probe",
            scopes: &["channel:chan-stdout"],
            action: RuleAction::Rewrite,
            removes: &[],
            justification: Some("probe"),
            known_non_compliant: None,
        }
    }

    #[test]
    fn a_compliant_rule_passes() {
        assert!(check_normalization_rules(&[compliant()]).is_empty());
    }

    #[test]
    fn the_registry_is_name_sorted_and_unique() {
        let names: Vec<&str> = NORMALIZATION_RULES.iter().map(|r| r.name).collect();
        let mut sorted = names.clone();
        sorted.sort_unstable();
        assert_eq!(names, sorted, "the rule registry must be name-sorted");
        let unique: BTreeSet<&&str> = names.iter().collect();
        assert_eq!(unique.len(), names.len(), "rule names must be unique");
    }
}

// ---------------------------------------------------------------------------
// The no-coverage-loss report (T070–T075, contracts/migration-report.md)
// ---------------------------------------------------------------------------

use serde::Serialize;

/// The pre-migration totals (contracts/migration-report.md `totals.before`).
///
/// **Two documented additions to the contract's shape**, both forced by real state the
/// contract predates:
///
/// - `unitsAtBranchPoint` — the contract's example writes `units: 111`, the executable
///   count when it was authored. User Story 4 (T055/T056) then ADDED 7 fault-injection
///   guard units to the frozen baseline, so `units` is now 118. Both numbers matter:
///   `units` is what the accounting must sum to (failure condition 5), while
///   `unitsAtBranchPoint` keeps SC-005's original denominator auditable. Reporting only
///   one would either hide the growth or lose the benchmark.
/// - `recordedOnlyUnits` — the 33 `external-corpus-entry` entries. They are inventoried
///   but never counted as migrated (research D8), so they are excluded from `units` and
///   from the accounting counters, and reported here instead of vanishing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BeforeTotals {
    /// Executable baseline units — the accounting denominator.
    pub units: usize,
    /// Executable units at the migration's branch point (research §1).
    pub units_at_branch_point: usize,
    /// Recorded-only `external-corpus-entry` units (research D8).
    pub recorded_only_units: usize,
    pub behaviors: usize,
    pub channels: usize,
    pub fixtures: usize,
    pub exceptions: usize,
}

/// The post-migration totals (contracts/migration-report.md `totals.after`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AfterTotals {
    pub cases: usize,
    pub variants: usize,
    pub behaviors: usize,
    pub channels: usize,
    pub fixtures: usize,
    pub exceptions: usize,
}

/// Before-and-after totals.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Totals {
    pub before: BeforeTotals,
    pub after: AfterTotals,
}

/// A baseline unit with no disposition — named with its origin and what it asserted, so
/// the failure is actionable without opening the baseline (failure condition 1).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UnaccountedUnit {
    pub unit: String,
    pub program: String,
    pub assertion: String,
}

/// Disposition counts plus the unaccounted list.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Accounting {
    pub migrated: usize,
    pub deduplicated: usize,
    pub residual: usize,
    pub retired: usize,
    pub unaccounted: Vec<UnaccountedUnit>,
}

/// What a rejection lost on its way across the migration (failure condition 3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ErrorPathLoss {
    /// The counterpart no longer asserts that the operation is REJECTED — it would pass
    /// if deacon started accepting the input.
    Direction,
    /// The counterpart no longer asserts anything about the diagnostic, so a rejection
    /// with a useless message would pass.
    Diagnostic,
}

impl ErrorPathLoss {
    /// The wire spelling, matching contracts/migration-report.md's `direction|diagnostic`.
    pub fn as_str(self) -> &'static str {
        match self {
            ErrorPathLoss::Direction => "direction",
            ErrorPathLoss::Diagnostic => "diagnostic",
        }
    }
}

/// One weakened error path.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WeakenedErrorPath {
    pub unit: String,
    pub lost: ErrorPathLoss,
    /// The destination cases inspected, so the fix site is obvious.
    pub cases: Vec<String>,
}

/// Error-path accounting (FR-042).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ErrorPaths {
    pub before: usize,
    pub preserved: usize,
    pub weakened: Vec<WeakenedErrorPath>,
}

/// One deduplication: several units absorbed into the cases of one behavior.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Deduplication {
    pub behavior: String,
    pub absorbed_units: Vec<String>,
    pub cases: Vec<String>,
    pub rationale: String,
}

/// One residual, with the carrier it pins (T075 — feeds the US7 deletion predicate).
///
/// `followUp` and `outOfScopeRationale` are mutually exclusive and exactly one is present,
/// per the residual's disposition (024 P1): queued work is tracked, a permanent exclusion
/// is justified.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResidualEntry {
    pub id: String,
    pub units: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub blocked_carrier: Option<String>,
    pub missing_capability: String,
    /// Whether this residual is still queued work or a permanent exclusion (024 P1).
    pub disposition: crate::residual::ResidualDisposition,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub follow_up: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub out_of_scope_rationale: Option<String>,
}

/// One deliberately dropped unit (failure condition 6 requires the rationale).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RetiredEntry {
    pub unit: String,
    pub rationale: String,
}

/// One difference the replacement detects that the superseded path did not (FR-036).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StrictnessImprovement {
    pub unit: String,
    pub detail: String,
    /// A `case-…` / `wvr-…` / issue reference. Absent ⇒ failure condition 7: the
    /// improvement was suppressed rather than characterized.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub characterized_as: Option<String>,
}

/// Why a carrier is not yet deletable (T075). **A documented addition to the contract's
/// shape**: `deletableCarriers` alone says which carriers may go, but not why the others
/// may not — and "why" is the actionable half for User Story 7, which consumes this.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeletionBlocker {
    pub carrier: String,
    pub reason: String,
    /// True when EVERY residual pinning this carrier is a permanent exclusion (024 P1), so
    /// the carrier is a settled fixture of the repository rather than outstanding work.
    ///
    /// Without this distinction the blocker list reads as a queue: a reviewer seeing nine
    /// blocked carriers reasonably infers nine pending deletions, when four of them are
    /// pinned by units that can never be expressed as data. Same defect as folding permanent
    /// residuals into `residualQueue`, one level up.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub permanent: bool,
}

/// One report-level failure, naming the specific item and its category.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReportViolation {
    /// The failure-condition number from contracts/migration-report.md (1–8).
    pub condition: u8,
    /// The specific item at fault — never an aggregate.
    pub item: String,
    /// A precise diagnosis.
    pub message: String,
}

/// The no-coverage-loss report (contracts/migration-report.md).
///
/// Deterministic by construction: no timestamps, no absolute paths, no hostnames, every
/// array sorted by a stable key.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MigrationReport {
    pub schema_version: u32,
    pub baseline_revision: String,
    pub totals: Totals,
    pub accounting: Accounting,
    pub error_paths: ErrorPaths,
    pub deduplication: Vec<Deduplication>,
    pub residual_queue: Vec<ResidualEntry>,
    pub retired: Vec<RetiredEntry>,
    pub strictness_improvements: Vec<StrictnessImprovement>,
    /// Baseline carriers whose source file is already gone — the deletions this migration
    /// has completed. **A documented addition to the contract's shape.** The contract has
    /// `deletableCarriers` (may go) and, by this file's addition, `deletionBlockers` (may
    /// not, and why); neither can express "already went", because a deleted carrier is
    /// absent from the live registry and so drops out of both lists. Without this field
    /// the report renders "No carrier is deletable yet." over a diff that removes four
    /// carriers and fifty files — technically true of the survivors and thoroughly
    /// misleading about the change.
    pub deleted_carriers: Vec<String>,
    pub deletable_carriers: Vec<String>,
    pub deletion_blockers: Vec<DeletionBlocker>,
    pub violations: Vec<ReportViolation>,
}

impl MigrationReport {
    /// Whether every baseline item is accounted for — the `migration check` gate.
    pub fn is_clean(&self) -> bool {
        self.violations.is_empty()
    }
}

/// The report's schema version.
pub const REPORT_SCHEMA_VERSION: u32 = 1;

/// Why a report could not be computed at all. Distinguished from a report WITH
/// violations: an absent baseline means the accounting was never performed, and
/// reporting "0 unaccounted" for it would be the silent pass FR-023 forbids.
#[derive(Debug, thiserror::Error)]
pub enum ReportError {
    #[error(
        "no committed baseline — run `baseline generate --freeze <sha>` first. Without \
         it there is nothing to measure conservation against, and an empty accounting \
         would read as a clean report."
    )]
    NoBaseline,
}

/// Compute the no-coverage-loss report over a loaded registry (T070–T075).
///
/// Pure over the registry plus `fixtures_root` (counted, never read for content) and an
/// optional equivalence ledger. Never writes; never reads the baseline for anything but
/// comparison (FR-045).
pub fn migration_report(
    registry: &Registry,
    fixtures_root: &std::path::Path,
    ledger: Option<&EquivalenceFacts>,
) -> Result<MigrationReport, ReportError> {
    let baseline = registry.baseline.as_ref().ok_or(ReportError::NoBaseline)?;

    let units: BTreeMap<&str, &crate::baseline::BaselineUnit> = baseline
        .records
        .iter()
        .map(|u| (u.id.as_str(), u))
        .collect();
    let executable: BTreeSet<&str> = baseline
        .records
        .iter()
        .filter(|u| u.category != crate::baseline::UnitCategory::ExternalCorpusEntry)
        .map(|u| u.id.as_str())
        .collect();
    let recorded_only = baseline.records.len() - executable.len();

    let cases: BTreeMap<&str, &crate::model::TestCase> =
        registry.cases.iter().map(|c| (c.id.as_str(), c)).collect();
    let mapping: BTreeMap<&str, &crate::mapping::MigrationMapping> = registry
        .mapping
        .iter()
        .map(|m| (m.unit.as_str(), m))
        .collect();

    let mut violations: Vec<ReportViolation> = Vec::new();

    // -- Accounting (failure conditions 1 and 5) ---------------------------
    let mut accounting = Accounting {
        migrated: 0,
        deduplicated: 0,
        residual: 0,
        retired: 0,
        unaccounted: Vec::new(),
    };
    for unit_id in &executable {
        match mapping.get(unit_id).map(|m| m.disposition) {
            Some(Disposition::Migrated) => accounting.migrated += 1,
            Some(Disposition::Deduplicated) => accounting.deduplicated += 1,
            Some(Disposition::Residual) => accounting.residual += 1,
            Some(Disposition::Retired) => accounting.retired += 1,
            None => {
                let unit = units.get(unit_id);
                accounting.unaccounted.push(UnaccountedUnit {
                    unit: (*unit_id).to_string(),
                    program: unit.map(|u| u.program.clone()).unwrap_or_default(),
                    assertion: unit.map(|u| u.assertion.clone()).unwrap_or_default(),
                });
            }
        }
    }
    accounting.unaccounted.sort_by(|a, b| a.unit.cmp(&b.unit));
    for item in &accounting.unaccounted {
        violations.push(ReportViolation {
            condition: 1,
            item: item.unit.clone(),
            message: format!(
                "unaccounted: `{}` from `{}` asserted \"{}\" and has no disposition",
                item.unit, item.program, item.assertion
            ),
        });
    }

    // Every RECORDED-ONLY unit must also carry a disposition — it is excluded from the
    // accounting counters (D8), not from the requirement to be accounted for.
    for unit in &baseline.records {
        if unit.category == crate::baseline::UnitCategory::ExternalCorpusEntry
            && !mapping.contains_key(unit.id.as_str())
        {
            violations.push(ReportViolation {
                condition: 1,
                item: unit.id.clone(),
                message: format!(
                    "unaccounted: recorded-only entry `{}` has no disposition; it is \
                     excluded from the accounting counters, not from being accounted for",
                    unit.id
                ),
            });
        }
    }

    let disposition_sum =
        accounting.migrated + accounting.deduplicated + accounting.residual + accounting.retired;
    if disposition_sum != executable.len() {
        violations.push(ReportViolation {
            condition: 5,
            item: "accounting".to_string(),
            message: format!(
                "dispositions sum to {disposition_sum} but there are {} executable \
                 baseline units — every unit needs exactly one disposition",
                executable.len()
            ),
        });
    }

    // -- Totals ------------------------------------------------------------
    let counts = denominator_counts(registry);
    let before_fixtures = before_fixture_set(baseline);
    let totals = Totals {
        before: BeforeTotals {
            units: executable.len(),
            units_at_branch_point: PRE_MIGRATION_EXECUTABLE_UNITS,
            recorded_only_units: recorded_only,
            behaviors: PRE_MIGRATION_BEHAVIORS,
            channels: PRE_MIGRATION_CHANNELS,
            fixtures: before_fixtures.len(),
            exceptions: PRE_MIGRATION_EXCEPTIONS,
        },
        after: AfterTotals {
            cases: counts.cases,
            variants: counts.variants,
            behaviors: counts.behaviors,
            channels: registry.channels.len(),
            fixtures: count_fixture_dirs(fixtures_root),
            exceptions: registry.waivers.len() + registry.extensions.len(),
        },
    };

    // Failure condition 4: denominator inflation.
    //
    // The ceiling is the frozen `before` count PLUS the behaviors explicitly accounted in
    // `POST_BRANCH_BEHAVIORS` — newly OBSERVED facts, each naming why it is not a variant.
    // Using the bare `before` count here would contradict `denominator_inflated()`, which
    // is the same rule enforced from the registry side; two copies of one threshold that
    // disagree is worse than either.
    let allowance = POST_BRANCH_BEHAVIORS.len();
    if totals.after.behaviors > totals.before.behaviors + allowance {
        violations.push(ReportViolation {
            condition: 4,
            item: "totals.after.behaviors".to_string(),
            message: format!(
                "the behavior denominator grew from {} to {}, beyond the {allowance} \
                 explicitly accounted post-branch behavior(s) — a variant was authored as \
                 a new behavior (SC-005)",
                totals.before.behaviors, totals.after.behaviors
            ),
        });
    }

    // An allowance whose behavior no longer exists silently raises the ceiling forever.
    for stale in stale_post_branch_behaviors(registry) {
        violations.push(ReportViolation {
            condition: 4,
            item: stale.to_string(),
            message: format!(
                "POST_BRANCH_BEHAVIORS accounts for `{stale}`, which no longer resolves in \
                 the registry — a stale allowance raises the denominator ceiling for a \
                 behavior that is gone"
            ),
        });
    }

    // -- Failure condition 2: a before-item with no counterpart ------------
    violations.extend(missing_counterparts(registry, baseline, &cases, &mapping));

    // -- Error paths (failure condition 3, T072) ---------------------------
    let error_paths = error_path_accounting(baseline, &cases, &mapping);
    for weakened in &error_paths.weakened {
        violations.push(ReportViolation {
            condition: 3,
            item: weakened.unit.clone(),
            message: format!(
                "error path weakened: `{}` lost its {} expectation; destination case(s) {:?}",
                weakened.unit,
                weakened.lost.as_str(),
                weakened.cases
            ),
        });
    }

    // -- Deduplications, residual queue, retirements -----------------------
    let deduplication = deduplications(registry, &cases);
    let residual_queue = residual_entries(registry);
    let retired = retirements(registry, &mut violations);

    // -- Strictness improvements (failure condition 7) ---------------------
    let strictness_improvements = ledger.map(strictness_from_ledger).unwrap_or_default();
    for improvement in &strictness_improvements {
        if improvement
            .characterized_as
            .as_ref()
            .is_none_or(|c| c.trim().is_empty())
        {
            violations.push(ReportViolation {
                condition: 7,
                item: improvement.unit.clone(),
                message: format!(
                    "strictness improvement on `{}` has no `characterizedAs` — a newly \
                     detected difference must be characterized, never suppressed (FR-036)",
                    improvement.unit
                ),
            });
        }
    }

    // -- Deletion linkage (T075) -------------------------------------------
    let (deleted_carriers, deletable_carriers, deletion_blockers) =
        deletion_status(baseline, registry, ledger);

    violations.sort_by(|a, b| {
        a.condition
            .cmp(&b.condition)
            .then_with(|| a.item.cmp(&b.item))
            .then_with(|| a.message.cmp(&b.message))
    });

    Ok(MigrationReport {
        schema_version: REPORT_SCHEMA_VERSION,
        baseline_revision: baseline.revision.clone(),
        totals,
        accounting,
        error_paths,
        deduplication,
        residual_queue,
        retired,
        strictness_improvements,
        deleted_carriers,
        deletable_carriers,
        deletion_blockers,
        violations,
    })
}

/// The distinct fixtures the pre-migration units consumed (repo-relative dirs and
/// `inline:` code-authored workspaces alike).
fn before_fixture_set(baseline: &crate::baseline::BaselineFile) -> BTreeSet<&str> {
    baseline
        .records
        .iter()
        .flat_map(|u| u.fixtures.iter().map(String::as_str))
        .collect()
}

/// Count the committed fixture directories under `fixtures_root`. A missing directory
/// counts zero rather than failing — the count is a total, not a gate.
fn count_fixture_dirs(fixtures_root: &std::path::Path) -> usize {
    match std::fs::read_dir(fixtures_root) {
        Ok(entries) => entries
            .filter_map(Result::ok)
            .filter(|e| e.path().is_dir())
            .count(),
        Err(_) => 0,
    }
}

/// **Failure condition 2**: a before-behavior, channel, fixture or characterized
/// exception with no counterpart.
///
/// The baseline does not enumerate behavior ids, so the pre-migration behavior set is
/// derived from the LEGACY (binary-backed) pointer cases — those records ARE the
/// pre-migration coverage claim. Channels and fixtures come straight off the baseline
/// units, and exceptions off the registry's waivers/extensions.
fn missing_counterparts(
    registry: &Registry,
    baseline: &crate::baseline::BaselineFile,
    cases: &BTreeMap<&str, &crate::model::TestCase>,
    mapping: &BTreeMap<&str, &crate::mapping::MigrationMapping>,
) -> Vec<ReportViolation> {
    let mut out = Vec::new();

    // Behaviors: every behavior a legacy pointer case claimed must still exist AND still
    // be reached by at least one case.
    let known_behaviors: BTreeSet<&str> =
        registry.behaviors.iter().map(|b| b.id.as_str()).collect();
    let reached: BTreeSet<&str> = registry
        .cases
        .iter()
        .flat_map(|c| c.behaviors.iter().map(String::as_str))
        .collect();
    // The behaviors that existed BEFORE the migration.
    //
    // Deriving this from the surviving legacy cases alone makes the check erase itself:
    // as each superseded carrier is deleted — the stated goal — its pointer case goes with
    // it, the set shrinks, and once the last one is gone the loop below never runs and
    // condition 2 passes vacuously for every behavior. The "before" side would be read
    // from mutable present state, in a module whose whole discipline is measuring against
    // a frozen record.
    //
    // `baseline.json` cannot supply it (a `BaselineUnit` records channels and fixtures,
    // not behaviors), and fabricating a frozen list now — four carriers into the deletion
    // — would be recording a subset and calling it the whole. What IS durable is
    // `mapping.json`: a migrated unit's entry permanently names the case that took over,
    // and that case claims the behaviors the unit's carrier used to claim. So the union of
    // (still-live legacy cases) and (every mapping destination's behaviors) is stable
    // under carrier deletion — deleting a carrier moves entries between the two halves
    // rather than dropping them. A destination id that stops resolving is already V21.
    let mapping_destinations: BTreeSet<&str> = registry
        .mapping
        .iter()
        .flat_map(|m| m.case_ids.iter().map(String::as_str))
        .collect();
    let before_behaviors: BTreeSet<&str> = registry
        .cases
        .iter()
        .filter(|c| {
            matches!(c.classify(), Ok(CaseKind::Legacy))
                || mapping_destinations.contains(c.id.as_str())
        })
        .flat_map(|c| c.behaviors.iter().map(String::as_str))
        .collect();
    for behavior in &before_behaviors {
        if !known_behaviors.contains(behavior) {
            out.push(ReportViolation {
                condition: 2,
                item: (*behavior).to_string(),
                message: format!("behavior `{behavior}` existed before the migration and is gone"),
            });
        } else if !reached.contains(behavior) {
            out.push(ReportViolation {
                condition: 2,
                item: (*behavior).to_string(),
                message: format!("behavior `{behavior}` is reached by no case after the migration"),
            });
        }
    }

    // Channels: every channel a baseline unit inspected must still be declared.
    let known_channels: BTreeSet<&str> = registry.channels.iter().map(|c| c.id.as_str()).collect();
    let mut seen_channels: BTreeSet<&str> = BTreeSet::new();
    for unit in &baseline.records {
        for channel in &unit.channels {
            if seen_channels.insert(channel.as_str()) && !known_channels.contains(channel.as_str())
            {
                out.push(ReportViolation {
                    condition: 2,
                    item: channel.clone(),
                    message: format!(
                        "observable channel `{channel}`, inspected by baseline unit `{}`, is \
                         no longer declared",
                        unit.id
                    ),
                });
            }
        }
    }

    // Fixtures: every fixture a MIGRATED unit consumed needs a `fixtureMapping`
    // counterpart. A residual/retired unit's fixtures stay with its carrier.
    for unit in &baseline.records {
        let Some(entry) = mapping.get(unit.id.as_str()) else {
            continue; // already reported as unaccounted
        };
        if !entry.disposition.requires_cases() {
            continue;
        }
        let mapped: BTreeSet<&str> = entry
            .fixture_mapping
            .iter()
            .map(|fm| fm.from.as_str())
            .collect();
        for fixture in &unit.fixtures {
            if !mapped.contains(fixture.as_str()) {
                out.push(ReportViolation {
                    condition: 2,
                    item: fixture.clone(),
                    message: format!(
                        "fixture `{fixture}`, consumed by migrated unit `{}`, has no \
                         counterpart in the migration",
                        unit.id
                    ),
                });
            }
        }
    }

    // Exceptions: every characterized exception needs a mapping entry naming a mechanism.
    let mapped_exceptions: BTreeMap<&str, &crate::mapping::ExceptionMapping> = registry
        .mapping_exceptions
        .iter()
        .map(|e| (e.exception.as_str(), e))
        .collect();
    // POST-branch exceptions are exempt, by the same DERIVED rule `validate.rs` uses: an
    // exception every one of whose behaviors is accounted in `POST_BRANCH_BEHAVIORS`
    // characterizes something the pre-migration system never knew, so it has no
    // pre-migration counterpart to record. Requiring one would be unsatisfiable, and the
    // only ways to satisfy it would be to invent an entry or not record the divergence.
    let post_branch = post_branch_exceptions(registry);
    let known_exceptions: Vec<&str> = registry
        .waivers
        .iter()
        .map(|w| w.id.as_str())
        .chain(registry.extensions.iter().map(|e| e.id.as_str()))
        .filter(|id| !post_branch.contains(*id))
        .collect();
    for exception in known_exceptions {
        match mapped_exceptions.get(exception) {
            None => out.push(ReportViolation {
                condition: 2,
                item: exception.to_string(),
                message: format!(
                    "characterized exception `{exception}` has no counterpart recorded in \
                     the migration mapping"
                ),
            }),
            Some(entry)
                if entry.disposition == crate::mapping::ExceptionDisposition::Preserved
                    && entry.mechanisms.is_empty() =>
            {
                out.push(ReportViolation {
                    condition: 2,
                    item: exception.to_string(),
                    message: format!(
                        "characterized exception `{exception}` is preserved by no mechanism"
                    ),
                });
            }
            Some(_) => {}
        }
    }

    // Cases named by the mapping must exist (the inverse of unaccounted).
    for entry in mapping.values() {
        for case_id in &entry.case_ids {
            if !cases.contains_key(case_id.as_str()) {
                out.push(ReportViolation {
                    condition: 2,
                    item: case_id.clone(),
                    message: format!(
                        "unit `{}` names destination case `{case_id}`, which does not exist",
                        entry.unit
                    ),
                });
            }
        }
    }

    out.sort_by(|a, b| a.item.cmp(&b.item).then_with(|| a.message.cmp(&b.message)));
    out.dedup();
    out
}

/// **T072 / failure condition 3**: error-path direction and diagnostic preservation.
///
/// A baseline unit with `errorPath: true` asserted a REJECTION. Its counterpart must
/// still assert two things:
///
/// - **direction** — that the operation is rejected. A case that merely compares deacon's
///   exit code to the reference's does NOT: it passes just as happily if both CLIs start
///   accepting the input, which is precisely the rejection quietly evaporating.
/// - **diagnostic** — that the rejection says something. Without it, a rejection with a
///   useless message passes.
///
/// A `residual` (or `retired`-with-rationale) unit is preserved by its carrier, which
/// still runs and still asserts both — the coverage did not move, so there is nothing to
/// weaken.
fn error_path_accounting(
    baseline: &crate::baseline::BaselineFile,
    cases: &BTreeMap<&str, &crate::model::TestCase>,
    mapping: &BTreeMap<&str, &crate::mapping::MigrationMapping>,
) -> ErrorPaths {
    let mut before = 0usize;
    let mut preserved = 0usize;
    let mut weakened: Vec<WeakenedErrorPath> = Vec::new();

    for unit in &baseline.records {
        if !unit.error_path {
            continue;
        }
        before += 1;
        let Some(entry) = mapping.get(unit.id.as_str()) else {
            continue; // unaccounted — reported under condition 1
        };
        if !entry.disposition.requires_cases() {
            // The carrier still asserts it; the rejection did not move.
            preserved += 1;
            continue;
        }

        let destinations: Vec<&crate::model::TestCase> = entry
            .case_ids
            .iter()
            .filter_map(|id| cases.get(id.as_str()).copied())
            .collect();
        let case_ids: Vec<String> = entry.case_ids.clone();

        // DIRECTION is preserved when some destination case PINS deacon's own decision
        // with a concrete exit-code assertion (or declares the phase it fails in). A bare
        // live-differential does not: it asserts only "the same exit code as the
        // reference", so it passes just as happily once BOTH CLIs start accepting the
        // input — the rejection evaporates without a single test turning red.
        let direction = destinations.iter().any(|c| pins_decision(c));

        // DIAGNOSTIC is required only when DEACON is the rejecting side. When the case
        // pins deacon at success, the reference is the rejecting side and the diagnostic
        // is the reference's — characterized by the preserved `wvr-` exception, whose
        // direction V21/T045 already forbids widening. Demanding a deacon stderr
        // assertion there would demand a diagnostic deacon never emits.
        let deacon_rejects = destinations.iter().any(|c| asserts_rejection(c));
        let diagnostic = !deacon_rejects || destinations.iter().any(|c| asserts_diagnostic(c));

        match (direction, diagnostic) {
            (true, true) => preserved += 1,
            (false, _) => weakened.push(WeakenedErrorPath {
                unit: unit.id.clone(),
                lost: ErrorPathLoss::Direction,
                cases: case_ids,
            }),
            (true, false) => weakened.push(WeakenedErrorPath {
                unit: unit.id.clone(),
                lost: ErrorPathLoss::Diagnostic,
                cases: case_ids,
            }),
        }
    }

    weakened.sort_by(|a, b| a.unit.cmp(&b.unit));
    ErrorPaths {
        before,
        preserved,
        weakened,
    }
}

/// Whether a case PINS deacon's own decision — a concrete exit-code assertion (of any
/// value) or an operation declaring the phase it is expected to fail in.
///
/// This is the direction test. A live-differential expectation with no assertion pins
/// nothing: it compares deacon's exit code to the reference's, which is agreement, not a
/// decision.
fn pins_decision(case: &crate::model::TestCase) -> bool {
    if case
        .operations
        .iter()
        .any(|op| op.expect_failure_phase.is_some())
    {
        return true;
    }
    case.expected.iter().any(|exp| {
        exp.channel == crate::model::CHAN_EXIT_CODE
            && exp.assertion.as_ref().is_some_and(|a| {
                a.get("nonZero").and_then(serde_json::Value::as_bool) == Some(true)
                    || a.get("equals")
                        .and_then(serde_json::Value::as_i64)
                        .is_some()
            })
    })
}

/// Whether a case asserts that DEACON rejects: a non-zero exit-code expectation, or an
/// operation declaring the phase it is expected to fail in.
fn asserts_rejection(case: &crate::model::TestCase) -> bool {
    if case
        .operations
        .iter()
        .any(|op| op.expect_failure_phase.is_some())
    {
        return true;
    }
    case.expected.iter().any(|exp| {
        exp.channel == crate::model::CHAN_EXIT_CODE
            && exp.assertion.as_ref().is_some_and(|a| {
                a.get("nonZero").and_then(serde_json::Value::as_bool) == Some(true)
                    || a.get("equals")
                        .and_then(serde_json::Value::as_i64)
                        .is_some_and(|code| code != 0)
            })
    })
}

/// Whether a case asserts something about the rejection's DIAGNOSTIC: a `contains` or
/// `matches` expectation on stderr.
fn asserts_diagnostic(case: &crate::model::TestCase) -> bool {
    case.expected.iter().any(|exp| {
        exp.channel == crate::model::CHAN_STDERR
            && exp
                .assertion
                .as_ref()
                .is_some_and(|a| a.get("contains").is_some() || a.get("matches").is_some())
    })
}

/// The units, cases and rationales one behavior absorbed — the accumulator
/// [`deduplications`] folds into a [`Deduplication`].
type DeduplicationSlot = (BTreeSet<String>, BTreeSet<String>, Vec<String>);

/// Deduplications, grouped by the behavior that absorbed them.
fn deduplications(
    registry: &Registry,
    cases: &BTreeMap<&str, &crate::model::TestCase>,
) -> Vec<Deduplication> {
    let mut by_behavior: BTreeMap<String, DeduplicationSlot> = BTreeMap::new();
    for entry in &registry.mapping {
        if entry.disposition != Disposition::Deduplicated {
            continue;
        }
        for case_id in &entry.case_ids {
            let behaviors = cases
                .get(case_id.as_str())
                .map(|c| c.behaviors.clone())
                .unwrap_or_default();
            for behavior in behaviors {
                let slot = by_behavior.entry(behavior).or_default();
                slot.0.insert(entry.unit.clone());
                slot.1.insert(case_id.clone());
                if let Some(rationale) = &entry.rationale {
                    slot.2.push(rationale.clone());
                }
            }
        }
    }
    by_behavior
        .into_iter()
        .map(|(behavior, (units, cases, rationales))| Deduplication {
            behavior,
            absorbed_units: units.into_iter().collect(),
            cases: cases.into_iter().collect(),
            rationale: rationales.join(" "),
        })
        .collect()
}

/// The residual queue with its blocked carriers (T075).
fn residual_entries(registry: &Registry) -> Vec<ResidualEntry> {
    let mut out: Vec<ResidualEntry> = registry
        .residuals
        .iter()
        .map(|r| ResidualEntry {
            id: r.id.clone(),
            units: {
                let mut units = r.units.clone();
                units.sort();
                units
            },
            blocked_carrier: r.blocked_carrier.clone(),
            missing_capability: r.missing_capability.clone(),
            disposition: r.disposition,
            follow_up: r.follow_up.clone(),
            out_of_scope_rationale: r.out_of_scope_rationale.clone(),
        })
        .collect();
    out.sort_by(|a, b| a.id.cmp(&b.id));
    out
}

/// Deliberately dropped units. **Failure condition 6**: a retirement without a rationale
/// is an unexplained loss.
fn retirements(registry: &Registry, violations: &mut Vec<ReportViolation>) -> Vec<RetiredEntry> {
    let mut out: Vec<RetiredEntry> = Vec::new();
    for entry in &registry.mapping {
        if entry.disposition != Disposition::Retired {
            continue;
        }
        let rationale = entry.rationale.clone().unwrap_or_default();
        if rationale.trim().is_empty() {
            violations.push(ReportViolation {
                condition: 6,
                item: entry.unit.clone(),
                message: format!(
                    "unit `{}` is retired with no rationale — a deliberate loss must be \
                     explained, never implied by a total",
                    entry.unit
                ),
            });
        }
        out.push(RetiredEntry {
            unit: entry.unit.clone(),
            rationale,
        });
    }
    out.sort_by(|a, b| a.unit.cmp(&b.unit));
    out
}

/// The equivalence-ledger facts the report consumes (T075, contracts/equivalence-ledger.md).
///
/// The ledger itself is produced live by `parity-harness` under the parity profile and is
/// git-ignored; this crate is hermetic, so it takes the already-parsed facts rather than
/// reaching for a file that may not exist. `None` means the ledger has not been produced
/// — which is NOT the same as "every unit is equivalent", and is why no carrier can be
/// declared deletable without one.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EquivalenceFacts {
    /// Units whose replacement is `equivalent` or `stricter` — deletion-permitting.
    pub cleared: BTreeSet<String>,
    /// Units whose replacement is `more-permissive` — deletion-blocking (FR-035).
    pub more_permissive: BTreeSet<String>,
    /// `stricter` units and the newly detected difference, for
    /// `strictnessImprovements`: `(unit, detail, characterizedAs)`.
    pub stricter: Vec<(String, String, Option<String>)>,
}

/// Lift the ledger's `stricter` relations into report entries (FR-036).
fn strictness_from_ledger(facts: &EquivalenceFacts) -> Vec<StrictnessImprovement> {
    let mut out: Vec<StrictnessImprovement> = facts
        .stricter
        .iter()
        .map(|(unit, detail, characterized_as)| StrictnessImprovement {
            unit: unit.clone(),
            detail: detail.clone(),
            characterized_as: characterized_as.clone(),
        })
        .collect();
    out.sort_by(|a, b| a.unit.cmp(&b.unit));
    out
}

/// The program every migrated case runs on. It is the migration's destination, so it is
/// never a deletion candidate however its own units are judged.
pub const SURVIVING_RUNNER: &str = "parity_conformance_runner";

/// The behaviors that ONLY this carrier's legacy cases evidence — i.e. behaviors that
/// would become uncovered the moment the carrier is deleted.
fn behaviors_only_this_carrier_evidences(
    carrier: &str,
    registry: &Registry,
) -> Option<Vec<String>> {
    let mut orphaned: Vec<String> = Vec::new();
    for case in &registry.cases {
        let backed_by_carrier = case
            .executable
            .as_ref()
            .is_some_and(|e| e.binary == carrier);
        if !backed_by_carrier {
            continue;
        }
        for behavior in &case.behaviors {
            let covered_elsewhere = registry.cases.iter().any(|other| {
                other.id != case.id
                    && other.behaviors.contains(behavior)
                    && !other
                        .executable
                        .as_ref()
                        .is_some_and(|e| e.binary == carrier)
            });
            if !covered_elsewhere && !orphaned.contains(behavior) {
                orphaned.push(behavior.clone());
            }
        }
    }
    orphaned.sort();
    (!orphaned.is_empty()).then_some(orphaned)
}

/// Whether a carrier's test source still exists anywhere under `crates/*/tests/`.
/// Mirrors V1's resolution, so "retired" here means the same thing it means to the
/// validator.
fn carrier_source_exists(carrier: &str) -> bool {
    let crates_dir = crate::workspace_root().join("crates");
    let Ok(entries) = std::fs::read_dir(&crates_dir) else {
        return false;
    };
    let file_name = format!("{carrier}.rs");
    entries
        .flatten()
        .any(|entry| entry.path().join("tests").join(&file_name).is_file())
}

/// **T075** — the deletion predicate's inputs: which carriers may be deleted, and for each
/// that may not, exactly why.
///
/// A carrier is deletable iff every unit it carries is cleared by the equivalence ledger
/// **and** no residual names it as `blockedCarrier` (FR-035, FR-038). Absent a ledger,
/// nothing is deletable — "we have not checked" is not "it is fine", and treating it as
/// such is how a more-permissive replacement gets deleted into.
fn deletion_status(
    baseline: &crate::baseline::BaselineFile,
    registry: &Registry,
    ledger: Option<&EquivalenceFacts>,
) -> (Vec<String>, Vec<String>, Vec<DeletionBlocker>) {
    // Carriers = the programs of the EXECUTABLE units. The recorded-only `realworld`
    // pseudo-program is not a carrier: no program runs it (research D8).
    let mut by_carrier: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for unit in &baseline.records {
        if unit.category == crate::baseline::UnitCategory::ExternalCorpusEntry {
            continue;
        }
        // The declarative runner is the migration's DESTINATION, not a superseded
        // carrier. Listing it as deletable-or-blocked would invite deleting the very
        // thing every migrated case runs on.
        if unit.program == SURVIVING_RUNNER {
            continue;
        }
        by_carrier
            .entry(unit.program.as_str())
            .or_default()
            .push(unit.id.as_str());
    }

    // Per carrier: the residuals pinning it, and whether EVERY one of them is a permanent
    // exclusion (024 P1) — a carrier pinned only by permanent residuals will never become
    // deletable, and saying so keeps the blocker list from reading as a queue.
    let mut blocking_residuals: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    let mut all_permanent: BTreeMap<&str, bool> = BTreeMap::new();
    for residual in &registry.residuals {
        if let Some(carrier) = residual.blocked_carrier.as_deref() {
            blocking_residuals
                .entry(carrier)
                .or_default()
                .push(residual.id.as_str());
            let permanent = residual.disposition.is_permanent();
            all_permanent
                .entry(carrier)
                .and_modify(|acc| *acc = *acc && permanent)
                .or_insert(permanent);
        }
    }

    let mut deleted: Vec<String> = Vec::new();
    let mut deletable: Vec<String> = Vec::new();
    let mut blockers: Vec<DeletionBlocker> = Vec::new();

    for (carrier, units) in by_carrier {
        // A carrier whose source is already gone was retired in an earlier pass. Its
        // units stay in the FROZEN baseline (that record is the pre-migration world and
        // never shrinks), but reporting it as "blocked" would read as work outstanding
        // when the work is done — so it is reported as DONE instead of not at all.
        if !carrier_source_exists(carrier) {
            deleted.push(carrier.to_string());
            continue;
        }
        if let Some(residuals) = blocking_residuals.get(carrier) {
            let permanent = all_permanent.get(carrier).copied().unwrap_or(false);
            let reason = if permanent {
                format!(
                    "residual record(s) {residuals:?} name it as `blockedCarrier`, and every \
                     one of them is a PERMANENT exclusion — this carrier is a settled fixture \
                     of the repository, not outstanding work (024 P1)"
                )
            } else {
                format!(
                    "residual record(s) {residuals:?} name it as `blockedCarrier`; every \
                     unit must migrate before the carrier can go (FR-013)"
                )
            };
            blockers.push(DeletionBlocker {
                carrier: carrier.to_string(),
                reason,
                permanent,
            });
            continue;
        }
        // A carrier that is the ONLY evidence for some behavior cannot be deleted, even
        // with a clean equivalence verdict. The unit-level predicate cannot see this: a
        // unit maps one-to-one to its replacement case, but a legacy case may claim
        // SEVERAL behaviors from one reported outcome (research D2's inverse defect), and
        // the replacement inherits only the behaviors its own case declares. Deleting such
        // a carrier silently uncovers the rest — which `validate` then reports as V5, i.e.
        // after the irreversible act rather than before it.
        if let Some(orphaned) = behaviors_only_this_carrier_evidences(carrier, registry) {
            blockers.push(DeletionBlocker {
                carrier: carrier.to_string(),
                reason: format!(
                    "it carries the ONLY evidence for behavior(s) {orphaned:?}; deleting it \
                     would leave them uncovered (V5). The replacement case covers fewer \
                     behaviors than the legacy case claimed."
                ),
                permanent: false,
            });
            continue;
        }
        let Some(facts) = ledger else {
            blockers.push(DeletionBlocker {
                carrier: carrier.to_string(),
                reason: format!(
                    "no equivalence ledger: none of its {} unit(s) has been proven \
                     equivalent-or-stricter, and unproven is not the same as safe (FR-034)",
                    units.len()
                ),
                permanent: false,
            });
            continue;
        };
        let permissive: Vec<&str> = units
            .iter()
            .copied()
            .filter(|u| facts.more_permissive.contains(*u))
            .collect();
        if !permissive.is_empty() {
            blockers.push(DeletionBlocker {
                carrier: carrier.to_string(),
                reason: format!(
                    "the replacement is MORE PERMISSIVE for unit(s) {permissive:?} — it \
                     misses a difference the superseded path catches (FR-035)"
                ),
                permanent: false,
            });
            continue;
        }
        let unproven: Vec<&str> = units
            .iter()
            .copied()
            .filter(|u| !facts.cleared.contains(*u))
            .collect();
        if !unproven.is_empty() {
            blockers.push(DeletionBlocker {
                carrier: carrier.to_string(),
                reason: format!(
                    "unit(s) {unproven:?} have no equivalence verdict; every unit a \
                     carrier carries must be cleared (FR-038)"
                ),
                permanent: false,
            });
            continue;
        }
        deletable.push(carrier.to_string());
    }

    deleted.sort();
    deletable.sort();
    blockers.sort_by(|a, b| a.carrier.cmp(&b.carrier));
    (deleted, deletable, blockers)
}

// ---------------------------------------------------------------------------
// Rendering (T074)
// ---------------------------------------------------------------------------

/// Render the report to its canonical JSON string: 2-space indent, LF endings, trailing
/// newline. No timestamps, no absolute paths (FR-043).
pub fn render_report_json(report: &MigrationReport) -> String {
    let mut out = serde_json::to_string_pretty(report)
        .unwrap_or_else(|e| unreachable!("report serialization is infallible: {e}"));
    out.push('\n');
    out
}

/// Render the report as Markdown — a **pure function of the report value** (T074,
/// FR-043), so the two formats can never disagree and the Markdown inherits the JSON's
/// determinism for free.
pub fn render_report_md(report: &MigrationReport) -> String {
    use std::fmt::Write as _;
    let mut out = String::new();
    let b = &report.totals.before;
    let a = &report.totals.after;

    let _ = writeln!(out, "# Migration conservation report");
    let _ = writeln!(out);
    let _ = writeln!(
        out,
        "Baseline revision `{}` · schema v{}",
        report.baseline_revision, report.schema_version
    );
    let _ = writeln!(out);
    let _ = writeln!(
        out,
        "**{}**",
        if report.is_clean() {
            "Every baseline item is accounted for."
        } else {
            "UNACCOUNTED ITEMS — see Violations."
        }
    );
    let _ = writeln!(out);

    let _ = writeln!(out, "## Totals");
    let _ = writeln!(out);
    let _ = writeln!(out, "| | before | after |");
    let _ = writeln!(out, "|---|---:|---:|");
    let _ = writeln!(out, "| units (executable) | {} | — |", b.units);
    let _ = writeln!(
        out,
        "| units at branch point | {} | — |",
        b.units_at_branch_point
    );
    let _ = writeln!(
        out,
        "| recorded-only entries | {} | — |",
        b.recorded_only_units
    );
    let _ = writeln!(out, "| cases | — | {} |", a.cases);
    let _ = writeln!(out, "| variants | — | {} |", a.variants);
    let _ = writeln!(out, "| behaviors | {} | {} |", b.behaviors, a.behaviors);
    let _ = writeln!(out, "| channels | {} | {} |", b.channels, a.channels);
    let _ = writeln!(out, "| fixtures | {} | {} |", b.fixtures, a.fixtures);
    let _ = writeln!(out, "| exceptions | {} | {} |", b.exceptions, a.exceptions);
    let _ = writeln!(out);
    // The em-dashed rows are one-sided on purpose, but a reader still tries to subtract
    // them. Units and cases are different populations — say so once, here.
    let _ = writeln!(
        out,
        "Rows with `—` have no counterpart on that side: pre-migration units and \
         post-migration cases are different populations and do not subtract. Conservation \
         is measured by **Accounting** below (every unit has exactly one disposition), not \
         by comparing the two counts."
    );
    let _ = writeln!(out);
    // The fixture row is the one number that legitimately falls, and a falling number in
    // a conservation report is exactly what a reviewer should stop on. Say why here
    // rather than making them derive it.
    if a.fixtures < b.fixtures {
        let _ = writeln!(
            out,
            "The `after` fixture count is the set the CASES consume. It is lower because a \
             residual unit's fixture stays with its still-living carrier and is consumed by \
             no case; nothing was dropped. Every before-fixture maps one-to-one under \
             `fixtureMapping`, which V22 enforces independently of this count."
        );
        let _ = writeln!(out);
    }

    let acc = &report.accounting;
    let _ = writeln!(out, "## Accounting");
    let _ = writeln!(out);
    let _ = writeln!(
        out,
        "migrated {} · deduplicated {} · residual {} · retired {} · unaccounted {}",
        acc.migrated,
        acc.deduplicated,
        acc.residual,
        acc.retired,
        acc.unaccounted.len()
    );
    let _ = writeln!(out);
    // Without this the reader hits an apparent contradiction: these counters cover the
    // EXECUTABLE units only, while the residual queue below counts every unit a residual
    // blocks — including the recorded-only entries, which have dispositions but no
    // counter. Same units, two denominators.
    let _ = writeln!(
        out,
        "Counted over the {} executable unit(s). The {} recorded-only entr(ies) each carry \
         a disposition too, but are excluded from these counters (research D8) — which is \
         why the residual queue below can name more units than `residual` here.",
        b.units, b.recorded_only_units
    );
    let _ = writeln!(out);
    for item in &acc.unaccounted {
        let _ = writeln!(
            out,
            "- **unaccounted** `{}` (from `{}`) — asserted: {}",
            item.unit, item.program, item.assertion
        );
    }
    if !acc.unaccounted.is_empty() {
        let _ = writeln!(out);
    }

    let ep = &report.error_paths;
    let _ = writeln!(out, "## Error paths");
    let _ = writeln!(out);
    let _ = writeln!(
        out,
        "{} rejection unit(s) before · {} preserved · {} weakened",
        ep.before,
        ep.preserved,
        ep.weakened.len()
    );
    let _ = writeln!(out);
    for weak in &ep.weakened {
        let _ = writeln!(
            out,
            "- **weakened** `{}` lost its {} expectation (cases: {})",
            weak.unit,
            weak.lost.as_str(),
            weak.cases.join(", ")
        );
    }
    if !ep.weakened.is_empty() {
        let _ = writeln!(out);
    }

    if !report.deduplication.is_empty() {
        let _ = writeln!(out, "## Deduplications");
        let _ = writeln!(out);
        for dedup in &report.deduplication {
            let _ = writeln!(
                out,
                "- `{}` absorbs {} unit(s) into {} — {}",
                dedup.behavior,
                dedup.absorbed_units.len(),
                dedup.cases.join(", "),
                dedup.rationale
            );
        }
        let _ = writeln!(out);
    }

    if !report.retired.is_empty() {
        let _ = writeln!(out, "## Retired");
        let _ = writeln!(out);
        for entry in &report.retired {
            let _ = writeln!(out, "- `{}` — {}", entry.unit, entry.rationale);
        }
        let _ = writeln!(out);
    }

    // Queued and permanent residuals are rendered separately (024 P1) for the same reason
    // `certify` splits them: a queue that can never reach zero cannot be read as progress.
    // Both lists are derived from the single `residualQueue` array, so the Markdown stays a
    // pure function of the JSON.
    let (queued, permanent): (Vec<&ResidualEntry>, Vec<&ResidualEntry>) = report
        .residual_queue
        .iter()
        .partition(|r| !r.disposition.is_permanent());

    let _ = writeln!(out, "## Residual queue");
    let _ = writeln!(out);
    if queued.is_empty() {
        let _ = writeln!(
            out,
            "Empty — every remaining residual is a permanent exclusion (below)."
        );
    }
    for residual in &queued {
        let _ = writeln!(
            out,
            "- `{}` blocks `{}` ({} unit(s)) — missing: {} [follow-up {}]",
            residual.id,
            residual
                .blocked_carrier
                .as_deref()
                .unwrap_or("<no carrier>"),
            residual.units.len(),
            residual.missing_capability,
            residual.follow_up.as_deref().unwrap_or("<untracked>")
        );
    }
    let _ = writeln!(out);

    if !permanent.is_empty() {
        let _ = writeln!(out, "## Permanent exclusions");
        let _ = writeln!(out);
        let _ = writeln!(
            out,
            "These units can never be expressed as data. They are not queued work: each \
             names the principle or category mismatch that forbids it."
        );
        let _ = writeln!(out);
        for residual in &permanent {
            let _ = writeln!(
                out,
                "- `{}` pins `{}` ({} unit(s)) — missing: {} — out of scope: {}",
                residual.id,
                residual
                    .blocked_carrier
                    .as_deref()
                    .unwrap_or("<no carrier>"),
                residual.units.len(),
                residual.missing_capability,
                residual
                    .out_of_scope_rationale
                    .as_deref()
                    .unwrap_or("<unjustified>")
            );
        }
        let _ = writeln!(out);
    }

    if !report.strictness_improvements.is_empty() {
        let _ = writeln!(out, "## Strictness improvements");
        let _ = writeln!(out);
        for improvement in &report.strictness_improvements {
            let _ = writeln!(
                out,
                "- `{}` — {} [characterized as {}]",
                improvement.unit,
                improvement.detail,
                improvement
                    .characterized_as
                    .as_deref()
                    .unwrap_or("NOTHING — this is a suppression")
            );
        }
        let _ = writeln!(out);
    }

    let _ = writeln!(out, "## Deletion linkage");
    let _ = writeln!(out);
    for carrier in &report.deleted_carriers {
        let _ = writeln!(
            out,
            "- **deleted** `{carrier}` — the predicate held and the carrier is gone; its \
             units remain in the frozen baseline as the record of what it used to cover"
        );
    }
    if !report.deleted_carriers.is_empty() {
        let _ = writeln!(out);
    }
    // Pending blockers and permanent pins are counted separately (024 P1): a carrier pinned
    // only by permanent exclusions will never be deletable, so listing it beside genuinely
    // pending ones would read as work that does not exist.
    let (pending, permanent_pins): (Vec<&DeletionBlocker>, Vec<&DeletionBlocker>) =
        report.deletion_blockers.iter().partition(|b| !b.permanent);

    if report.deletable_carriers.is_empty() {
        let _ = writeln!(
            out,
            "No SURVIVING carrier is deletable yet — {} pending, {} permanently pinned{}.",
            pending.len(),
            permanent_pins.len(),
            if report.deleted_carriers.is_empty() {
                String::new()
            } else {
                format!(
                    ", {} already deleted (listed above)",
                    report.deleted_carriers.len()
                )
            }
        );
    } else {
        for carrier in &report.deletable_carriers {
            let _ = writeln!(out, "- **deletable** `{carrier}`");
        }
    }
    let _ = writeln!(out);
    for blocker in &pending {
        let _ = writeln!(out, "- `{}` — {}", blocker.carrier, blocker.reason);
    }
    if !permanent_pins.is_empty() {
        let _ = writeln!(out);
        let _ = writeln!(
            out,
            "Permanently pinned (not a queue — these carriers stay by design):"
        );
        let _ = writeln!(out);
        for blocker in &permanent_pins {
            let _ = writeln!(out, "- `{}` — {}", blocker.carrier, blocker.reason);
        }
    }
    let _ = writeln!(out);

    let _ = writeln!(out, "## Violations");
    let _ = writeln!(out);
    if report.violations.is_empty() {
        let _ = writeln!(out, "None.");
    } else {
        for violation in &report.violations {
            let _ = writeln!(
                out,
                "- **condition {}** `{}` — {}",
                violation.condition, violation.item, violation.message
            );
        }
    }
    out
}

/// Write both report renderings atomically into `out_dir` (temp file + rename, via the
/// single [`crate::atomic_write`] primitive).
pub fn write_reports(
    out_dir: &std::path::Path,
    report: &MigrationReport,
) -> std::io::Result<(std::path::PathBuf, std::path::PathBuf)> {
    let json_path = out_dir.join("migration-report.json");
    let md_path = out_dir.join("migration-report.md");
    crate::atomic_write(&json_path, &render_report_json(report))?;
    crate::atomic_write(&md_path, &render_report_md(report))?;
    Ok((json_path, md_path))
}
