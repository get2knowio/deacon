//! The eleven-category mutation operator catalogue
//! (025-exploratory-parity-discovery, data-model.md § 5, T031/T032).
//!
//! The catalogue lives in code rather than as a data file because each operator is
//! executable logic; [`MUTATION_CATALOG_VERSION`] (one of the seven pinned-input-set
//! elements) pins its identity. Every application records its `mop-<name>` on the
//! witness (FR-009), which is what lets a candidate name the operators that produced it
//! and what lets shrinking un-apply one operator as a reduction step (research D5).
//!
//! ## This module is the single source of the category key list
//!
//! `CampaignOutcome::mutation_applications` must carry **all eleven keys, always**,
//! including zeroes (FR-010): a category absent from the map is indistinguishable from a
//! category that was never applied, and FR-010 requires zero to be reported as an
//! explicit generation deficiency — which needs the key present. Every producer of that
//! map therefore starts from [`empty_application_counts`] rather than restating the
//! eleven names. A second list is a list that drifts, and the drift would be silent: the
//! map would simply stop mentioning a category nobody noticed had gone.
//!
//! ## Schema-adjacent, never byte-corrupted
//!
//! Every operator rewrites the **parsed** document and returns a document that still
//! serializes, still parses, and is still a JSON object. Corrupting bytes would produce
//! candidates that die at the document-syntax stage, which is exactly the budget waste
//! SC-002 caps at 10% — a campaign whose mutations produce malformed JSON explores the
//! parser rather than the tool.

use indexmap::IndexMap;
use serde_json::{Map, Value};

use super::rng::Prng;

/// The mutation **operator set**'s identity — one of the seven pinned-input-set elements
/// (data-model.md § 4).
///
/// It names the operator set and nothing else. The pseudorandom stream and the reduction
/// catalogue's order live in `generatorVersion`, because folding either in here would
/// name them for something they are not: a deliberate change to reduction order would
/// look like a change to the mutation operators.
///
/// Bump this whenever an operator is added, removed, or changes what it produces.
pub const MUTATION_CATALOG_VERSION: &str = "mutation-catalogue/v1";

/// The eleven mandated categories (FR-008, data-model.md § 5).
///
/// A closed enum rather than strings so a twelfth category cannot appear by typo, and so
/// [`MutationCategory::all`] is exhaustive by construction rather than by review.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum MutationCategory {
    /// Insert a key absent from the schema at a pointer.
    UnknownField,
    /// Replace a value with one of a different JSON type.
    WrongType,
    /// Replace a value with `null`.
    NullValue,
    /// Empty a collection or string in place.
    EmptyValue,
    /// Add a second configuration source (image + Dockerfile + Compose).
    ConflictingSource,
    /// Corrupt a Feature identifier's registry/path/tag shape.
    InvalidFeatureId,
    /// Introduce a cycle into an `extends` chain.
    ExtendsCycle,
    /// Nest, self-reference, or leave unterminated a `${…}` token.
    SubstitutionEdge,
    /// Switch between the permitted string/array/object lifecycle forms.
    LifecycleShape,
    /// Vary service count, `runServices`, override-file ordering.
    ComposeCombination,
    /// Permute a declaration-ordered collection.
    OrderingChange,
}

/// The number of mandated categories. Stated as a constant so a caller can assert the
/// arity without counting a slice, and so FR-008's "at minimum eleven" is checkable.
pub const CATEGORY_COUNT: usize = 11;

impl MutationCategory {
    /// Every category, in the catalogue's declaration order — which is also the key order
    /// of [`empty_application_counts`], so a report renders the same way every run.
    pub fn all() -> &'static [MutationCategory; CATEGORY_COUNT] {
        &[
            MutationCategory::UnknownField,
            MutationCategory::WrongType,
            MutationCategory::NullValue,
            MutationCategory::EmptyValue,
            MutationCategory::ConflictingSource,
            MutationCategory::InvalidFeatureId,
            MutationCategory::ExtendsCycle,
            MutationCategory::SubstitutionEdge,
            MutationCategory::LifecycleShape,
            MutationCategory::ComposeCombination,
            MutationCategory::OrderingChange,
        ]
    }

    /// The category name, exactly as data-model.md § 5's table spells it.
    pub fn name(self) -> &'static str {
        match self {
            MutationCategory::UnknownField => "unknown-field",
            MutationCategory::WrongType => "wrong-type",
            MutationCategory::NullValue => "null-value",
            MutationCategory::EmptyValue => "empty-value",
            MutationCategory::ConflictingSource => "conflicting-source",
            MutationCategory::InvalidFeatureId => "invalid-feature-id",
            MutationCategory::ExtendsCycle => "extends-cycle",
            MutationCategory::SubstitutionEdge => "substitution-edge",
            MutationCategory::LifecycleShape => "lifecycle-shape",
            MutationCategory::ComposeCombination => "compose-combination",
            MutationCategory::OrderingChange => "ordering-change",
        }
    }

    /// The `mop-<name>` operator id recorded on a witness (FR-009, data-model.md § 1).
    ///
    /// Hand-assigned and stable rather than hashed: the id IS the declared name, so a
    /// reviewer reading a witness sees which operator ran without a lookup.
    pub fn operator_id(self) -> &'static str {
        match self {
            MutationCategory::UnknownField => "mop-unknown-field",
            MutationCategory::WrongType => "mop-wrong-type",
            MutationCategory::NullValue => "mop-null-value",
            MutationCategory::EmptyValue => "mop-empty-value",
            MutationCategory::ConflictingSource => "mop-conflicting-source",
            MutationCategory::InvalidFeatureId => "mop-invalid-feature-id",
            MutationCategory::ExtendsCycle => "mop-extends-cycle",
            MutationCategory::SubstitutionEdge => "mop-substitution-edge",
            MutationCategory::LifecycleShape => "mop-lifecycle-shape",
            MutationCategory::ComposeCombination => "mop-compose-combination",
            MutationCategory::OrderingChange => "mop-ordering-change",
        }
    }

    /// Parse a category name (not the `mop-` id), returning `None` on anything else —
    /// never a default, which would silently attribute an application to the wrong
    /// operator.
    pub fn parse(name: &str) -> Option<MutationCategory> {
        MutationCategory::all()
            .iter()
            .copied()
            .find(|c| c.name() == name)
    }

    /// Parse a `mop-` operator id.
    pub fn parse_operator(id: &str) -> Option<MutationCategory> {
        MutationCategory::all()
            .iter()
            .copied()
            .find(|c| c.operator_id() == id)
    }
}

/// Applications per mutation category, in catalogue declaration order.
///
/// A named alias so a caller in another crate can hold the map without naming `indexmap`
/// itself — which would make an ordering-critical container a dependency decision at every
/// call site rather than a property of the catalogue that owns it.
pub type ApplicationCounts = IndexMap<String, u64>;

/// The eleven category names, in declaration order — **the** key list for
/// `CampaignOutcome::mutation_applications`.
pub fn category_names() -> Vec<&'static str> {
    MutationCategory::all().iter().map(|c| c.name()).collect()
}

/// A fresh application-count map carrying **all eleven keys at zero**, in catalogue
/// declaration order (FR-010).
///
/// Every producer of `mutationApplications` starts here. Building the map by inserting
/// only the categories that fired is the defect FR-010 names: a category that never
/// applied would simply be missing, and "we never generated this shape" would be
/// indistinguishable from "we never looked".
pub fn empty_application_counts() -> IndexMap<String, u64> {
    MutationCategory::all()
        .iter()
        .map(|c| (c.name().to_string(), 0u64))
        .collect()
}

/// A category that never successfully applied, reported as an explicit generation
/// deficiency rather than by omission (FR-010, SC-003).
pub fn unapplied_categories(counts: &IndexMap<String, u64>) -> Vec<&'static str> {
    MutationCategory::all()
        .iter()
        .filter(|c| counts.get(c.name()).copied().unwrap_or(0) == 0)
        .map(|c| c.name())
        .collect()
}

/// One recorded mutation application (FR-009 attribution).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mutation {
    /// The category that fired.
    pub category: MutationCategory,
    /// `mop-<name>` — what the witness records.
    pub operator: String,
    /// A short, human-readable description of *where* and *what*, so a reviewer reading
    /// a candidate can tell two applications of the same operator apart.
    pub detail: String,
}

impl Mutation {
    fn new(category: MutationCategory, detail: impl Into<String>) -> Mutation {
        Mutation {
            category,
            operator: category.operator_id().to_string(),
            detail: detail.into(),
        }
    }
}

/// A successfully applied mutation and the document it produced.
#[derive(Debug, Clone, PartialEq)]
pub struct Applied {
    /// The mutated document — still an object, still serializable, still parseable.
    pub document: Value,
    /// What was applied.
    pub mutation: Mutation,
}

/// Apply one category to `document`, or return `None` when the operator has no target.
///
/// `None` is a first-class answer, not a failure: `ordering-change` on a document with no
/// multi-element collection has nothing to reorder, and inventing a collection to permute
/// would record an ordering mutation that never happened. The campaign counts only
/// **successful** applications, which is what makes FR-010's zero-count report honest.
///
/// Every branch is a pure function of `document` and the [`Prng`] draws it makes, so the
/// same seed and the same input always yield the same output — the property FR-001 rests
/// on.
pub fn apply(category: MutationCategory, document: &Value, prng: &mut Prng) -> Option<Applied> {
    let Value::Object(object) = document else {
        // Every candidate is a devcontainer.json document, which is an object. A
        // non-object has no property to mutate, so there is nothing honest to do.
        return None;
    };
    let (mutated, detail) = match category {
        MutationCategory::UnknownField => unknown_field(object, prng)?,
        MutationCategory::WrongType => wrong_type(object, prng)?,
        MutationCategory::NullValue => null_value(object, prng)?,
        MutationCategory::EmptyValue => empty_value(object, prng)?,
        MutationCategory::ConflictingSource => conflicting_source(object, prng)?,
        MutationCategory::InvalidFeatureId => invalid_feature_id(object, prng)?,
        MutationCategory::ExtendsCycle => extends_cycle(object, prng)?,
        MutationCategory::SubstitutionEdge => substitution_edge(object, prng)?,
        MutationCategory::LifecycleShape => lifecycle_shape(object, prng)?,
        MutationCategory::ComposeCombination => compose_combination(object, prng)?,
        MutationCategory::OrderingChange => ordering_change(object, prng)?,
    };
    Some(Applied {
        document: Value::Object(mutated),
        mutation: Mutation::new(category, detail),
    })
}

// ---------------------------------------------------------------------------
// The eleven operators
// ---------------------------------------------------------------------------

/// Key names that are **not** properties of the pinned devcontainer schema.
///
/// A fixed pool rather than a generated string so the mutation is reproducible from the
/// seed alone, and so an unknown-field finding names a key a reviewer can grep for.
const UNKNOWN_FIELD_NAMES: &[&str] = &[
    "deaconDiscoveryProbe",
    "xUnrecognizedSetting",
    "notASchemaProperty",
    "vendorSpecificKey",
];

/// `mop-unknown-field` — insert a key absent from the schema.
///
/// Grammar input: `additional-properties` / `property-existence` (data-model.md § 5) —
/// the schema's own statement of which keys may appear at a pointer, and therefore of
/// which keys are the near-valid one-edit-away set.
fn unknown_field(
    object: &Map<String, Value>,
    prng: &mut Prng,
) -> Option<(Map<String, Value>, String)> {
    let free: Vec<&&str> = UNKNOWN_FIELD_NAMES
        .iter()
        .filter(|name| !object.contains_key(**name))
        .collect();
    let name = **prng.choose(&free)?;
    let value = scalar_draw(prng);
    let mut out = object.clone();
    out.insert(name.to_string(), value);
    Some((
        out,
        format!("inserted unknown key `{name}` at the document root"),
    ))
}

/// `mop-wrong-type` — replace a value with one of a different JSON type.
///
/// Grammar input: `type`. The replacement is *type*-directed, not random: swapping a
/// string for another string would be a value change, and this category exists to probe
/// the boundary the `type` constraint draws.
fn wrong_type(
    object: &Map<String, Value>,
    prng: &mut Prng,
) -> Option<(Map<String, Value>, String)> {
    let key = pick_key(object, prng, |_, _| true)?;
    let current = object.get(&key)?;
    let replacement = other_type(current);
    let mut out = object.clone();
    let from = json_type(current);
    let to = json_type(&replacement);
    out.insert(key.clone(), replacement);
    Some((out, format!("retyped `{key}` from {from} to {to}")))
}

/// `mop-null-value` — replace a value with `null`.
///
/// Distinct from `empty-value` on purpose: the spec distinguishes an authored `null` from
/// an omission and from an empty collection, and this repository has already been bitten
/// by a normalization that conflated the three (023 T062's `prune`).
fn null_value(
    object: &Map<String, Value>,
    prng: &mut Prng,
) -> Option<(Map<String, Value>, String)> {
    let key = pick_key(object, prng, |_, v| !v.is_null())?;
    let mut out = object.clone();
    out.insert(key.clone(), Value::Null);
    Some((out, format!("set `{key}` to null")))
}

/// `mop-empty-value` — empty a collection or string **in place**, preserving its type.
///
/// Grammar input: `array-shape` / `type`.
fn empty_value(
    object: &Map<String, Value>,
    prng: &mut Prng,
) -> Option<(Map<String, Value>, String)> {
    let key = pick_key(object, prng, |_, v| match v {
        Value::Array(a) => !a.is_empty(),
        Value::Object(o) => !o.is_empty(),
        Value::String(s) => !s.is_empty(),
        _ => false,
    })?;
    let emptied = match object.get(&key)? {
        Value::Array(_) => Value::Array(Vec::new()),
        Value::Object(_) => Value::Object(Map::new()),
        _ => Value::String(String::new()),
    };
    let mut out = object.clone();
    let shape = json_type(&emptied);
    out.insert(key.clone(), emptied);
    Some((out, format!("emptied `{key}` in place (still {shape})")))
}

/// The four mutually-exclusive configuration sources the schema's `oneOf` branches
/// declare (`imageContainer` / `dockerfileContainer` × 2 spellings / `composeContainer`).
const CONFIG_SOURCE_KEYS: &[&str] = &["image", "dockerFile", "build", "dockerComposeFile"];

/// `mop-conflicting-source` — add a *second* configuration source.
///
/// Grammar input: `union-alternative`. The schema's `oneOf` says exactly one branch may
/// match; satisfying two is the near-valid input that asks both implementations what they
/// do when the union is violated — a question no curated fixture asks, because a fixture
/// author writes configurations that work.
fn conflicting_source(
    object: &Map<String, Value>,
    prng: &mut Prng,
) -> Option<(Map<String, Value>, String)> {
    let present: Vec<&str> = CONFIG_SOURCE_KEYS
        .iter()
        .copied()
        .filter(|k| object.contains_key(*k))
        .collect();
    let absent: Vec<&str> = CONFIG_SOURCE_KEYS
        .iter()
        .copied()
        .filter(|k| !object.contains_key(*k))
        .collect();
    let added = *prng.choose(&absent)?;
    let value = match added {
        "image" => Value::String("alpine:3.19".to_string()),
        "dockerFile" => Value::String("Dockerfile".to_string()),
        "build" => serde_json::json!({ "dockerfile": "Dockerfile" }),
        _ => Value::String("docker-compose.yml".to_string()),
    };
    let mut out = object.clone();
    out.insert(added.to_string(), value);
    let existing = if present.is_empty() {
        "no other source".to_string()
    } else {
        present.join(" + ")
    };
    Some((out, format!("added source `{added}` alongside {existing}")))
}

/// One way to corrupt a Feature identifier: a label for the witness, and the rewrite.
///
/// A named type rather than an inline tuple so the corruption table reads as the catalogue
/// it is, and so a reader sees that the label and the rewrite are one record.
type FeatureIdCorruption = (&'static str, fn(&str) -> String);

/// Corruptions of a Feature identifier's registry / path / tag shape.
const FEATURE_ID_CORRUPTIONS: &[FeatureIdCorruption] = &[
    ("dropped the tag", |id| {
        id.rsplit_once(':')
            .map(|(l, _)| l.to_string())
            .unwrap_or_else(|| id.to_string())
    }),
    ("doubled a registry dot", |id| id.replacen('.', "..", 1)),
    ("stripped the registry and path", |id| {
        id.rsplit('/').next().unwrap_or(id).to_string()
    }),
    ("appended an empty tag", |id| format!("{id}:")),
];

/// `mop-invalid-feature-id` — corrupt a Feature identifier's registry/path/tag shape.
///
/// Grammar input: `property-existence` under `features`. A Feature id is not a schema
/// *type* — the schema says only that `features` is an object — so its shape is enforced
/// by the resolver rather than the validator, which makes it a place the two
/// implementations can genuinely disagree without either violating the schema.
fn invalid_feature_id(
    object: &Map<String, Value>,
    prng: &mut Prng,
) -> Option<(Map<String, Value>, String)> {
    let mut out = object.clone();
    let existing: Vec<(String, Value)> = match object.get("features") {
        Some(Value::Object(f)) if !f.is_empty() => {
            f.iter().map(|(k, v)| (k.clone(), v.clone())).collect()
        }
        _ => Vec::new(),
    };

    if existing.is_empty() {
        // No feature to corrupt: introduce one whose id is already malformed. This is
        // still an invalid-feature-id mutation — the operator's subject is the identifier,
        // not a pre-existing entry.
        let mut features = Map::new();
        features.insert("not a feature id".to_string(), serde_json::json!({}));
        out.insert("features".to_string(), Value::Object(features));
        return Some((
            out,
            "introduced `features` with the malformed id `not a feature id`".to_string(),
        ));
    }

    let index = prng.next_index(existing.len())?;
    let (target, value) = &existing[index];
    let corruption_index = prng.next_index(FEATURE_ID_CORRUPTIONS.len())?;
    let (label, corrupt) = FEATURE_ID_CORRUPTIONS[corruption_index];
    let corrupted = corrupt(target);

    let mut features = Map::new();
    for (k, v) in &existing {
        if k == target {
            features.insert(corrupted.clone(), value.clone());
        } else {
            features.insert(k.clone(), v.clone());
        }
    }
    out.insert("features".to_string(), Value::Object(features));
    Some((
        out,
        format!("{label} on feature id `{target}` → `{corrupted}`"),
    ))
}

/// `mop-extends-cycle` — introduce a cycle into an `extends` chain.
///
/// Grammar input: `property-existence`. Both the self-reference and the two-hop cycle are
/// reachable: the self-reference is the shortest cycle a resolver must detect, and the
/// two-hop form is the shortest one a naive "is it me?" check misses.
fn extends_cycle(
    object: &Map<String, Value>,
    prng: &mut Prng,
) -> Option<(Map<String, Value>, String)> {
    let (value, detail) = if prng.next_bool() {
        (
            Value::String("./devcontainer.json".to_string()),
            "self-referential `extends` (the shortest cycle)",
        )
    } else {
        (
            Value::Array(vec![
                Value::String("./devcontainer.json".to_string()),
                Value::String("./devcontainer.json".to_string()),
            ]),
            "two-hop `extends` cycle through the document itself",
        )
    };
    if object.get("extends") == Some(&value) {
        // Already exactly this cycle: applying it again would record a mutation that
        // changed nothing, and an application count that includes no-ops is not a count
        // of anything.
        return None;
    }
    let mut out = object.clone();
    out.insert("extends".to_string(), value);
    Some((out, detail.to_string()))
}

/// `${…}` edge cases: nesting, self-reference, and non-termination.
const SUBSTITUTION_EDGES: &[(&str, &str)] = &[
    ("nested", "${localEnv:${localEnv:DEACON_DISCOVERY}}"),
    ("unterminated", "${containerWorkspaceFolder"),
    (
        "self-referential",
        "${localWorkspaceFolder}${localWorkspaceFolder}",
    ),
    ("empty token", "${}"),
    ("unknown scope", "${notAScope:VALUE}"),
];

/// `mop-substitution-edge` — nest, self-reference, or leave unterminated a `${…}` token.
///
/// Grammar input: `type` restricted to string-valued fields. Substitution is applied by
/// the consumer, not by the schema, so this is another place two conformant
/// implementations can disagree while both satisfy the pinned schema.
fn substitution_edge(
    object: &Map<String, Value>,
    prng: &mut Prng,
) -> Option<(Map<String, Value>, String)> {
    let edge_index = prng.next_index(SUBSTITUTION_EDGES.len())?;
    let (label, token) = SUBSTITUTION_EDGES[edge_index];
    let mut out = object.clone();
    match pick_key(object, prng, |_, v| v.is_string()) {
        Some(key) => {
            out.insert(key.clone(), Value::String(token.to_string()));
            Some((out, format!("{label} substitution token in `{key}`")))
        }
        None => {
            // No string-valued field to carry the token: `name` is a string on every
            // branch of the schema, so introducing it keeps the document schema-adjacent.
            out.insert("name".to_string(), Value::String(token.to_string()));
            Some((out, format!("{label} substitution token in a new `name`")))
        }
    }
}

/// The lifecycle hooks whose value may be a string, an array, or an object.
const LIFECYCLE_KEYS: &[&str] = &[
    "initializeCommand",
    "onCreateCommand",
    "updateContentCommand",
    "postCreateCommand",
    "postStartCommand",
    "postAttachCommand",
];

/// `mop-lifecycle-shape` — switch between the permitted string/array/object forms.
///
/// Grammar input: `union-alternative`. All three forms are legal, so this operator
/// produces a **valid** document whose *shape* differs — which is precisely the case
/// where a difference is a defect rather than a rejection, and precisely the shape family
/// this repository has already shipped a fix for (012-fix-lifecycle-formats).
fn lifecycle_shape(
    object: &Map<String, Value>,
    prng: &mut Prng,
) -> Option<(Map<String, Value>, String)> {
    let present: Vec<&&str> = LIFECYCLE_KEYS
        .iter()
        .filter(|k| object.contains_key(**k))
        .collect();
    let mut out = object.clone();

    let (key, current) = match prng.choose(&present) {
        Some(k) => ((**k).to_string(), object.get(**k).cloned()),
        None => {
            let index = prng.next_index(LIFECYCLE_KEYS.len())?;
            (LIFECYCLE_KEYS[index].to_string(), None)
        }
    };

    let (rotated, to) = match current.as_ref() {
        // string → array
        Some(Value::String(s)) => (Value::Array(vec![Value::String(s.clone())]), "array"),
        // array → object (one named command per element, declaration-ordered)
        Some(Value::Array(items)) => {
            let mut map = Map::new();
            for (i, item) in items.iter().enumerate() {
                map.insert(format!("step{i}"), item.clone());
            }
            if map.is_empty() {
                map.insert("step0".to_string(), Value::String("true".to_string()));
            }
            (Value::Object(map), "object")
        }
        // object → string (join the named commands)
        Some(Value::Object(map)) => {
            let joined = map
                .values()
                .map(render_command)
                .collect::<Vec<String>>()
                .join(" && ");
            let joined = if joined.is_empty() {
                "true".to_string()
            } else {
                joined
            };
            (Value::String(joined), "string")
        }
        // absent (or a shape the schema does not permit): introduce the string form
        _ => (Value::String("echo discovery".to_string()), "string"),
    };

    let from = current.as_ref().map(json_type).unwrap_or("absent");
    out.insert(key.clone(), rotated);
    Some((out, format!("lifecycle `{key}` from {from} to {to}")))
}

/// `mop-compose-combination` — vary service count, `runServices`, override-file ordering.
///
/// Grammar input: `union-alternative` + `array-shape`. `dockerComposeFile` accepts both a
/// string and an array of override files whose **order** is significant, so this operator
/// covers a shape whose semantics live entirely outside the schema.
fn compose_combination(
    object: &Map<String, Value>,
    prng: &mut Prng,
) -> Option<(Map<String, Value>, String)> {
    let mut out = object.clone();
    match object.get("dockerComposeFile") {
        Some(Value::String(single)) => {
            // string → ordered override list. The order is the semantics: later files
            // override earlier ones.
            out.insert(
                "dockerComposeFile".to_string(),
                Value::Array(vec![
                    Value::String(single.clone()),
                    Value::String("docker-compose.override.yml".to_string()),
                ]),
            );
            Some((
                out,
                "`dockerComposeFile` from a single file to an ordered override list".to_string(),
            ))
        }
        Some(Value::Array(files)) if files.len() >= 2 => {
            let mut reversed = files.clone();
            reversed.reverse();
            out.insert("dockerComposeFile".to_string(), Value::Array(reversed));
            Some((
                out,
                "reversed the `dockerComposeFile` override order".to_string(),
            ))
        }
        Some(Value::Array(files)) => {
            let mut extended = files.clone();
            extended.push(Value::String("docker-compose.override.yml".to_string()));
            out.insert("dockerComposeFile".to_string(), Value::Array(extended));
            Some((out, "appended a `dockerComposeFile` override".to_string()))
        }
        _ => {
            // No Compose source: introduce one, with the two keys `composeContainer`
            // additionally requires so the result stays schema-adjacent rather than
            // becoming an obviously-incomplete branch.
            out.insert(
                "dockerComposeFile".to_string(),
                Value::String("docker-compose.yml".to_string()),
            );
            out.insert("service".to_string(), Value::String("app".to_string()));
            if !out.contains_key("workspaceFolder") {
                out.insert(
                    "workspaceFolder".to_string(),
                    Value::String("/workspace".to_string()),
                );
            }
            let services = if prng.next_bool() {
                Value::Array(vec![Value::String("db".to_string())])
            } else {
                Value::Array(vec![
                    Value::String("db".to_string()),
                    Value::String("cache".to_string()),
                ])
            };
            let count = services.as_array().map(Vec::len).unwrap_or(0);
            out.insert("runServices".to_string(), services);
            Some((
                out,
                format!("introduced a Compose source with {count} `runServices`"),
            ))
        }
    }
}

/// `mop-ordering-change` — permute a declaration-ordered collection.
///
/// Grammar input: `array-shape`. Returns `None` when the document holds no collection with
/// at least two members: there is no order to change, and manufacturing one would record
/// an ordering mutation that never happened. That honesty is what makes FR-010's
/// zero-count report mean something.
///
/// The permutation is guaranteed to *differ* from the input — a shuffle that happens to
/// return the identity is rotated — because a mutation that changed nothing would inflate
/// the application count without exploring anything.
fn ordering_change(
    object: &Map<String, Value>,
    prng: &mut Prng,
) -> Option<(Map<String, Value>, String)> {
    let mut candidates: Vec<String> = object
        .iter()
        .filter(|(_, v)| matches!(v, Value::Array(a) if a.len() >= 2))
        .map(|(k, _)| k.clone())
        .collect();
    candidates.sort_unstable();
    let key = prng.choose(&candidates)?.clone();
    let Some(Value::Array(items)) = object.get(&key) else {
        return None;
    };

    let mut permuted = items.clone();
    prng.shuffle(&mut permuted);
    if permuted == *items {
        permuted.rotate_left(1);
    }

    let mut out = object.clone();
    let len = permuted.len();
    out.insert(key.clone(), Value::Array(permuted));
    Some((out, format!("permuted the {len} elements of `{key}`")))
}

// ---------------------------------------------------------------------------
// Shared helpers
// ---------------------------------------------------------------------------

/// Pick a key of `object` satisfying `predicate`, drawn uniformly from the **sorted**
/// candidate list.
///
/// Sorting matters for reproducibility: `serde_json` preserves insertion order in this
/// crate, so two documents that are `==` as maps can enumerate their keys differently, and
/// drawing from the enumeration order would make the same seed pick different targets for
/// documents the comparison calls identical.
fn pick_key(
    object: &Map<String, Value>,
    prng: &mut Prng,
    predicate: impl Fn(&str, &Value) -> bool,
) -> Option<String> {
    let mut keys: Vec<&String> = object
        .iter()
        .filter(|(k, v)| predicate(k.as_str(), v))
        .map(|(k, _)| k)
        .collect();
    keys.sort_unstable();
    prng.choose(&keys).map(|k| (*k).clone())
}

/// A value of a **different** JSON type than `current`, chosen deterministically.
fn other_type(current: &Value) -> Value {
    match current {
        Value::String(_) => Value::Number(42.into()),
        Value::Number(_) => Value::String("42".to_string()),
        Value::Bool(b) => Value::String(b.to_string()),
        Value::Array(_) => Value::String("joined,as,a,string".to_string()),
        Value::Object(_) => Value::Array(vec![Value::String("was-an-object".to_string())]),
        Value::Null => Value::Number(0.into()),
    }
}

/// A scalar draw for the unknown-field operator, covering every scalar type so an unknown
/// key is not always a string.
fn scalar_draw(prng: &mut Prng) -> Value {
    match prng.next_bounded(4).unwrap_or(0) {
        0 => Value::String("discovery".to_string()),
        1 => Value::Number(7.into()),
        2 => Value::Bool(true),
        _ => Value::Null,
    }
}

/// The JSON type name, for readable mutation details.
fn json_type(v: &Value) -> &'static str {
    match v {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

/// Render a lifecycle command element as a shell-ish string for the object → string
/// rotation.
fn render_command(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Array(items) => items
            .iter()
            .map(render_command)
            .collect::<Vec<String>>()
            .join(" "),
        other => other.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// A document rich enough that every operator has a target: a config source, a
    /// feature, a lifecycle hook, a string field, and two multi-element arrays.
    fn rich() -> Value {
        json!({
            "name": "Discovery Seed",
            "image": "alpine:3.19",
            "features": { "ghcr.io/devcontainers/features/git:1": { "version": "os-provided" } },
            "forwardPorts": [3000, 8080],
            "runArgs": ["--init", "--rm"],
            "containerEnv": { "A": "1", "B": "2" },
            "postCreateCommand": "echo hello",
            "remoteUser": "vscode"
        })
    }

    fn apply_ok(category: MutationCategory, doc: &Value, seed: u64) -> Applied {
        let mut prng = Prng::from_seed(seed);
        apply(category, doc, &mut prng)
            .unwrap_or_else(|| panic!("{} must apply to the rich document", category.name()))
    }

    // --- catalogue identity -------------------------------------------------

    #[test]
    fn the_catalogue_declares_exactly_eleven_categories() {
        // FR-008 mandates eleven. Asserting the arity here means adding a twelfth is a
        // deliberate act that also has to update `MUTATION_CATALOG_VERSION`, rather than
        // a quiet append nobody notices in the outcome map.
        assert_eq!(MutationCategory::all().len(), CATEGORY_COUNT);
        assert_eq!(CATEGORY_COUNT, 11);
        assert_eq!(category_names().len(), 11);
    }

    #[test]
    fn names_and_operator_ids_are_unique_and_round_trip() {
        let mut names: Vec<&str> = category_names();
        let mut ids: Vec<&str> = MutationCategory::all()
            .iter()
            .map(|c| c.operator_id())
            .collect();
        names.sort_unstable();
        ids.sort_unstable();
        let unique_names = {
            let mut d = names.clone();
            d.dedup();
            d.len()
        };
        let unique_ids = {
            let mut d = ids.clone();
            d.dedup();
            d.len()
        };
        assert_eq!(unique_names, 11, "two categories share a name");
        assert_eq!(unique_ids, 11, "two categories share a `mop-` id");

        for category in MutationCategory::all() {
            assert_eq!(MutationCategory::parse(category.name()), Some(*category));
            assert_eq!(
                MutationCategory::parse_operator(category.operator_id()),
                Some(*category)
            );
            assert_eq!(
                category.operator_id(),
                format!("mop-{}", category.name()),
                "the operator id is the declared name, prefixed — never an independent \
                 spelling that could drift from it"
            );
        }
        assert_eq!(MutationCategory::parse("not-a-category"), None);
        assert_eq!(MutationCategory::parse_operator("wrong-type"), None);
    }

    #[test]
    fn the_empty_count_map_carries_every_key_at_zero_in_declaration_order() {
        // FR-010: a category absent from the map is indistinguishable from a category
        // that was never applied. This map is the single source every producer starts
        // from, so the property is established once rather than re-established per caller.
        let counts = empty_application_counts();
        assert_eq!(counts.len(), 11);
        let keys: Vec<&str> = counts.keys().map(String::as_str).collect();
        assert_eq!(
            keys,
            category_names(),
            "declaration order, not sorted order"
        );
        assert!(counts.values().all(|&n| n == 0));
        assert_eq!(unapplied_categories(&counts).len(), 11);

        let mut applied = counts.clone();
        for name in category_names() {
            applied.insert(name.to_string(), 1);
        }
        assert!(unapplied_categories(&applied).is_empty());

        let mut partial = counts;
        partial.insert("wrong-type".to_string(), 3);
        assert_eq!(unapplied_categories(&partial).len(), 10);
        assert!(!unapplied_categories(&partial).contains(&"wrong-type"));
    }

    // --- T032: per-operator behavior ---------------------------------------

    #[test]
    fn every_operator_applies_to_a_rich_document_and_stays_schema_adjacent() {
        // The whole point of structural mutation (research D5's argument, applied at
        // generation time): a mutated candidate must still PARSE, or the campaign spends
        // its budget on the document-syntax stage instead of on configuration
        // resolution — the pathology SC-002 caps at 10%.
        let doc = rich();
        for category in MutationCategory::all() {
            let applied = apply_ok(*category, &doc, 0x5EED_0001);
            assert!(
                applied.document.is_object(),
                "{} produced a non-object document",
                category.name()
            );
            let rendered = serde_json::to_string(&applied.document).unwrap_or_else(|e| {
                panic!("{} produced unserializable JSON: {e}", category.name())
            });
            let reparsed: Value = serde_json::from_str(&rendered).unwrap_or_else(|e| {
                panic!(
                    "{} produced JSON that does not round-trip: {e}",
                    category.name()
                )
            });
            assert_eq!(reparsed, applied.document);
            assert_ne!(
                applied.document,
                doc,
                "{} changed nothing — an application count that includes no-ops counts \
                 nothing",
                category.name()
            );
            assert_eq!(applied.mutation.category, *category);
            assert_eq!(applied.mutation.operator, category.operator_id());
            assert!(
                !applied.mutation.detail.is_empty(),
                "{} recorded no detail; a witness must name what was applied where",
                category.name()
            );
        }
    }

    #[test]
    fn unknown_field_inserts_a_key_the_schema_does_not_declare() {
        let doc = rich();
        let applied = apply_ok(MutationCategory::UnknownField, &doc, 11);
        let out = applied.document.as_object().expect("object");
        let added: Vec<&String> = out
            .keys()
            .filter(|k| !doc.as_object().expect("object").contains_key(*k))
            .collect();
        assert_eq!(added.len(), 1, "exactly one key added");
        assert!(
            UNKNOWN_FIELD_NAMES.contains(&added[0].as_str()),
            "the inserted key must come from the declared pool so a finding names a key \
             a reviewer can grep for"
        );
    }

    #[test]
    fn wrong_type_changes_the_json_type_and_nothing_else() {
        let doc = rich();
        let applied = apply_ok(MutationCategory::WrongType, &doc, 22);
        let before = doc.as_object().expect("object");
        let after = applied.document.as_object().expect("object");
        assert_eq!(before.len(), after.len(), "no key added or removed");
        let changed: Vec<&String> = after
            .keys()
            .filter(|k| before.get(*k) != after.get(*k))
            .collect();
        assert_eq!(changed.len(), 1);
        let key = changed[0];
        assert_ne!(
            json_type(&before[key]),
            json_type(&after[key]),
            "a wrong-TYPE mutation that keeps the type is a value change wearing the \
             wrong operator's name"
        );
    }

    #[test]
    fn null_value_and_empty_value_are_not_the_same_mutation() {
        // The spec distinguishes an authored `null` from an authored empty collection
        // from an omission; conflating them is the defect 023 T062 removed from the
        // normalizer, and re-introducing it in the generator would be the same mistake
        // one stage earlier.
        let doc = rich();
        let nulled = apply_ok(MutationCategory::NullValue, &doc, 33).document;
        let emptied = apply_ok(MutationCategory::EmptyValue, &doc, 33).document;

        let nulled_keys: Vec<&String> = nulled
            .as_object()
            .expect("object")
            .iter()
            .filter(|(_, v)| v.is_null())
            .map(|(k, _)| k)
            .collect();
        assert_eq!(nulled_keys.len(), 1, "exactly one key became null");

        let before = doc.as_object().expect("object");
        let after = emptied.as_object().expect("object");
        let changed: Vec<&String> = after
            .keys()
            .filter(|k| before.get(*k) != after.get(*k))
            .collect();
        assert_eq!(changed.len(), 1);
        let key = changed[0];
        assert!(!after[key].is_null(), "empty-value must not produce a null");
        assert_eq!(
            json_type(&before[key]),
            json_type(&after[key]),
            "emptied IN PLACE: the type is preserved"
        );
        let is_empty = match &after[key] {
            Value::Array(a) => a.is_empty(),
            Value::Object(o) => o.is_empty(),
            Value::String(s) => s.is_empty(),
            other => panic!("unexpected emptied shape {other}"),
        };
        assert!(is_empty);
    }

    #[test]
    fn conflicting_source_leaves_two_sources_declared() {
        let doc = rich(); // carries `image`
        let applied = apply_ok(MutationCategory::ConflictingSource, &doc, 44);
        let out = applied.document.as_object().expect("object");
        let sources = CONFIG_SOURCE_KEYS
            .iter()
            .filter(|k| out.contains_key(**k))
            .count();
        assert!(
            sources >= 2,
            "the schema's oneOf permits exactly one source; violating it is the whole \
             point of this operator, and it needs at least two present to do so"
        );
    }

    #[test]
    fn invalid_feature_id_corrupts_the_identifier_and_keeps_the_options() {
        let doc = rich();
        let applied = apply_ok(MutationCategory::InvalidFeatureId, &doc, 55);
        let features = applied.document["features"].as_object().expect("features");
        assert_eq!(features.len(), 1, "the entry is corrupted, not duplicated");
        let (id, options) = features.iter().next().expect("one entry");
        assert_ne!(id, "ghcr.io/devcontainers/features/git:1");
        assert_eq!(
            options,
            &json!({ "version": "os-provided" }),
            "the identifier is the subject; the options ride along unchanged"
        );

        // With no `features` at all, the operator introduces a malformed one rather than
        // declining: the identifier is its subject whether or not one already exists.
        let bare = json!({ "image": "alpine:3.19" });
        let applied = apply_ok(MutationCategory::InvalidFeatureId, &bare, 55);
        assert!(applied.document["features"].is_object());
    }

    #[test]
    fn extends_cycle_produces_a_cycle_and_declines_to_re_apply_the_same_one() {
        let doc = rich();
        let applied = apply_ok(MutationCategory::ExtendsCycle, &doc, 66);
        let extends = &applied.document["extends"];
        let refers_to_self = match extends {
            Value::String(s) => s == "./devcontainer.json",
            Value::Array(items) => items.iter().all(|v| v == "./devcontainer.json"),
            other => panic!("unexpected extends shape {other}"),
        };
        assert!(refers_to_self);

        // Re-applying the identical cycle changes nothing, so the operator declines
        // rather than inflating the application count with a no-op.
        let mut prng = Prng::from_seed(66);
        assert!(
            apply(MutationCategory::ExtendsCycle, &applied.document, &mut prng).is_none(),
            "an operator that changes nothing must not report an application"
        );
    }

    #[test]
    fn substitution_edge_writes_a_declared_edge_token() {
        let doc = rich();
        let applied = apply_ok(MutationCategory::SubstitutionEdge, &doc, 77);
        let out = applied.document.as_object().expect("object");
        let tokens: Vec<&str> = SUBSTITUTION_EDGES.iter().map(|(_, t)| *t).collect();
        assert!(
            out.values()
                .any(|v| v.as_str().is_some_and(|s| tokens.contains(&s))),
            "the mutated document must carry one of the declared edge tokens"
        );

        // A document with no string field still gets one: `name` is a string on every
        // branch of the schema, so the result stays schema-adjacent.
        let no_strings = json!({ "forwardPorts": [1, 2] });
        let applied = apply_ok(MutationCategory::SubstitutionEdge, &no_strings, 77);
        assert!(applied.document["name"].is_string());
    }

    #[test]
    fn lifecycle_shape_rotates_string_to_array_to_object_to_string() {
        let string_form = json!({ "image": "alpine:3.19", "postCreateCommand": "echo hi" });
        let as_array = apply_ok(MutationCategory::LifecycleShape, &string_form, 88).document;
        assert!(
            as_array["postCreateCommand"].is_array(),
            "string rotates to array"
        );

        let as_object = apply_ok(MutationCategory::LifecycleShape, &as_array, 88).document;
        assert!(
            as_object["postCreateCommand"].is_object(),
            "array rotates to object"
        );

        let back_to_string = apply_ok(MutationCategory::LifecycleShape, &as_object, 88).document;
        assert!(
            back_to_string["postCreateCommand"].is_string(),
            "object rotates back to string — the three forms are a cycle, so a campaign \
             reaches all of them"
        );

        // All three forms are legal, so every rotation is a VALID document. That is what
        // makes this category able to surface a defect rather than a rejection.
        for doc in [&as_array, &as_object, &back_to_string] {
            assert!(doc.is_object());
        }
    }

    #[test]
    fn compose_combination_varies_the_override_list_and_its_order() {
        let single = json!({
            "dockerComposeFile": "docker-compose.yml",
            "service": "app",
            "workspaceFolder": "/workspace"
        });
        let as_list = apply_ok(MutationCategory::ComposeCombination, &single, 99).document;
        let files = as_list["dockerComposeFile"].as_array().expect("array");
        assert_eq!(
            files.len(),
            2,
            "a single file becomes an ordered override list"
        );

        let reversed = apply_ok(MutationCategory::ComposeCombination, &as_list, 99).document;
        let reversed_files = reversed["dockerComposeFile"].as_array().expect("array");
        assert_eq!(
            reversed_files,
            &files.iter().rev().cloned().collect::<Vec<Value>>(),
            "override ORDER is the semantics — later files override earlier ones — so \
             reversing it is a real mutation, not a cosmetic one"
        );

        // A non-Compose document gains a complete Compose branch, not a half of one.
        let image = json!({ "image": "alpine:3.19" });
        let composed = apply_ok(MutationCategory::ComposeCombination, &image, 99).document;
        assert!(composed["dockerComposeFile"].is_string());
        assert!(composed["service"].is_string());
        assert!(composed["workspaceFolder"].is_string());
        assert!(composed["runServices"].is_array());
    }

    #[test]
    fn ordering_change_permutes_a_collection_and_never_returns_the_identity() {
        let doc = rich();
        let applied = apply_ok(MutationCategory::OrderingChange, &doc, 1234);
        let before = doc.as_object().expect("object");
        let after = applied.document.as_object().expect("object");
        let changed: Vec<&String> = after
            .keys()
            .filter(|k| before.get(*k) != after.get(*k))
            .collect();
        assert_eq!(changed.len(), 1);
        let key = changed[0];

        let mut before_sorted: Vec<String> = before[key]
            .as_array()
            .expect("array")
            .iter()
            .map(|v| v.to_string())
            .collect();
        let mut after_sorted: Vec<String> = after[key]
            .as_array()
            .expect("array")
            .iter()
            .map(|v| v.to_string())
            .collect();
        before_sorted.sort();
        after_sorted.sort();
        assert_eq!(
            before_sorted, after_sorted,
            "a permutation, not a rewrite: the multiset must be preserved"
        );
        assert_ne!(
            before[key], after[key],
            "a shuffle that returned the identity is rotated, because a mutation that \
             changed nothing would inflate the count without exploring anything"
        );
    }

    #[test]
    fn ordering_change_declines_when_there_is_no_order_to_change() {
        // The honesty that makes FR-010's zero-count report mean something: rather than
        // manufacture a collection to permute, the operator reports that it did not apply.
        let flat = json!({ "image": "alpine:3.19", "name": "flat", "forwardPorts": [3000] });
        let mut prng = Prng::from_seed(7);
        assert!(apply(MutationCategory::OrderingChange, &flat, &mut prng).is_none());
    }

    #[test]
    fn a_non_object_document_has_nothing_to_mutate() {
        let mut prng = Prng::from_seed(1);
        for doc in [json!([1, 2, 3]), json!("string"), json!(null)] {
            for category in MutationCategory::all() {
                assert!(
                    apply(*category, &doc, &mut prng).is_none(),
                    "{} claimed to mutate a non-object",
                    category.name()
                );
            }
        }
    }

    // --- reproducibility ----------------------------------------------------

    #[test]
    fn the_same_seed_and_document_reproduce_the_same_mutation() {
        // FR-001 in miniature at the operator level: without this, a recorded seed could
        // not reproduce a candidate even with an identical generator.
        let doc = rich();
        for category in MutationCategory::all() {
            let mut a = Prng::from_seed(0xC0FF_EE01);
            let mut b = Prng::from_seed(0xC0FF_EE01);
            assert_eq!(
                apply(*category, &doc, &mut a),
                apply(*category, &doc, &mut b),
                "{} is not reproducible from its seed",
                category.name()
            );
        }
    }

    #[test]
    fn target_selection_does_not_depend_on_key_insertion_order() {
        // `serde_json` preserves insertion order in this crate, so two documents that are
        // `==` as maps can enumerate their keys differently. Drawing from the enumeration
        // order would make the same seed pick different targets for documents the
        // comparison itself calls identical — a reproducibility hole with no symptom
        // until a finding stopped reproducing for no visible reason.
        let a = json!({ "image": "alpine:3.19", "name": "x", "remoteUser": "vscode" });
        let b = json!({ "remoteUser": "vscode", "name": "x", "image": "alpine:3.19" });
        assert_eq!(a, b, "serde_json compares objects as maps");

        for category in [
            MutationCategory::WrongType,
            MutationCategory::NullValue,
            MutationCategory::SubstitutionEdge,
        ] {
            let left = apply_ok(category, &a, 0xDEAD_BEEF);
            let right = apply_ok(category, &b, 0xDEAD_BEEF);
            assert_eq!(
                left.mutation.detail,
                right.mutation.detail,
                "{} picked a different target for two equal documents",
                category.name()
            );
            assert_eq!(left.document, right.document);
        }
    }
}
