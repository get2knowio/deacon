//! Constrained candidate generation from the pinned grammar
//! (025-exploratory-parity-discovery, T030, FR-006/FR-007/FR-008a/FR-011/FR-012).
//!
//! Draws from [`super::grammar`] so that `required` keys are satisfied for **valid**
//! candidates and violated deliberately for **near-valid** ones — the distinction the
//! `required` constraint kind exists to make (research D1). Nothing here is hand-authored
//! schema knowledge: the branch set comes from the inventory's `union-alternative` shape,
//! the property set from its `property-existence` units, the value domain from its `type`
//! and `enum` units, and the valid/near-valid line from its `required` units.
//!
//! ## Why the grammar rather than a hand-written generator
//!
//! A hand-authored grammar generates the shapes its author thought of, which is exactly
//! what the curated fixtures already do and exactly the maintainer imagination this
//! feature exists to escape (research D1). Drawing from the committed inventory also
//! means a re-vendored schema pin changes `grammarVersion`, which correctly invalidates
//! every finding bound to the old value with no separate bookkeeping.
//!
//! ## Three candidate kinds, one stream
//!
//! | Kind | Source | What it probes |
//! |---|---|---|
//! | [`CandidateKind::Valid`] | grammar draw satisfying one branch's `required` set | agreement on inputs both implementations should accept |
//! | [`CandidateKind::NearValid`] | the same draw with one `required` key removed | agreement on *rejection*, the place strictness divergences live |
//! | [`CandidateKind::MutatedFixture`] | a committed fixture plus mutation operators | the near-miss neighbourhood of a document known to work |
//!
//! The mutation seed corpus is the **committed deterministic fixtures only** (FR-008a),
//! embedded with `include_str!` so generation is reproducible from the recorded seed and
//! pinned input set with no filesystem or network access at all. The real-world corpus is
//! deliberately not a seed source: it would make the candidate stream depend on a fetch.
//!
//! ## Category coverage is structural, not statistical
//!
//! The primary mutation category cycles round-robin through a seed-shuffled permutation of
//! the eleven, and the generator retries across base documents until that category has a
//! target. SC-003 ("every declared category applied at least once") is therefore a
//! property of the schedule rather than a bet on a long-enough run — a campaign that ends
//! early still covers every category it had budget to reach, and a category reported as
//! never applied is a real generation deficiency rather than a short-run artifact.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};

use super::grammar::Grammar;
use super::hash8;
use super::mutate::{self, Mutation, MutationCategory};
use super::rng::{Prng, prng_identity};
use super::shrink::reduction_catalogue_identity;
use crate::model::ConstraintKind;

/// The revision of *this module's* derivation — bumped whenever a draw could move.
///
/// Distinct from the PRNG's own version: the stream can stay identical while the order in
/// which the generator consumes it changes, and both determine the candidate sequence.
pub const GENERATOR_VERSION: u32 = 1;

/// The `generatorVersion` element of a campaign's pinned input set (data-model.md § 4).
///
/// Covers the two things that determine output but are neither a grammar nor a mutation:
/// the pseudorandom stream's algorithm identity (FR-001 depends on it) and the reduction
/// catalogue's *order* (FR-020 depends on it). Folding either into
/// `mutationCatalogVersion` would name it for something it is not, so a deliberate change
/// to reduction order would look like a change to the mutation operators.
pub fn generator_identity() -> String {
    format!(
        "{}+{}+generator/v{GENERATOR_VERSION}",
        prng_identity(),
        reduction_catalogue_identity()
    )
}

// ---------------------------------------------------------------------------
// Grammar anchors
// ---------------------------------------------------------------------------

/// Properties shared by every branch (`devContainerCommon`).
const GROUP_COMMON: &str = "/definitions/devContainerCommon";
/// Properties shared by the non-Compose branches (`nonComposeBase`).
const GROUP_NON_COMPOSE: &str = "/definitions/nonComposeBase";
/// The `imageContainer` branch.
const GROUP_IMAGE: &str = "/definitions/imageContainer";
/// The `composeContainer` branch.
const GROUP_COMPOSE: &str = "/definitions/composeContainer";
/// The `dockerfileContainer` branch's canonical (nested `build`) spelling.
const GROUP_DOCKERFILE: &str = "/definitions/dockerfileContainer/oneOf/0";
/// The nested `build` object of the canonical Dockerfile spelling.
const GROUP_BUILD: &str = "/definitions/dockerfileContainer/oneOf/0/properties/build/allOf/0";

/// The three configuration-source branches the schema's `oneOf` declares.
///
/// Named rather than derived from the `union-alternative` units because the *names* are
/// what a candidate and a finding report; the branch **contents** — which keys exist,
/// which are required, what types they hold — all come from the grammar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigBranch {
    /// `image`.
    Image,
    /// `build.dockerfile` (the canonical containers.dev spelling).
    Dockerfile,
    /// `dockerComposeFile` + `service` + `workspaceFolder`.
    Compose,
}

impl ConfigBranch {
    /// Every branch, in declaration order.
    pub fn all() -> &'static [ConfigBranch; 3] {
        &[
            ConfigBranch::Image,
            ConfigBranch::Dockerfile,
            ConfigBranch::Compose,
        ]
    }

    /// The stable wire spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            ConfigBranch::Image => "image",
            ConfigBranch::Dockerfile => "dockerfile",
            ConfigBranch::Compose => "compose",
        }
    }

    /// The grammar groups whose properties this branch may carry, in draw order.
    fn groups(self) -> &'static [&'static str] {
        match self {
            ConfigBranch::Image => &[GROUP_COMMON, GROUP_NON_COMPOSE, GROUP_IMAGE],
            ConfigBranch::Dockerfile => &[GROUP_COMMON, GROUP_NON_COMPOSE, GROUP_DOCKERFILE],
            ConfigBranch::Compose => &[GROUP_COMMON, GROUP_COMPOSE],
        }
    }
}

// ---------------------------------------------------------------------------
// The committed mutation seed corpus (FR-008a)
// ---------------------------------------------------------------------------

/// One committed fixture, embedded at compile time.
///
/// `include_str!` rather than a filesystem read so generation needs no I/O at all: FR-008a
/// requires the seed corpus to be the committed deterministic fixtures, and a compile-time
/// embed makes "committed" and "deterministic" properties of the build rather than of the
/// working tree.
struct SeedFixture {
    name: &'static str,
    raw: &'static str,
}

/// The committed seed corpus. Plain-JSON fixtures only — the two JSONC fixtures
/// (`fixtures/config/{basic,with-variables}/devcontainer.jsonc`) are deliberately absent:
/// parsing them here would need a second JSONC parser, and a second parser is a second
/// opinion on what a document says (the same argument FR-015 makes about normalization).
const SEED_FIXTURES: &[SeedFixture] = &[
    SeedFixture {
        name: "config-image-reference",
        raw: include_str!("../../../../fixtures/config/build/image-reference/devcontainer.json"),
    },
    SeedFixture {
        name: "config-compose-service-target",
        raw: include_str!(
            "../../../../fixtures/config/build/compose-service-target/devcontainer.json"
        ),
    },
    SeedFixture {
        name: "config-compose-multiservice",
        raw: include_str!(
            "../../../../fixtures/config/compose-multiservice/.devcontainer/devcontainer.json"
        ),
    },
    SeedFixture {
        name: "up-single-container",
        raw: include_str!(
            "../../../../fixtures/devcontainer-up/single-container/devcontainer.json"
        ),
    },
    SeedFixture {
        name: "up-feature-and-dotfiles",
        raw: include_str!(
            "../../../../fixtures/devcontainer-up/feature-and-dotfiles/devcontainer.json"
        ),
    },
];

/// The seed corpus, parsed. A fixture that stops parsing is a hard error rather than a
/// silently smaller corpus: a corpus that shrinks without anyone noticing explores less
/// and reports the same "found nothing".
fn seed_corpus() -> Vec<(&'static str, Value)> {
    SEED_FIXTURES
        .iter()
        .map(|f| {
            let value: Value = serde_json::from_str(f.raw).unwrap_or_else(|e| {
                unreachable!(
                    "committed seed fixture `{}` no longer parses as JSON: {e}. It is \
                     embedded with include_str!, so this is a compile-time-committed \
                     document that changed shape.",
                    f.name
                )
            });
            (f.name, value)
        })
        .collect()
}

/// The names of the committed seed fixtures, for reporting and tests.
pub fn seed_fixture_names() -> Vec<&'static str> {
    SEED_FIXTURES.iter().map(|f| f.name).collect()
}

// ---------------------------------------------------------------------------
// Candidates
// ---------------------------------------------------------------------------

/// One consumer-surface invocation a candidate is executed under.
///
/// `${WORKSPACE}` is the same token the declarative conformance runner uses, so a
/// candidate's operations read the same way a case's do.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Operation {
    /// The consumer subcommand (`read-configuration`, `up`, …).
    pub subcommand: String,
    /// Its argv, `${WORKSPACE}`-tokenized.
    pub argv: Vec<String>,
}

impl Operation {
    /// The configuration-resolution operation every hermetic-tier candidate runs.
    pub fn read_configuration() -> Operation {
        Operation {
            subcommand: "read-configuration".to_string(),
            argv: vec![
                "read-configuration".to_string(),
                "--workspace-folder".to_string(),
                "${WORKSPACE}".to_string(),
            ],
        }
    }
}

/// How a candidate's **base document** was produced.
///
/// It describes the base, not the final document: a [`CandidateKind::Valid`] draw that
/// then had a mutation operator applied is very likely no longer valid. That is deliberate
/// — the kind and [`Candidate::mutations`] are two independent facts, and collapsing them
/// into one label would lose the ability to say "this started from a valid draw" — but it
/// means a consumer asking "was this input deliberately malformed?" must consult **both**
/// (see `parity_harness::discovery::campaign`, which does).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CandidateKind {
    /// A grammar draw satisfying its branch's `required` set.
    Valid,
    /// A grammar draw with one `required` key deliberately removed.
    NearValid,
    /// A committed fixture used as a mutation base.
    MutatedFixture,
}

impl CandidateKind {
    /// The stable wire spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            CandidateKind::Valid => "valid",
            CandidateKind::NearValid => "near-valid",
            CandidateKind::MutatedFixture => "mutated-fixture",
        }
    }
}

/// A generated candidate: the document, its operations, and its full provenance.
#[derive(Debug, Clone, PartialEq)]
pub struct Candidate {
    /// `cnd-<hash8>` over `canonical(document) ‖ canonical(operations)`.
    pub id: String,
    /// The candidate's position in the seed's stream, from zero.
    pub index: u64,
    /// How it was produced.
    pub kind: CandidateKind,
    /// The branch it draws from, when it is a grammar draw.
    pub branch: Option<ConfigBranch>,
    /// The committed fixture it mutates, when it is a mutated fixture.
    pub fixture: Option<&'static str>,
    /// The configuration document.
    pub document: Value,
    /// Every mutation operator applied, in application order (FR-009 attribution).
    pub mutations: Vec<Mutation>,
    /// The `required` keys deliberately removed, when near-valid.
    pub violated_required: Vec<String>,
    /// The ordered operations it is executed under.
    pub operations: Vec<Operation>,
}

impl Candidate {
    /// The `mop-` operator ids this candidate carries, in application order.
    pub fn operator_ids(&self) -> Vec<String> {
        self.mutations.iter().map(|m| m.operator.clone()).collect()
    }

    /// Derive the `cnd-` id from the document and operations.
    ///
    /// The canonical form is `serde_json`'s own rendering, which **preserves key order**
    /// (this crate enables `preserve_order`). Sorting keys first would be wrong here, not
    /// merely different: `mop-ordering-change` deliberately produces documents that differ
    /// only in the order of a declaration-ordered collection, and a key-sorting canonical
    /// form would give those the same id — collapsing exactly the candidates that category
    /// exists to explore.
    pub fn derive_id(document: &Value, operations: &[Operation]) -> String {
        let doc = serde_json::to_string(document)
            .unwrap_or_else(|e| unreachable!("a candidate document always serializes: {e}"));
        let ops = serde_json::to_string(operations)
            .unwrap_or_else(|e| unreachable!("candidate operations always serialize: {e}"));
        format!("cnd-{}", hash8(&[&doc, &ops]))
    }
}

// ---------------------------------------------------------------------------
// The generator
// ---------------------------------------------------------------------------

/// The deterministic candidate stream for one seed.
///
/// Two generators built from the same seed and the same grammar produce the identical
/// ordered sequence forever — the property SC-001 verifies end to end and FR-001 requires.
#[derive(Debug)]
pub struct Generator<'a> {
    grammar: &'a Grammar,
    prng: Prng,
    /// The seed-shuffled category schedule; candidate `n` gets `schedule[n % 11]` as its
    /// primary category.
    schedule: Vec<MutationCategory>,
    seeds: Vec<(&'static str, Value)>,
    produced: u64,
}

impl<'a> Generator<'a> {
    /// Build a generator for `seed` over `grammar`.
    pub fn new(grammar: &'a Grammar, seed: u64) -> Generator<'a> {
        let mut prng = Prng::from_seed(seed);
        let mut schedule: Vec<MutationCategory> = MutationCategory::all().to_vec();
        prng.shuffle(&mut schedule);
        Generator {
            grammar,
            prng,
            schedule,
            seeds: seed_corpus(),
            produced: 0,
        }
    }

    /// The next candidate in the stream.
    ///
    /// Always yields: a generator that could return `None` would let a campaign end early
    /// for a reason nobody recorded, and "the generator stopped" would be indistinguishable
    /// from "the budget ran out".
    pub fn next_candidate(&mut self) -> Candidate {
        let index = self.produced;
        self.produced += 1;

        // The primary category cycles round-robin, so eleven consecutive candidates cover
        // the whole catalogue regardless of how the draws fall (SC-003).
        let primary = self.schedule[(index as usize) % self.schedule.len()];

        // A base document: a committed fixture two candidates in three, a grammar draw
        // otherwise. Mutating a document known to work probes the near-miss neighbourhood;
        // drawing from the grammar reaches shapes no fixture author wrote.
        let use_fixture = self.prng.next_bounded(3).unwrap_or(0) < 2;
        let (mut document, mut kind, mut branch, mut fixture, mut violated) = if use_fixture {
            let (name, doc) = self.draw_fixture();
            (
                doc,
                CandidateKind::MutatedFixture,
                None,
                Some(name),
                Vec::new(),
            )
        } else {
            let branch = *self
                .prng
                .choose(ConfigBranch::all())
                .unwrap_or(&ConfigBranch::Image);
            let document = self.draw_document(branch);
            (
                document,
                CandidateKind::Valid,
                Some(branch),
                None,
                Vec::new(),
            )
        };

        // One candidate in four is near-valid: a `required` key the grammar names is
        // removed. This is the line the `required` constraint kind draws, and it is where
        // the strictness divergences between the two implementations live.
        if self.prng.next_bounded(4) == Some(0) {
            if let Some(removed) = self.violate_required(&mut document, branch) {
                kind = CandidateKind::NearValid;
                violated.push(removed);
            }
        }

        // Apply the primary category, retrying across base documents when it has no
        // target here — so the schedule's coverage guarantee survives a document that
        // happens not to suit it.
        let mut mutations = Vec::new();
        match mutate::apply(primary, &document, &mut self.prng) {
            Some(applied) => {
                document = applied.document;
                mutations.push(applied.mutation);
            }
            None => {
                if let Some(applied) = self.retry_primary(primary) {
                    document = applied.0;
                    fixture = applied.2;
                    branch = applied.3;
                    kind = if fixture.is_some() {
                        CandidateKind::MutatedFixture
                    } else {
                        CandidateKind::Valid
                    };
                    violated.clear();
                    mutations.push(applied.1);
                }
            }
        }

        // A second, freely drawn operator on some candidates, so operator *interactions*
        // are reachable — a defect that needs two mutations to surface is invisible to a
        // stream that only ever applies one.
        if self.prng.next_bool() {
            let extra = *self
                .prng
                .choose(MutationCategory::all().as_slice())
                .unwrap_or(&primary);
            if let Some(applied) = mutate::apply(extra, &document, &mut self.prng) {
                document = applied.document;
                mutations.push(applied.mutation);
            }
        }

        let operations = vec![Operation::read_configuration()];
        Candidate {
            id: Candidate::derive_id(&document, &operations),
            index,
            kind,
            branch,
            fixture,
            document,
            mutations,
            violated_required: violated,
            operations,
        }
    }

    /// Draw a committed fixture.
    fn draw_fixture(&mut self) -> (&'static str, Value) {
        let index = self.prng.next_index(self.seeds.len()).unwrap_or(0);
        let (name, doc) = &self.seeds[index];
        (name, doc.clone())
    }

    /// Try every base document in turn until `primary` applies to one.
    ///
    /// Returns the mutated document, the mutation, the fixture name (if the base was a
    /// fixture), and the branch (if it was a grammar draw). Bases are tried in a fixed
    /// order — fixtures first, then each branch — so the retry is as reproducible as the
    /// first attempt.
    #[allow(clippy::type_complexity)]
    fn retry_primary(
        &mut self,
        primary: MutationCategory,
    ) -> Option<(Value, Mutation, Option<&'static str>, Option<ConfigBranch>)> {
        for index in 0..self.seeds.len() {
            let (name, base) = self.seeds[index].clone();
            if let Some(applied) = mutate::apply(primary, &base, &mut self.prng) {
                return Some((applied.document, applied.mutation, Some(name), None));
            }
        }
        for branch in ConfigBranch::all() {
            let base = self.draw_document(*branch);
            if let Some(applied) = mutate::apply(primary, &base, &mut self.prng) {
                return Some((applied.document, applied.mutation, None, Some(*branch)));
            }
        }
        None
    }

    /// Draw a document satisfying `branch`'s `required` set, plus a random subset of the
    /// optional properties the grammar declares for its groups.
    pub fn draw_document(&mut self, branch: ConfigBranch) -> Value {
        let mut object = Map::new();

        // Every configuration carries a name: it is the one property that makes a
        // generated document legible in a failure message.
        object.insert(
            "name".to_string(),
            Value::String(format!("discovery-{}", branch.as_str())),
        );

        // Required first, so an optional draw can never displace one.
        for key in required_keys(self.grammar, branch) {
            let value = self.draw_property(&key, branch);
            object.insert(key, value);
        }

        // Then a subset of the optional properties. Roughly a third are included, which
        // keeps documents small enough to read and large enough to interact.
        for spec in optional_properties(self.grammar, branch) {
            if self.prng.next_bounded(3) != Some(0) {
                continue;
            }
            if object.contains_key(&spec.name) || !is_generatable(&spec.name) {
                continue;
            }
            if let Some(value) = self.draw_from_spec(&spec) {
                object.insert(spec.name.clone(), value);
            }
        }

        Value::Object(object)
    }

    /// Remove one `required` key the grammar declares for the document's branch.
    ///
    /// Returns the removed key, or `None` when the document carries none (a fixture with
    /// no branch, or a draw already missing them). Silently reporting a near-valid
    /// candidate that violates nothing would make the kind a lie.
    fn violate_required(
        &mut self,
        document: &mut Value,
        branch: Option<ConfigBranch>,
    ) -> Option<String> {
        let object = document.as_object_mut()?;
        let branches: Vec<ConfigBranch> = match branch {
            Some(b) => vec![b],
            None => ConfigBranch::all().to_vec(),
        };
        let mut present: Vec<String> = branches
            .iter()
            .flat_map(|b| required_keys(self.grammar, *b))
            .filter(|k| object.contains_key(k))
            .collect();
        present.sort_unstable();
        present.dedup();
        let key = self.prng.choose(&present)?.clone();
        object.shift_remove(&key);
        Some(key)
    }

    /// Draw a value for a `required` key, preferring the grammar's own declaration.
    fn draw_property(&mut self, key: &str, branch: ConfigBranch) -> Value {
        for group in branch.groups() {
            let pointer = format!("{group}/properties/{key}");
            if let Some(spec) = property_spec(self.grammar, &pointer, key)
                && let Some(value) = self.draw_from_spec(&spec)
            {
                return value;
            }
        }
        // `build` is the one required key whose value is itself an object with its own
        // required set, so it is composed from the nested group rather than drawn flat.
        if key == "build" {
            return self.draw_build_object();
        }
        Value::String(format!("discovery-{key}"))
    }

    /// Compose the nested `build` object from its own grammar group.
    fn draw_build_object(&mut self) -> Value {
        let mut build = Map::new();
        for unit in self.grammar.of_kind(ConstraintKind::Required) {
            let Some(rest) = unit
                .pointer
                .strip_prefix(&format!("{GROUP_BUILD}/required/"))
            else {
                continue;
            };
            if rest.contains('/') {
                continue;
            }
            let value = curated_values(rest)
                .first()
                .cloned()
                .unwrap_or_else(|| Value::String(format!("discovery-{rest}")));
            build.insert(rest.to_string(), value);
        }
        if self.prng.next_bool() {
            build.insert("context".to_string(), Value::String(".".to_string()));
        }
        Value::Object(build)
    }

    /// Draw a value for one property spec: an `enum`/`const` value when the grammar
    /// declares one, else a curated value for that property name, else a generic value of
    /// one of its declared `type`s.
    fn draw_from_spec(&mut self, spec: &PropertySpec) -> Option<Value> {
        if !spec.allowed_values.is_empty() {
            return self.prng.choose(&spec.allowed_values).cloned();
        }
        let curated = curated_values(&spec.name);
        if !curated.is_empty() {
            return self.prng.choose(&curated).cloned();
        }
        let ty = self.prng.choose(&spec.types)?.clone();
        Some(generic_value(&ty))
    }
}

// ---------------------------------------------------------------------------
// Grammar projection
// ---------------------------------------------------------------------------

/// One property as the grammar declares it.
#[derive(Debug, Clone, PartialEq)]
pub struct PropertySpec {
    /// The property name.
    pub name: String,
    /// The schema pointer that declares it.
    pub pointer: String,
    /// Its declared JSON types (`type` units), possibly several.
    pub types: Vec<String>,
    /// Its exact legal values, when the grammar declares an `enum` or a `const`.
    pub allowed_values: Vec<Value>,
}

/// The `required` keys the grammar declares for `branch`, sorted.
///
/// Read from the inventory's `required` units rather than restated: the schema is the
/// authority on which keys a *valid* instance must carry, and restating it here would be
/// a second view of the pinned surface that could disagree with the first (research D1's
/// argument, applied to the valid/near-valid line specifically).
pub fn required_keys(grammar: &Grammar, branch: ConfigBranch) -> Vec<String> {
    let mut keys: Vec<String> = Vec::new();
    for group in branch.groups() {
        let prefix = format!("{group}/required/");
        for unit in grammar.of_kind(ConstraintKind::Required) {
            let Some(rest) = unit.pointer.strip_prefix(&prefix) else {
                continue;
            };
            if rest.contains('/') {
                continue;
            }
            if let Some(key) = unit.substance.get("required").and_then(Value::as_str) {
                keys.push(key.to_string());
            }
        }
    }
    keys.sort_unstable();
    keys.dedup();
    keys
}

/// Every property `branch` may carry that is not `required`, sorted by name.
pub fn optional_properties(grammar: &Grammar, branch: ConfigBranch) -> Vec<PropertySpec> {
    let required = required_keys(grammar, branch);
    let mut specs: Vec<PropertySpec> = Vec::new();
    for group in branch.groups() {
        let prefix = format!("{group}/properties/");
        for unit in grammar.of_kind(ConstraintKind::PropertyExistence) {
            let Some(name) = unit.pointer.strip_prefix(&prefix) else {
                continue;
            };
            if name.contains('/') || required.iter().any(|r| r == name) {
                continue;
            }
            if let Some(spec) = property_spec(grammar, &unit.pointer, name) {
                specs.push(spec);
            }
        }
    }
    specs.sort_by(|a, b| a.name.cmp(&b.name));
    specs.dedup_by(|a, b| a.name == b.name);
    specs
}

/// Build a [`PropertySpec`] from the units at `pointer`, or `None` when the grammar
/// declares no type there (nothing to draw from).
fn property_spec(grammar: &Grammar, pointer: &str, name: &str) -> Option<PropertySpec> {
    let mut types: Vec<String> = Vec::new();
    for unit in grammar.at_pointer_of_kind(pointer, ConstraintKind::Type) {
        match unit.substance.get("type") {
            Some(Value::String(t)) => types.push(t.clone()),
            Some(Value::Array(items)) => {
                types.extend(items.iter().filter_map(Value::as_str).map(str::to_string));
            }
            _ => {}
        }
    }
    let mut allowed_values: Vec<Value> = Vec::new();
    for unit in grammar.at_pointer_of_kind(pointer, ConstraintKind::Enum) {
        if let Some(Value::Array(items)) = unit.substance.get("enum") {
            allowed_values.extend(items.iter().cloned());
        }
    }
    for unit in grammar.at_pointer_of_kind(pointer, ConstraintKind::Const) {
        if let Some(v) = unit.substance.get("const") {
            allowed_values.push(v.clone());
        }
    }
    if types.is_empty() && allowed_values.is_empty() {
        return None;
    }
    types.sort_unstable();
    types.dedup();
    Some(PropertySpec {
        name: name.to_string(),
        pointer: pointer.to_string(),
        types,
        allowed_values,
    })
}

/// Whether generation may emit this property at all.
///
/// `initializeCommand` is excluded **by construction**: it executes on the developer's
/// host before any container sandboxing, and a machine-generated host command is exactly
/// the class deacon's workspace-trust gate exists to refuse (see `crates/core/src/trust.rs`
/// and SECURITY.md). Note that this is not the same as never producing one — the
/// `lifecycle-shape` mutation operator can still introduce it — which is deliberate: the
/// campaign's unsafe-candidate guard (FR-011) then discards and *counts* it, so the guard
/// is exercised by real traffic rather than being a branch nothing ever reaches.
///
/// `$schema` and `additionalProperties` are excluded as pure noise: neither affects
/// configuration resolution, so a difference at either would be a finding about the echo
/// rather than about the tool.
fn is_generatable(name: &str) -> bool {
    !matches!(
        name,
        "initializeCommand" | "$schema" | "additionalProperties"
    )
}

/// Curated values for a property, chosen so a drawn document reaches configuration
/// resolution rather than dying at the document-syntax stage (FR-007, SC-002).
///
/// These are **values within the grammar's declared type**, never additional structure:
/// the grammar decides what may appear and of what type; this decides which of the legal
/// values is worth drawing. A generic string for `image` would be legal and useless.
fn curated_values(name: &str) -> Vec<Value> {
    match name {
        "image" => vec![
            json!("alpine:3.19"),
            json!("debian:bookworm-slim"),
            json!("mcr.microsoft.com/devcontainers/base:ubuntu-22.04"),
        ],
        "dockerfile" | "dockerFile" => vec![json!("Dockerfile")],
        "context" => vec![json!("."), json!("..")],
        "build" => vec![json!({ "dockerfile": "Dockerfile" })],
        "dockerComposeFile" => vec![
            json!("docker-compose.yml"),
            json!(["docker-compose.yml"]),
            json!(["docker-compose.yml", "docker-compose.override.yml"]),
        ],
        "service" => vec![json!("app")],
        "runServices" => vec![json!([]), json!(["db"]), json!(["db", "cache"])],
        "workspaceFolder" => vec![json!("/workspace"), json!("/workspaces/project")],
        "workspaceMount" => vec![json!(
            "source=${localWorkspaceFolder},target=/workspace,type=bind"
        )],
        "name" => vec![json!("discovery candidate")],
        "remoteUser" | "containerUser" => vec![json!("root"), json!("vscode")],
        "forwardPorts" => vec![json!([3000]), json!([3000, 8080]), json!(["db:5432"])],
        "appPort" => vec![json!(3000), json!([3000, 8080]), json!("3000:3000")],
        "runArgs" => vec![json!(["--init"]), json!(["--init", "--rm"])],
        "capAdd" => vec![json!(["SYS_PTRACE"])],
        "securityOpt" => vec![json!(["seccomp=unconfined"])],
        "mounts" => vec![
            json!(["source=${localWorkspaceFolder}/.cache,target=/cache,type=bind"]),
            json!([{ "source": "${localWorkspaceFolder}/.cache", "target": "/cache", "type": "bind" }]),
        ],
        "containerEnv" | "remoteEnv" => vec![
            json!({}),
            json!({ "DISCOVERY": "1" }),
            json!({ "DISCOVERY_PATH": "${containerEnv:PATH}" }),
        ],
        "features" => vec![
            json!({}),
            json!({ "ghcr.io/devcontainers/features/git:1": {} }),
            json!({ "ghcr.io/devcontainers/features/common-utils:2": { "installZsh": false } }),
        ],
        "overrideFeatureInstallOrder" => vec![
            json!([]),
            json!(["ghcr.io/devcontainers/features/common-utils"]),
        ],
        "customizations" => vec![
            json!({}),
            json!({ "vscode": { "extensions": ["rust-lang.rust-analyzer"] } }),
        ],
        "portsAttributes" => vec![
            json!({}),
            json!({ "3000": { "label": "web", "onAutoForward": "notify" } }),
        ],
        "otherPortsAttributes" => vec![json!({}), json!({ "onAutoForward": "silent" })],
        "hostRequirements" => vec![json!({}), json!({ "cpus": 2 })],
        "secrets" => vec![json!({}), json!({ "TOKEN": { "description": "a token" } })],
        "onCreateCommand"
        | "updateContentCommand"
        | "postCreateCommand"
        | "postStartCommand"
        | "postAttachCommand" => vec![
            json!("echo discovery"),
            json!(["echo", "discovery"]),
            json!({ "step": "echo discovery" }),
        ],
        _ => Vec::new(),
    }
}

/// A generic value of a declared JSON type, for a property with no curated pool.
fn generic_value(ty: &str) -> Value {
    match ty {
        "string" => json!("discovery"),
        "boolean" => json!(true),
        "integer" | "number" => json!(1),
        "array" => json!([]),
        "object" => json!({}),
        _ => Value::Null,
    }
}

// ---------------------------------------------------------------------------
// FR-011 / FR-012 — safety and pinning predicates
// ---------------------------------------------------------------------------

/// Why a candidate must not be executed (FR-011).
///
/// Pure and hermetic so the guard is unit-testable without a campaign, and so the campaign
/// driver's only job is to *discard and count* — a guard whose decision lives inside the
/// driver can only be tested by running one.
pub fn unsafe_reasons(document: &Value, container_backed: bool) -> Vec<String> {
    let Some(object) = document.as_object() else {
        return vec!["the document is not a JSON object".to_string()];
    };
    let mut reasons = Vec::new();

    // `initializeCommand` executes on the developer's HOST before any container
    // sandboxing. A generated host command is precisely what deacon's workspace-trust
    // gate refuses, and a discovery campaign must never be the thing that runs one.
    if object.contains_key("initializeCommand") {
        reasons.push(
            "carries `initializeCommand`, which executes on the host before any container \
             sandboxing — a machine-generated host command is never executed"
                .to_string(),
        );
    }

    if !container_backed {
        // The remaining hazards are all about what a container would be granted. A
        // configuration-resolution comparison starts no container, so flagging them there
        // would discard candidates for a risk the tier does not take.
        return reasons;
    }

    if object.get("privileged") == Some(&Value::Bool(true)) {
        reasons.push("requests `privileged: true`".to_string());
    }
    if let Some(Value::Array(args)) = object.get("runArgs") {
        for arg in args.iter().filter_map(Value::as_str) {
            if arg == "--privileged" || arg.starts_with("--device") || arg.starts_with("--pid=host")
            {
                reasons.push(format!("`runArgs` contains `{arg}`"));
            }
        }
    }
    for mount in mount_sources(object) {
        if is_sensitive_host_path(&mount) {
            reasons.push(format!("binds the sensitive host path `{mount}`"));
        }
    }

    reasons
}

/// Image inputs that are not pinned (FR-012).
///
/// An unpinned input makes the comparison non-reproducible in the one way the pinned input
/// set cannot record: `alpine:latest` is a different image tomorrow, so a finding recorded
/// against it is a claim about content nobody can retrieve. The same rule the declarative
/// runner enforces as **V18** for committed Docker cases.
pub fn unpinned_image_inputs(document: &Value) -> Vec<String> {
    let Some(object) = document.as_object() else {
        return Vec::new();
    };
    let mut unpinned = Vec::new();
    if let Some(Value::String(image)) = object.get("image")
        && !is_pinned_image(image)
    {
        unpinned.push(image.clone());
    }
    unpinned
}

/// Whether an image reference is pinned: a digest, or a concrete tag that is not
/// `latest`. A tag-less reference resolves to `latest` and is therefore unpinned too.
pub fn is_pinned_image(image: &str) -> bool {
    if image.contains("@sha256:") {
        return true;
    }
    // The tag is what follows the last `:` — unless that `:` is inside a registry host's
    // port (`localhost:5000/img`), which is why the tail must not contain a `/`.
    match image.rsplit_once(':') {
        Some((_, tag)) if !tag.contains('/') => !tag.is_empty() && tag != "latest",
        _ => false,
    }
}

/// Every mount source string a document declares, in both the string and object forms.
fn mount_sources(object: &Map<String, Value>) -> Vec<String> {
    let mut sources = Vec::new();
    let mut collect = |value: &Value| match value {
        Value::String(spec) => {
            for part in spec.split(',') {
                if let Some(rest) = part.trim().strip_prefix("source=") {
                    sources.push(rest.to_string());
                }
            }
        }
        Value::Object(map) => {
            if let Some(Value::String(s)) = map.get("source") {
                sources.push(s.clone());
            }
        }
        _ => {}
    };
    if let Some(Value::Array(mounts)) = object.get("mounts") {
        for mount in mounts {
            collect(mount);
        }
    }
    if let Some(mount) = object.get("workspaceMount") {
        collect(mount);
    }
    sources
}

/// Host paths a generated candidate is never allowed to bind.
const SENSITIVE_HOST_PATHS: &[&str] = &[
    "/",
    "/etc",
    "/root",
    "/home",
    "/var/run/docker.sock",
    "/var/run",
    "/proc",
    "/sys",
    "/dev",
];

/// Whether a mount source names a sensitive host path.
///
/// `${localWorkspaceFolder}`-rooted sources are the candidate's own workspace and are
/// always allowed; an absolute path is compared against the declared list exactly or as a
/// parent directory, so `/etc/passwd` is caught by `/etc`.
fn is_sensitive_host_path(source: &str) -> bool {
    if source.contains("${") {
        return false;
    }
    if !source.starts_with('/') {
        return false;
    }
    SENSITIVE_HOST_PATHS.iter().any(|p| {
        source == *p
            || (*p != "/" && source.starts_with(&format!("{p}/")))
            || *p == "/" && source == "/"
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn grammar() -> Grammar {
        Grammar::load_default().expect("the committed constraint inventory must load")
    }

    // --- grammar projection -------------------------------------------------

    #[test]
    fn required_keys_come_from_the_inventorys_own_required_units() {
        let g = grammar();
        assert_eq!(required_keys(&g, ConfigBranch::Image), vec!["image"]);
        assert_eq!(required_keys(&g, ConfigBranch::Dockerfile), vec!["build"]);
        assert_eq!(
            required_keys(&g, ConfigBranch::Compose),
            vec!["dockerComposeFile", "service", "workspaceFolder"],
            "the Compose branch's three required keys are the grammar's, not a \
             hand-written list — a re-vendored schema changes them here automatically"
        );
    }

    #[test]
    fn optional_properties_exclude_the_required_ones_and_carry_declared_types() {
        let g = grammar();
        let optional = optional_properties(&g, ConfigBranch::Image);
        assert!(
            !optional.iter().any(|p| p.name == "image"),
            "a required key is never offered as optional"
        );
        assert!(
            optional.len() > 20,
            "got {} optional properties",
            optional.len()
        );

        let names: Vec<&str> = optional.iter().map(|p| p.name.as_str()).collect();
        for expected in [
            "remoteUser",
            "forwardPorts",
            "features",
            "runArgs",
            "waitFor",
        ] {
            assert!(
                names.contains(&expected),
                "{expected} missing from the draw domain"
            );
        }

        let wait_for = optional
            .iter()
            .find(|p| p.name == "waitFor")
            .expect("waitFor is declared");
        assert_eq!(
            wait_for.allowed_values,
            vec![
                json!("initializeCommand"),
                json!("onCreateCommand"),
                json!("updateContentCommand"),
                json!("postCreateCommand"),
                json!("postStartCommand"),
            ],
            "an `enum` property draws from the schema's exact legal values"
        );

        let ports = optional
            .iter()
            .find(|p| p.name == "forwardPorts")
            .expect("forwardPorts is declared");
        assert_eq!(ports.types, vec!["array".to_string()]);
    }

    #[test]
    fn properties_are_sorted_and_deduplicated_across_groups() {
        let g = grammar();
        for branch in ConfigBranch::all() {
            let optional = optional_properties(&g, *branch);
            let mut names: Vec<&str> = optional.iter().map(|p| p.name.as_str()).collect();
            let ordered = names.clone();
            names.sort_unstable();
            assert_eq!(
                ordered,
                names,
                "{}: draw domain must be sorted",
                branch.as_str()
            );
            let before = names.len();
            names.dedup();
            assert_eq!(
                before,
                names.len(),
                "{}: a property appears twice across groups",
                branch.as_str()
            );
        }
    }

    // --- T030: constrained generation ---------------------------------------

    #[test]
    fn a_valid_draw_satisfies_its_branchs_required_set() {
        let g = grammar();
        let mut generator = Generator::new(&g, 0x5EED_0001);
        for branch in ConfigBranch::all() {
            for _ in 0..20 {
                let doc = generator.draw_document(*branch);
                let object = doc.as_object().expect("a document is an object");
                for key in required_keys(&g, *branch) {
                    assert!(
                        object.contains_key(&key),
                        "{} draw is missing required key `{key}`: {doc}",
                        branch.as_str()
                    );
                }
            }
        }
    }

    #[test]
    fn a_dockerfile_draw_composes_the_nested_build_objects_own_required_set() {
        let g = grammar();
        let mut generator = Generator::new(&g, 42);
        let mut saw_nested_required = false;
        for _ in 0..20 {
            let doc = generator.draw_document(ConfigBranch::Dockerfile);
            let build = &doc["build"];
            assert!(build.is_object(), "`build` is an object: {doc}");
            if build.get("dockerfile").is_some() {
                saw_nested_required = true;
            }
        }
        assert!(
            saw_nested_required,
            "the nested `build` object has its own `required` set, read from the same \
             inventory rather than hand-written"
        );
    }

    #[test]
    fn a_near_valid_candidate_records_the_required_key_it_removed() {
        let g = grammar();
        let mut generator = Generator::new(&g, 0xBEEF);
        let mut near_valid = 0usize;
        for _ in 0..400 {
            let candidate = generator.next_candidate();
            if candidate.kind != CandidateKind::NearValid {
                continue;
            }
            near_valid += 1;
            assert!(
                !candidate.violated_required.is_empty(),
                "a near-valid candidate that violates nothing is a lie about its kind"
            );
        }
        assert!(
            near_valid > 0,
            "the stream must reach the near-valid kind — it is where strictness \
             divergences live"
        );
    }

    #[test]
    fn every_candidate_is_a_parseable_object_with_operations_and_an_id() {
        let g = grammar();
        let mut generator = Generator::new(&g, 7);
        for _ in 0..300 {
            let candidate = generator.next_candidate();
            assert!(candidate.document.is_object(), "{}", candidate.document);
            let rendered = serde_json::to_string(&candidate.document).expect("serializes");
            let back: Value = serde_json::from_str(&rendered).expect("round-trips");
            assert_eq!(back, candidate.document);

            assert!(candidate.id.starts_with("cnd-"));
            assert_eq!(candidate.id.len(), "cnd-".len() + 8);
            assert_eq!(
                candidate.id,
                Candidate::derive_id(&candidate.document, &candidate.operations),
                "the id must recompute from its substance"
            );
            assert_eq!(candidate.operations.len(), 1);
            assert_eq!(candidate.operations[0].subcommand, "read-configuration");
        }
    }

    #[test]
    fn the_candidate_id_distinguishes_documents_that_differ_only_in_order() {
        // `mop-ordering-change` deliberately produces documents that differ only in the
        // order of a declaration-ordered collection. A key-sorting canonical form would
        // give those the same id, collapsing exactly the candidates that category exists
        // to explore.
        let ops = vec![Operation::read_configuration()];
        let a = json!({ "image": "alpine:3.19", "forwardPorts": [3000, 8080] });
        let b = json!({ "image": "alpine:3.19", "forwardPorts": [8080, 3000] });
        assert_ne!(
            Candidate::derive_id(&a, &ops),
            Candidate::derive_id(&b, &ops)
        );

        // The operations participate too: the same document under a different subcommand
        // is a different candidate.
        let other_ops = vec![Operation {
            subcommand: "build".to_string(),
            argv: vec!["build".to_string()],
        }];
        assert_ne!(
            Candidate::derive_id(&a, &ops),
            Candidate::derive_id(&a, &other_ops)
        );
    }

    #[test]
    fn generation_never_emits_an_initialize_command() {
        // Excluded by construction: it executes on the host before any container
        // sandboxing (crates/core/src/trust.rs, SECURITY.md).
        let g = grammar();
        let mut generator = Generator::new(&g, 0xABCD);
        for branch in ConfigBranch::all() {
            for _ in 0..60 {
                let doc = generator.draw_document(*branch);
                assert!(
                    doc.get("initializeCommand").is_none(),
                    "a drawn document must never carry a host-side hook: {doc}"
                );
            }
        }
    }

    // --- SC-003 / FR-008a ---------------------------------------------------

    #[test]
    fn the_stream_applies_every_mutation_category_within_a_short_prefix() {
        // SC-003 is a property of the round-robin schedule, not a bet on a long run: any
        // window of eleven consecutive candidates covers the catalogue. Asserting over a
        // slightly longer prefix leaves room for the retry path without weakening the
        // claim.
        let g = grammar();
        let mut generator = Generator::new(&g, 0x1234_5678);
        let mut counts = mutate::empty_application_counts();
        for _ in 0..33 {
            for mutation in generator.next_candidate().mutations {
                *counts
                    .get_mut(mutation.category.name())
                    .expect("every category has a key") += 1;
            }
        }
        assert!(
            mutate::unapplied_categories(&counts).is_empty(),
            "categories never applied in 33 candidates: {:?}",
            mutate::unapplied_categories(&counts)
        );
    }

    #[test]
    fn the_seed_corpus_is_the_committed_fixtures_and_every_one_parses() {
        let names = seed_fixture_names();
        assert_eq!(names.len(), 5);
        assert!(names.contains(&"config-image-reference"));
        assert!(names.contains(&"config-compose-multiservice"));
        let corpus = seed_corpus();
        assert_eq!(corpus.len(), names.len());
        for (name, doc) in &corpus {
            assert!(doc.is_object(), "{name} is not an object");
        }
    }

    #[test]
    fn candidates_name_the_fixture_or_branch_they_came_from() {
        let g = grammar();
        let mut generator = Generator::new(&g, 99);
        let mut fixtures = 0usize;
        let mut draws = 0usize;
        for _ in 0..200 {
            let c = generator.next_candidate();
            match (c.fixture, c.branch) {
                (Some(name), None) => {
                    assert!(seed_fixture_names().contains(&name));
                    fixtures += 1;
                }
                (None, Some(_)) => draws += 1,
                other => panic!("a candidate must name exactly one provenance, got {other:?}"),
            }
        }
        assert!(
            fixtures > 0 && draws > 0,
            "both provenances must be reachable"
        );
    }

    // --- SC-001 reproducibility --------------------------------------------

    #[test]
    fn the_same_seed_reproduces_the_identical_ordered_candidate_sequence() {
        // FR-001 at the generator level. The end-to-end claim (SC-001) also covers the
        // finding set, but if this fails nothing downstream can hold.
        let g = grammar();
        let mut a = Generator::new(&g, 0xFEED_FACE);
        let mut b = Generator::new(&g, 0xFEED_FACE);
        let left: Vec<Candidate> = (0..120).map(|_| a.next_candidate()).collect();
        let right: Vec<Candidate> = (0..120).map(|_| b.next_candidate()).collect();
        assert_eq!(left, right);

        let mut other = Generator::new(&g, 0xFEED_FACF);
        let different: Vec<Candidate> = (0..120).map(|_| other.next_candidate()).collect();
        assert_ne!(
            left.iter().map(|c| c.id.clone()).collect::<Vec<String>>(),
            different
                .iter()
                .map(|c| c.id.clone())
                .collect::<Vec<String>>(),
            "a different seed must not alias"
        );
    }

    #[test]
    fn the_generator_identity_names_the_stream_and_the_reduction_order() {
        let identity = generator_identity();
        assert!(identity.contains("xoshiro256starstar"));
        assert!(identity.contains("drop-optional-key"));
        assert!(identity.ends_with("+generator/v1"));
    }

    // --- FR-011 / FR-012 ----------------------------------------------------

    #[test]
    fn a_host_side_hook_is_unsafe_on_every_tier() {
        let doc = json!({ "image": "alpine:3.19", "initializeCommand": "echo host" });
        for container_backed in [false, true] {
            let reasons = unsafe_reasons(&doc, container_backed);
            assert_eq!(reasons.len(), 1, "{reasons:?}");
            assert!(reasons[0].contains("initializeCommand"));
        }
    }

    #[test]
    fn container_only_hazards_do_not_discard_a_configuration_only_candidate() {
        // Flagging them on the configuration tier would discard candidates for a risk that
        // tier does not take — a guard that over-refuses shrinks the explored space while
        // reporting nothing about it.
        let doc = json!({
            "image": "alpine:3.19",
            "privileged": true,
            "runArgs": ["--privileged"],
            "mounts": ["source=/var/run/docker.sock,target=/var/run/docker.sock,type=bind"]
        });
        assert!(unsafe_reasons(&doc, false).is_empty());
        let reasons = unsafe_reasons(&doc, true);
        assert!(reasons.iter().any(|r| r.contains("privileged: true")));
        assert!(reasons.iter().any(|r| r.contains("--privileged")));
        assert!(reasons.iter().any(|r| r.contains("docker.sock")));
    }

    #[test]
    fn a_workspace_rooted_mount_is_not_a_sensitive_host_path() {
        let doc = json!({
            "image": "alpine:3.19",
            "workspaceMount": "source=${localWorkspaceFolder},target=/workspace,type=bind",
            "mounts": [{ "source": "${localWorkspaceFolder}/.cache", "target": "/cache", "type": "bind" }]
        });
        assert!(unsafe_reasons(&doc, true).is_empty());
    }

    #[test]
    fn unpinned_image_inputs_are_named() {
        assert!(is_pinned_image("alpine:3.19"));
        assert!(is_pinned_image(
            "alpine@sha256:0000000000000000000000000000000000000000000000000000000000000000"
        ));
        assert!(is_pinned_image("localhost:5000/img:1.2"));
        assert!(!is_pinned_image("alpine"));
        assert!(!is_pinned_image("alpine:latest"));
        assert!(
            !is_pinned_image("localhost:5000/img"),
            "a tag-less reference is `latest`"
        );

        assert_eq!(
            unpinned_image_inputs(&json!({ "image": "alpine:latest" })),
            vec!["alpine:latest".to_string()]
        );
        assert!(unpinned_image_inputs(&json!({ "image": "alpine:3.19" })).is_empty());
        assert!(unpinned_image_inputs(&json!({ "dockerComposeFile": "c.yml" })).is_empty());
    }

    #[test]
    fn a_non_object_document_is_unsafe_rather_than_silently_fine() {
        let reasons = unsafe_reasons(&json!([1, 2, 3]), false);
        assert_eq!(reasons.len(), 1);
        assert!(reasons[0].contains("not a JSON object"));
    }
}
