//! The normalized signature — the deduplication key
//! (025-exploratory-parity-discovery, research D3, data-model.md § 2, T014/T015).
//!
//! A signature is `(channel, observable path, difference kind, value-shape class)`,
//! hashed into a substance-anchored `sig-<hash8>` id. It is **derived** from the
//! comparison's own diff output and never re-computed from the two documents.
//!
//! ## Why derivation, not a second diff
//!
//! FR-015 permits exactly one normalization definition. A signature computed by
//! independently re-diffing the two sides would be a second opinion on *what differs*,
//! able to disagree with the one the comparison actually used — the identical defect
//! class the single-normalizer rule exists to prevent. Deriving from the comparison's
//! output makes disagreement structurally impossible: there is nothing to disagree with.
//!
//! [`Divergence`] is therefore an **input shape**, not a differ. It carries exactly the
//! four fields `parity_harness::normalize::ConfigDivergence` already produces — kind,
//! path, and the two `Option<Value>`s — and this module never inspects anything else.
//!
//! ## Why this type lives here rather than importing `ConfigDivergence`
//!
//! `parity-harness` depends on `deacon-conformance`, not the other way round (the
//! hermetic half must stay loadable without the live half), so importing
//! `ConfigDivergence` here would be a dependency cycle. The live side adapts its
//! `ConfigDivergence` into [`Divergence`] at the single call site in
//! `parity_harness::discovery::differential` (T035) — a field-for-field move with no
//! recomputation, which is what keeps "derive only, never re-diff" true in the code and
//! not merely in the comment.
//!
//! ## Why concrete values are not in the signature
//!
//! Structure alone (channel + path + kind) merges a *missing* `remoteUser` with a
//! *wrongly-typed* `remoteUser`; including concrete values splits one defect across
//! every generated value and makes deduplication do nothing, so the queue would grow
//! with campaign volume. The value-*shape* class is the level at which "same defect" is
//! true. The concrete observed values are retained on the witness, where they are
//! evidence rather than identity.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::hash8;

/// The difference kind, mirroring `parity_harness::normalize::DiffKind` **exactly**,
/// including its wire spellings.
///
/// Kept as a closed enum rather than a bare string so an adapter cannot quietly
/// introduce a fourth kind: the diff produces three, the signature recognizes three, and
/// a fourth would have to be added here deliberately.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum DivergenceKind {
    /// Present on the reference side only.
    RefOnly,
    /// Present on the deacon side only.
    DeaconOnly,
    /// Present on both sides with different values.
    Value,
}

impl DivergenceKind {
    /// The stable wire spelling — byte-identical to `DiffKind::as_str()`.
    pub fn as_str(self) -> &'static str {
        match self {
            DivergenceKind::RefOnly => "ref-only",
            DivergenceKind::DeaconOnly => "deacon-only",
            DivergenceKind::Value => "value",
        }
    }

    /// Parse a wire spelling produced by `DiffKind::as_str()`.
    ///
    /// Returns `None` on anything else rather than defaulting to a kind: a silently
    /// mis-parsed kind would merge two genuinely different defect families under one
    /// signature (constitution IV).
    pub fn parse(s: &str) -> Option<DivergenceKind> {
        match s {
            "ref-only" => Some(DivergenceKind::RefOnly),
            "deacon-only" => Some(DivergenceKind::DeaconOnly),
            "value" => Some(DivergenceKind::Value),
            _ => None,
        }
    }
}

/// The value-shape class — the one derivation this feature adds
/// (data-model.md § 2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ValueShapeClass {
    /// One side has the value and the other does not.
    PresentAbsent,
    /// Both sides have a value, of different JSON types.
    TypeChanged,
    /// Both sides have an array, and one is a permutation of the other.
    OrderingChanged,
    /// Both sides have a same-typed value that differs some other way.
    ValueChanged,
}

impl ValueShapeClass {
    /// The stable wire spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            ValueShapeClass::PresentAbsent => "present-absent",
            ValueShapeClass::TypeChanged => "type-changed",
            ValueShapeClass::OrderingChanged => "ordering-changed",
            ValueShapeClass::ValueChanged => "value-changed",
        }
    }
}

/// The input shape a signature is derived from: exactly the fields
/// `parity_harness::normalize::ConfigDivergence` produces.
///
/// Borrowed rather than owned so the adapter is a zero-copy field-for-field move and no
/// caller is tempted to build one from anything other than the diff's own output.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Divergence<'a> {
    /// `ConfigDivergence::kind`.
    pub kind: DivergenceKind,
    /// `ConfigDivergence::path`, verbatim.
    pub path: &'a str,
    /// `ConfigDivergence::deacon`.
    pub deacon: Option<&'a Value>,
    /// `ConfigDivergence::reference`.
    pub reference: Option<&'a Value>,
}

/// The deduplication key (data-model.md § 2).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Signature {
    /// `sig-<hash8>` over `channel ‖ path ‖ kind ‖ valueShapeClass`.
    pub id: String,
    /// One of the declared observable channels (`chan-…`). Supplied by the caller: the
    /// observers already partition evidence this way, and a channel a signature invents
    /// is **D1**.
    pub channel: String,
    /// The observable path within that channel, verbatim from the diff.
    pub path: String,
    /// The difference kind.
    pub kind: DivergenceKind,
    /// The value-shape class.
    pub value_shape_class: ValueShapeClass,
}

impl Signature {
    /// Derive a signature from a channel and one divergence.
    ///
    /// Pure and total: every divergence classifies, so there is no "unclassifiable"
    /// escape hatch that could quietly drop a real difference.
    pub fn derive(channel: &str, divergence: &Divergence<'_>) -> Signature {
        let value_shape_class = classify(divergence);
        let id = signature_id(channel, divergence.path, divergence.kind, value_shape_class);
        Signature {
            id,
            channel: channel.to_string(),
            path: divergence.path.to_string(),
            kind: divergence.kind,
            value_shape_class,
        }
    }

    /// Recompute this signature's id from its own fields.
    ///
    /// The identity check: a hand-edited queue record whose `id` no longer matches its
    /// substance is **D1**, and this is what detects it.
    pub fn derived_id(&self) -> String {
        signature_id(&self.channel, &self.path, self.kind, self.value_shape_class)
    }

    /// The `fnd-<hash8>` id of the finding this signature keys.
    ///
    /// A finding's id is *derived* from its signature rather than independently
    /// assigned, which makes duplicate findings unrepresentable: FR-030 says two
    /// findings with the same signature **are** one finding, and an independently
    /// assigned id would leave that invariant to be maintained by the merge logic — and
    /// therefore violable by a bad merge (data-model.md § 1).
    ///
    /// The `fnd-` hash is taken over the signature's `id` rather than re-hashing its
    /// four fields, so the 1:1 correspondence is visible in the derivation itself.
    pub fn finding_id(&self) -> String {
        format!("fnd-{}", hash8(&[&self.id]))
    }
}

/// Classify a divergence's value shape (data-model.md § 2's table).
///
/// `ordering-changed` is tested **before** `value-changed` and is a distinct class
/// rather than a subcase, because declaration-order defects are a known recurring family
/// in this codebase (`BTreeMap` where the spec requires declaration order). Folding them
/// into `value-changed` would merge an order defect with an unrelated value defect at
/// the same path, and a merged finding cannot be split back into its causes because the
/// distinguishing information was never recorded.
pub fn classify(divergence: &Divergence<'_>) -> ValueShapeClass {
    match divergence.kind {
        // The diff only emits these when exactly one side has the value, so presence is
        // the whole difference regardless of what the present value happens to be.
        DivergenceKind::RefOnly | DivergenceKind::DeaconOnly => ValueShapeClass::PresentAbsent,
        DivergenceKind::Value => match (divergence.deacon, divergence.reference) {
            (Some(d), Some(r)) => classify_values(d, r),
            // A `Value` divergence missing a side is a malformed input rather than a
            // shape: treat it as the presence difference it describes rather than
            // inventing a value comparison out of a `None`.
            _ => ValueShapeClass::PresentAbsent,
        },
    }
}

/// Classify two present, differing values.
fn classify_values(deacon: &Value, reference: &Value) -> ValueShapeClass {
    if json_type(deacon) != json_type(reference) {
        return ValueShapeClass::TypeChanged;
    }
    if let (Value::Array(d), Value::Array(r)) = (deacon, reference)
        && is_permutation(d, r)
    {
        return ValueShapeClass::OrderingChanged;
    }
    ValueShapeClass::ValueChanged
}

/// The JSON type name, for the type-changed test.
///
/// Numbers are one type: `1` versus `1.0` is a serialization detail of the producer, not
/// a type difference either implementation chose, and classifying it as `type-changed`
/// would split a value defect into a phantom type defect.
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

/// Whether `a` and `b` hold the same multiset of elements in a different order.
///
/// A **multiset** comparison, not a set comparison: `[1, 1, 2]` and `[1, 2, 2]` are the
/// same *set* but are not a permutation of each other, and calling them an ordering
/// difference would misattribute a genuine content change to element order.
///
/// `Value` is not `Ord`, so the multiset test is done by canonical-string counting —
/// `serde_json`'s `to_string` is deterministic for a given `Value` (this crate enables
/// `preserve_order`, so object key order is the value's own and two equal `Value`s
/// always render identically).
fn is_permutation(a: &[Value], b: &[Value]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    // An empty (or single-element) array cannot be a *re*-ordering of anything: with
    // fewer than two elements there is only one order, so a difference at that path is
    // never about order. `diff` never emits equal values, so this only guards the
    // degenerate shapes.
    if a.len() < 2 {
        return false;
    }
    let mut counts: std::collections::BTreeMap<String, isize> = std::collections::BTreeMap::new();
    for v in a {
        *counts.entry(canonical_key(v)).or_insert(0) += 1;
    }
    for v in b {
        *counts.entry(canonical_key(v)).or_insert(0) -= 1;
    }
    counts.values().all(|&n| n == 0)
}

/// A canonical string form of a value, with **object keys sorted**, used as the
/// multiset key in [`is_permutation`].
///
/// Sorting is not cosmetic. This crate enables `serde_json`'s `preserve_order`, so a
/// `Value::Object` is an insertion-ordered `IndexMap` whose `PartialEq` compares as a
/// *map* — `{"a":1,"b":2}` and `{"b":2,"a":1}` are `==`. Their default renderings are
/// not. Keying the multiset on the raw rendering would therefore call two elements
/// different that `Value` itself calls equal, and an array reordering whose objects
/// happened to be serialized with different key order would fall out of
/// `ordering-changed` into `value-changed` — silently splitting one declaration-order
/// defect across two signatures and defeating the deduplication that class exists for.
fn canonical_key(value: &Value) -> String {
    fn write(value: &Value, out: &mut String) {
        use std::fmt::Write as _;
        match value {
            Value::Object(map) => {
                let mut keys: Vec<&String> = map.keys().collect();
                keys.sort_unstable();
                out.push('{');
                for (i, k) in keys.iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    // `Value::String`'s Display-via-to_string is the JSON-escaped form,
                    // so key and value alike stay unambiguous.
                    let _ = write!(out, "{}:", Value::String((*k).clone()));
                    write(&map[k.as_str()], out);
                }
                out.push('}');
            }
            Value::Array(items) => {
                out.push('[');
                for (i, item) in items.iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    write(item, out);
                }
                out.push(']');
            }
            // Scalars render unambiguously and carry no ordering of their own.
            other => {
                let _ = write!(out, "{other}");
            }
        }
    }
    let mut out = String::new();
    write(value, &mut out);
    out
}

/// `sig-<hash8>` over `channel ‖ path ‖ kind ‖ valueShapeClass`.
fn signature_id(
    channel: &str,
    path: &str,
    kind: DivergenceKind,
    value_shape_class: ValueShapeClass,
) -> String {
    format!(
        "sig-{}",
        hash8(&[channel, path, kind.as_str(), value_shape_class.as_str()])
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn div<'a>(
        kind: DivergenceKind,
        path: &'a str,
        deacon: Option<&'a Value>,
        reference: Option<&'a Value>,
    ) -> Divergence<'a> {
        Divergence {
            kind,
            path,
            deacon,
            reference,
        }
    }

    #[test]
    fn present_absent_covers_both_one_sided_kinds() {
        let v = json!("vscode");
        assert_eq!(
            classify(&div(
                DivergenceKind::RefOnly,
                "configuration.remoteUser",
                None,
                Some(&v)
            )),
            ValueShapeClass::PresentAbsent
        );
        assert_eq!(
            classify(&div(
                DivergenceKind::DeaconOnly,
                "configuration.remoteUser",
                Some(&v),
                None
            )),
            ValueShapeClass::PresentAbsent
        );
    }

    #[test]
    fn type_changed_when_the_two_json_types_differ() {
        let d = json!("3000");
        let r = json!(3000);
        assert_eq!(
            classify(&div(
                DivergenceKind::Value,
                "configuration.forwardPorts.0",
                Some(&d),
                Some(&r)
            )),
            ValueShapeClass::TypeChanged
        );

        // null-vs-value is a type change, not a presence change: both sides emitted
        // something, and `null` is a value the spec distinguishes from omission.
        let null = json!(null);
        let some = json!("x");
        assert_eq!(
            classify(&div(DivergenceKind::Value, "p", Some(&null), Some(&some))),
            ValueShapeClass::TypeChanged
        );

        // An array against an object is a type change, never an ordering change.
        let arr = json!([1, 2]);
        let obj = json!({ "a": 1 });
        assert_eq!(
            classify(&div(DivergenceKind::Value, "p", Some(&arr), Some(&obj))),
            ValueShapeClass::TypeChanged
        );
    }

    #[test]
    fn integers_and_floats_are_one_type() {
        // `1` vs `1.0` is the producer's serialization detail, not a type either
        // implementation chose; classifying it as `type-changed` would manufacture a
        // phantom type defect out of a value difference.
        let d = json!(1);
        let r = json!(1.5);
        assert_eq!(
            classify(&div(DivergenceKind::Value, "p", Some(&d), Some(&r))),
            ValueShapeClass::ValueChanged
        );
    }

    #[test]
    fn ordering_changed_detects_an_array_permutation() {
        let d = json!(["a", "b", "c"]);
        let r = json!(["c", "a", "b"]);
        assert_eq!(
            classify(&div(
                DivergenceKind::Value,
                "configuration.features",
                Some(&d),
                Some(&r)
            )),
            ValueShapeClass::OrderingChanged,
            "declaration-order defects are a known family here and must not fold into \
             value-changed"
        );

        // Permutation of non-scalars too — the class is about order, not element type.
        let d = json!([{ "id": "a" }, { "id": "b" }]);
        let r = json!([{ "id": "b" }, { "id": "a" }]);
        assert_eq!(
            classify(&div(DivergenceKind::Value, "p", Some(&d), Some(&r))),
            ValueShapeClass::OrderingChanged
        );
    }

    #[test]
    fn object_key_order_within_an_element_does_not_defeat_permutation_detection() {
        // `preserve_order` makes a JSON object an insertion-ordered map whose equality is
        // still map equality: these two elements are `==` as `Value`s but render
        // differently. Keying the multiset on the raw rendering would call them different
        // and demote a genuine reordering to `value-changed`, splitting one
        // declaration-order defect across two signatures.
        let left = json!({ "id": "a", "version": "1" });
        let right = json!({ "version": "1", "id": "a" });
        assert_eq!(left, right, "serde_json compares objects as maps");
        assert_eq!(canonical_key(&left), canonical_key(&right));

        let d = json!([{ "id": "a", "version": "1" }, { "id": "b", "version": "2" }]);
        let r = json!([{ "version": "2", "id": "b" }, { "version": "1", "id": "a" }]);
        assert_eq!(
            classify(&div(DivergenceKind::Value, "p", Some(&d), Some(&r))),
            ValueShapeClass::OrderingChanged
        );

        // Nested objects and arrays canonicalize too, and genuinely different values
        // still key differently.
        assert_eq!(
            canonical_key(&json!({ "a": [{ "y": 1, "x": 2 }] })),
            canonical_key(&json!({ "a": [{ "x": 2, "y": 1 }] }))
        );
        assert_ne!(
            canonical_key(&json!({ "a": 1 })),
            canonical_key(&json!({ "a": 2 }))
        );
        // A key/value boundary cannot be blurred: `{"a:1":null}` must not key the same
        // as `{"a":1}`.
        assert_ne!(
            canonical_key(&json!({ "a:1": null })),
            canonical_key(&json!({ "a": 1 }))
        );
    }

    #[test]
    fn a_multiset_difference_is_not_an_ordering_change() {
        // Same SET, different multiset: calling this an ordering difference would
        // misattribute a genuine content change to element order.
        let d = json!([1, 1, 2]);
        let r = json!([1, 2, 2]);
        assert_eq!(
            classify(&div(DivergenceKind::Value, "p", Some(&d), Some(&r))),
            ValueShapeClass::ValueChanged
        );

        // Different lengths, and a strict subsequence, are both value changes.
        let d = json!([1, 2, 3]);
        let r = json!([1, 2]);
        assert_eq!(
            classify(&div(DivergenceKind::Value, "p", Some(&d), Some(&r))),
            ValueShapeClass::ValueChanged
        );

        // A one-element array has only one order, so a difference there is never
        // about ordering.
        let d = json!(["a"]);
        let r = json!(["b"]);
        assert_eq!(
            classify(&div(DivergenceKind::Value, "p", Some(&d), Some(&r))),
            ValueShapeClass::ValueChanged
        );
    }

    #[test]
    fn value_changed_is_the_residual_class() {
        let d = json!("vscode");
        let r = json!("root");
        assert_eq!(
            classify(&div(
                DivergenceKind::Value,
                "configuration.remoteUser",
                Some(&d),
                Some(&r)
            )),
            ValueShapeClass::ValueChanged
        );
        let d = json!({ "a": 1 });
        let r = json!({ "a": 2 });
        assert_eq!(
            classify(&div(DivergenceKind::Value, "p", Some(&d), Some(&r))),
            ValueShapeClass::ValueChanged
        );
    }

    #[test]
    fn a_value_divergence_missing_a_side_degrades_to_present_absent() {
        // The diff never produces this, but a malformed adapter could. Classifying it
        // as the presence difference it describes is honest; inventing a comparison
        // against `None` would not be.
        let d = json!("x");
        assert_eq!(
            classify(&div(DivergenceKind::Value, "p", Some(&d), None)),
            ValueShapeClass::PresentAbsent
        );
    }

    #[test]
    fn the_id_is_substance_anchored_and_stable() {
        let d = json!("vscode");
        let r = json!("root");
        let a = Signature::derive(
            "chan-structured-output",
            &div(
                DivergenceKind::Value,
                "configuration.remoteUser",
                Some(&d),
                Some(&r),
            ),
        );
        // Same structure, DIFFERENT concrete values → same signature. This is the whole
        // point: concrete values are evidence on the witness, not identity.
        let d2 = json!("node");
        let r2 = json!("ubuntu");
        let b = Signature::derive(
            "chan-structured-output",
            &div(
                DivergenceKind::Value,
                "configuration.remoteUser",
                Some(&d2),
                Some(&r2),
            ),
        );
        assert_eq!(a.id, b.id, "concrete values must not enter the signature");
        assert_eq!(a.finding_id(), b.finding_id());

        assert!(a.id.starts_with("sig-"));
        assert_eq!(a.id.len(), "sig-".len() + 8);
        assert_eq!(
            a.derived_id(),
            a.id,
            "the id must recompute from its fields"
        );
        assert!(a.finding_id().starts_with("fnd-"));
        assert_eq!(a.finding_id().len(), "fnd-".len() + 8);
    }

    #[test]
    fn every_signature_component_changes_the_id() {
        let d = json!("a");
        let r = json!("b");
        let base = Signature::derive(
            "chan-structured-output",
            &div(
                DivergenceKind::Value,
                "configuration.remoteUser",
                Some(&d),
                Some(&r),
            ),
        );
        // channel
        let other_channel = Signature::derive(
            "chan-stdout",
            &div(
                DivergenceKind::Value,
                "configuration.remoteUser",
                Some(&d),
                Some(&r),
            ),
        );
        assert_ne!(base.id, other_channel.id);
        // path
        let other_path = Signature::derive(
            "chan-structured-output",
            &div(
                DivergenceKind::Value,
                "configuration.remoteEnv",
                Some(&d),
                Some(&r),
            ),
        );
        assert_ne!(base.id, other_path.id);
        // kind
        let other_kind = Signature::derive(
            "chan-structured-output",
            &div(
                DivergenceKind::RefOnly,
                "configuration.remoteUser",
                None,
                Some(&r),
            ),
        );
        assert_ne!(base.id, other_kind.id);
        // value-shape class (same channel/path/kind, different shape)
        let arr_d = json!(["a", "b"]);
        let arr_r = json!(["b", "a"]);
        let other_shape = Signature::derive(
            "chan-structured-output",
            &div(
                DivergenceKind::Value,
                "configuration.remoteUser",
                Some(&arr_d),
                Some(&arr_r),
            ),
        );
        assert_ne!(base.id, other_shape.id);
    }

    #[test]
    fn a_path_containing_the_field_separator_cannot_collide() {
        // A signature's `path` comes verbatim from the diff and is built from
        // user-controlled configuration keys, so it can contain any byte. The shared
        // `hash8` length-prefixes rather than separating for exactly this reason; assert
        // it at the signature level, where the consequence of a collision would be two
        // distinct defects merging into one finding.
        let v = json!(1);
        let a = Signature::derive(
            "chan-structured-output",
            &div(DivergenceKind::Value, "a\u{1f}b", Some(&v), Some(&v)),
        );
        let b = Signature::derive(
            "chan-structured-output\u{1f}a",
            &div(DivergenceKind::Value, "b", Some(&v), Some(&v)),
        );
        assert_ne!(a.id, b.id);
        assert_ne!(hash8(&["ab", "c"]), hash8(&["a", "bc"]));
    }

    #[test]
    fn kind_wire_spellings_round_trip() {
        // These must stay byte-identical to `DiffKind::as_str()`: the adapter in
        // parity-harness maps between them, and a spelling drift would silently
        // re-key every signature.
        for kind in [
            DivergenceKind::RefOnly,
            DivergenceKind::DeaconOnly,
            DivergenceKind::Value,
        ] {
            assert_eq!(DivergenceKind::parse(kind.as_str()), Some(kind));
        }
        assert_eq!(DivergenceKind::parse("value-ish"), None);
    }

    #[test]
    fn signature_round_trips_through_strict_json() {
        let d = json!(["a", "b"]);
        let r = json!(["b", "a"]);
        let sig = Signature::derive(
            "chan-structured-output",
            &div(
                DivergenceKind::Value,
                "configuration.features",
                Some(&d),
                Some(&r),
            ),
        );
        let raw = serde_json::to_string(&sig).expect("serializes");
        assert!(raw.contains("\"valueShapeClass\":\"ordering-changed\""));
        let back: Signature = serde_json::from_str(&raw).expect("round-trips");
        assert_eq!(back, sig);

        let err = serde_json::from_str::<Signature>(
            r#"{"id":"sig-1","channel":"c","path":"p","kind":"value",
                "valueShapeClass":"value-changed","extra":1}"#,
        )
        .expect_err("unknown fields are rejected at load");
        assert!(err.to_string().contains("extra"));
    }
}
