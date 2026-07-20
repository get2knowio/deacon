//! Single-visit, definition-site constraint extraction (T012, research
//! Decisions 3–4).
//!
//! The walk visits every schema object EXACTLY ONCE, at the JSON Pointer where it is
//! defined ([`extract`]). Each object's keywords decompose into typed facets — one
//! [`ExtractedUnit`] per facet — covering all 15 [`ConstraintKind`] variants. A
//! `$ref` becomes a single `reference` edge unit and is NEVER inlined; the target's
//! own content is extracted once, at the target's own pointer (Decision 3). Any
//! keyword the extractor does not recognise becomes an `unmodeled-keyword` unit
//! carrying its raw value — nothing is ever silently dropped (constitution IV,
//! Decision 4).
//!
//! ## Determinism
//!
//! Object-keyed iterations (`properties`, `patternProperties`, `definitions`, and the
//! per-object keyword scan) are sorted so the emitted set is stable regardless of the
//! parser's map ordering; arrays (`enum`, `type`, `oneOf`/`anyOf` arms, …) preserve
//! upstream order because that order is semantically significant. The final inventory
//! is additionally sorted by ID ([`crate::inventory`]), so byte-identical
//! regeneration (Decision 7) holds by construction.
//!
//! ## Granularity (documented, consistent per Decision 4)
//!
//! - `property-existence` / `required`: one unit per property/required name, emitted
//!   at the property's own pointer (`…/properties/<name>` / `…/required/<name>`), with
//!   the name folded in — so each is a distinct `(document, pointer, kind)` diff key.
//! - `array-shape` / `value-shape`: ONE combined unit per schema object owning any of
//!   that family's keywords (object-owning-keywords granularity). Sub-schema-valued
//!   array keywords (`items`, `contains`, …) are recorded as a presence marker in the
//!   substance and their content is extracted by recursion at the child pointer (never
//!   inlined).
//! - `additional-properties`: the `additionalProperties`/`unevaluatedProperties`
//!   tri-state (`false` closed / schema / `true` open) plus each `patternProperties`
//!   entry, one unit each; substance names which keyword it came from. Absence of the
//!   keyword is NOT recorded (absent-is-open framing).
//! - `annotation`: one unit per annotation keyword (independently classifiable).

use serde_json::{Map, Value};

use super::resolve::resolve_ref;
use super::{DocumentSet, SchemaDocument, pointer_push};
use crate::load::LoadError;
use crate::model::{BranchContext, ConditionContext, ConstraintKind, UnitContext};

/// One extracted constraint facet before ID derivation (document + id are assigned by
/// [`crate::inventory`]).
#[derive(Debug, Clone, PartialEq)]
pub struct ExtractedUnit {
    /// RFC 6901 JSON Pointer to the schema object owning the facet (definition site).
    pub pointer: String,
    /// The facet's constraint kind.
    pub kind: ConstraintKind,
    /// Canonicalized-by-caller JSON value of the facet — the testable rule itself.
    pub substance: Value,
    /// Composition/condition context when the owning object sits inside a branch.
    pub context: Option<UnitContext>,
}

// ---------------------------------------------------------------------------
// Keyword classification tables
// ---------------------------------------------------------------------------

/// Annotation keywords — carriers of no testable behavior (Decision 4). Each present
/// one becomes its own `annotation` unit.
const ANNOTATION_KEYWORDS: &[&str] = &[
    "$schema",
    "$id",
    "$comment",
    "title",
    "description",
    "markdownDescription",
    "examples",
    "deprecated",
    "deprecationMessage",
    "enumDescriptions",
    "defaultSnippets",
    "allowComments",
    "allowTrailingCommas",
    "readOnly",
    "writeOnly",
];

/// String/number scalar assertions folded into ONE `value-shape` unit per object.
const VALUE_SHAPE_KEYWORDS: &[&str] = &[
    "pattern",
    "format",
    "minLength",
    "maxLength",
    "minimum",
    "maximum",
    "exclusiveMinimum",
    "exclusiveMaximum",
    "multipleOf",
];

/// Scalar array assertions folded into the `array-shape` unit verbatim.
const ARRAY_SHAPE_SCALARS: &[&str] = &[
    "minItems",
    "maxItems",
    "uniqueItems",
    "minContains",
    "maxContains",
];

/// Sub-schema-valued array keywords: recorded as a presence marker in the
/// `array-shape` substance and recursed into (content extracted at the child pointer).
const ARRAY_SHAPE_SUBSCHEMAS: &[&str] = &["items", "additionalItems", "prefixItems", "contains"];

/// Purely structural containers: represented via their children (recursed) rather than
/// a unit for the container keyword itself. `properties` is here but ALSO emits a
/// per-key `property-existence` unit (handled explicitly).
const STRUCTURAL_CONTAINERS: &[&str] =
    &["definitions", "$defs", "properties", "propertyNames", "not"];

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

/// Extract every constraint facet from one document, in a deterministic order.
pub fn extract(doc: &SchemaDocument, docs: &DocumentSet) -> Result<Vec<ExtractedUnit>, LoadError> {
    let mut out = Vec::new();
    walk(docs, &doc.key, "", &doc.root, &None, &mut out)?;
    Ok(out)
}

/// Visit one schema object at `pointer`, emitting its facet units and recursing into
/// its sub-schemas. `context` is the composition/condition context inherited from the
/// owning branch (if any) and is attached to every unit emitted AT this object.
fn walk(
    docs: &DocumentSet,
    doc_key: &str,
    pointer: &str,
    value: &Value,
    context: &Option<UnitContext>,
    out: &mut Vec<ExtractedUnit>,
) -> Result<(), LoadError> {
    // A schema may be a boolean (`true`/`false`) in modern drafts; it carries no
    // keywords, so there is nothing to extract. Non-object, non-bool values only occur
    // inside keyword payloads we handle explicitly (e.g. enum members), never as a
    // schema position, so skipping here is safe.
    let obj = match value.as_object() {
        Some(o) => o,
        None => return Ok(()),
    };

    // Track which keys we have accounted for; anything left over is unmodeled.
    let mut handled: Vec<&str> = Vec::new();
    let emit = |kind, substance, out: &mut Vec<ExtractedUnit>| {
        out.push(ExtractedUnit {
            pointer: pointer.to_string(),
            kind,
            substance,
            context: context.clone(),
        });
    };

    // 1. Annotations — one unit per annotation keyword.
    for &kw in ANNOTATION_KEYWORDS {
        if let Some(v) = obj.get(kw) {
            handled.push(kw);
            emit(
                ConstraintKind::Annotation,
                serde_json::json!({ "keyword": kw, "value": v.clone() }),
                out,
            );
        }
    }

    // 2. type (with a nullable flag when "null" is a member of a type union).
    if let Some(t) = obj.get("type") {
        handled.push("type");
        emit(ConstraintKind::Type, type_substance(t), out);
    }

    // 3. enum / const / default — value assertions, order preserved for enum members.
    if let Some(e) = obj.get("enum") {
        handled.push("enum");
        emit(
            ConstraintKind::Enum,
            serde_json::json!({ "enum": e.clone() }),
            out,
        );
    }
    if let Some(c) = obj.get("const") {
        handled.push("const");
        emit(
            ConstraintKind::Const,
            serde_json::json!({ "const": c.clone() }),
            out,
        );
    }
    if let Some(d) = obj.get("default") {
        handled.push("default");
        emit(
            ConstraintKind::Default,
            serde_json::json!({ "default": d.clone() }),
            out,
        );
    }

    // 4. value-shape — one combined unit for the present string/number assertions.
    if let Some(sub) = collect_present(obj, VALUE_SHAPE_KEYWORDS, &mut handled) {
        emit(ConstraintKind::ValueShape, sub, out);
    }

    // 5. array-shape — one combined unit (scalars verbatim + sub-schema presence).
    if let Some(sub) = array_shape_substance(obj, &mut handled) {
        emit(ConstraintKind::ArrayShape, sub, out);
    }

    // 6. required — one unit per required name, folded into the pointer.
    if let Some(req) = obj.get("required") {
        handled.push("required");
        if let Some(names) = req.as_array() {
            let base = pointer_push(pointer, "required");
            for name in names {
                if let Some(name) = name.as_str() {
                    out.push(ExtractedUnit {
                        // Fold the required NAME (not array index) into the pointer so
                        // each required entry is a distinct, order-independent
                        // (document, pointer, kind) diff key.
                        pointer: pointer_push(&base, name),
                        kind: ConstraintKind::Required,
                        substance: serde_json::json!({ "required": name }),
                        context: context.clone(),
                    });
                }
            }
        }
    }

    // 7. $ref — a single reference edge (never inlined). Resolution validates the
    // target and fails loud on unresolved/external/malformed refs.
    if let Some(r) = obj.get("$ref") {
        handled.push("$ref");
        if let Some(reference) = r.as_str() {
            let resolved = resolve_ref(docs, doc_key, pointer, reference)?;
            let mut substance = serde_json::json!({
                "ref": reference,
                "targetPointer": resolved.pointer,
            });
            // Cross-document targets record the target document key explicitly.
            if resolved.document != doc_key {
                substance["targetDocument"] = Value::String(resolved.document);
            }
            emit(ConstraintKind::Reference, substance, out);
        } else {
            return Err(LoadError::MalformedRef {
                document: doc_key.to_string(),
                pointer: pointer.to_string(),
                reference: r.to_string(),
            });
        }
    }

    // 8. additional-properties family: additionalProperties / unevaluatedProperties
    // tri-state + each patternProperties entry.
    for kw in ["additionalProperties", "unevaluatedProperties"] {
        if let Some(v) = obj.get(kw) {
            handled.push(kw);
            emit(
                ConstraintKind::AdditionalProperties,
                additional_properties_substance(kw, v),
                out,
            );
        }
    }
    if let Some(pp) = obj.get("patternProperties").and_then(Value::as_object) {
        handled.push("patternProperties");
        for (pat, _sub) in sorted_entries(pp) {
            emit(
                ConstraintKind::AdditionalProperties,
                serde_json::json!({ "keyword": "patternProperties", "pattern": pat }),
                out,
            );
        }
    }

    // 9. all-of — a composition edge on this object.
    if let Some(all) = obj.get("allOf").and_then(Value::as_array) {
        handled.push("allOf");
        emit(
            ConstraintKind::AllOf,
            serde_json::json!({ "allOf": all.len() }),
            out,
        );
    }

    // 10. union-alternative — one unit per oneOf/anyOf arm, recording branch + index.
    for branch in ["oneOf", "anyOf"] {
        if let Some(arms) = obj.get(branch).and_then(Value::as_array) {
            handled.push(branch);
            for (i, _arm) in arms.iter().enumerate() {
                out.push(ExtractedUnit {
                    pointer: format!("{pointer}/{branch}/{i}"),
                    kind: ConstraintKind::UnionAlternative,
                    substance: serde_json::json!({ "branch": branch, "index": i }),
                    context: Some(UnitContext::Branch(BranchContext {
                        branch: branch.to_string(),
                        index: i,
                    })),
                });
            }
        }
    }

    // 11. conditional — if/then/else, preserving the condition's own pointer.
    if obj.contains_key("if") || obj.contains_key("then") || obj.contains_key("else") {
        let mut clauses = Vec::new();
        for clause in ["if", "then", "else"] {
            if obj.contains_key(clause) {
                handled.push(clause);
                clauses.push(clause);
            }
        }
        emit(
            ConstraintKind::Conditional,
            serde_json::json!({ "clauses": clauses }),
            out,
        );
    }

    // Mark the remaining structural containers as handled (represented by recursion).
    for &kw in STRUCTURAL_CONTAINERS {
        if obj.contains_key(kw) {
            handled.push(kw);
        }
    }

    // 12. unmodeled-keyword catch-all — any keyword not accounted for above lands here
    // verbatim, never silently dropped (constitution IV, Decision 4).
    for (k, v) in sorted_entries(obj) {
        if !handled.contains(&k.as_str()) {
            emit(
                ConstraintKind::UnmodeledKeyword,
                serde_json::json!({ "keyword": k, "value": v.clone() }),
                out,
            );
        }
    }

    // ---- Recursion into sub-schema positions -----------------------------------
    recurse(docs, doc_key, pointer, obj, out)
}

/// Recurse into every sub-schema position of `obj`, at its own pointer and context.
fn recurse(
    docs: &DocumentSet,
    doc_key: &str,
    pointer: &str,
    obj: &Map<String, Value>,
    out: &mut Vec<ExtractedUnit>,
) -> Result<(), LoadError> {
    // definitions / $defs — reusable sub-schemas (no context).
    for container in ["definitions", "$defs"] {
        if let Some(defs) = obj.get(container).and_then(Value::as_object) {
            for (name, sub) in sorted_entries(defs) {
                let child = pointer_push(pointer, container);
                walk(docs, doc_key, &pointer_push(&child, name), sub, &None, out)?;
            }
        }
    }

    // properties — emit property-existence AND recurse into each property schema.
    if let Some(props) = obj.get("properties").and_then(Value::as_object) {
        let base = pointer_push(pointer, "properties");
        for (name, sub) in sorted_entries(props) {
            let child = pointer_push(&base, name);
            out.push(ExtractedUnit {
                pointer: child.clone(),
                kind: ConstraintKind::PropertyExistence,
                substance: serde_json::json!({ "property": name }),
                context: None,
            });
            walk(docs, doc_key, &child, sub, &None, out)?;
        }
    }

    // patternProperties — recurse into each pattern's schema.
    if let Some(pp) = obj.get("patternProperties").and_then(Value::as_object) {
        let base = pointer_push(pointer, "patternProperties");
        for (pat, sub) in sorted_entries(pp) {
            walk(docs, doc_key, &pointer_push(&base, pat), sub, &None, out)?;
        }
    }

    // Single-schema sub-positions (recurse only when the value is a schema object).
    for kw in [
        "additionalProperties",
        "unevaluatedProperties",
        "propertyNames",
        "not",
        "additionalItems",
        "contains",
    ] {
        if let Some(v) = obj.get(kw) {
            if v.is_object() {
                walk(docs, doc_key, &pointer_push(pointer, kw), v, &None, out)?;
            }
        }
    }

    // items — a single schema or an array of schemas.
    if let Some(items) = obj.get("items") {
        match items {
            Value::Object(_) => walk(
                docs,
                doc_key,
                &pointer_push(pointer, "items"),
                items,
                &None,
                out,
            )?,
            Value::Array(arr) => {
                let base = pointer_push(pointer, "items");
                for (i, sub) in arr.iter().enumerate() {
                    walk(docs, doc_key, &format!("{base}/{i}"), sub, &None, out)?;
                }
            }
            _ => {}
        }
    }
    // prefixItems — an array of schemas.
    if let Some(Value::Array(arr)) = obj.get("prefixItems") {
        let base = pointer_push(pointer, "prefixItems");
        for (i, sub) in arr.iter().enumerate() {
            walk(docs, doc_key, &format!("{base}/{i}"), sub, &None, out)?;
        }
    }

    // Composition arms — allOf/oneOf/anyOf, each arm carrying its branch context.
    for branch in ["allOf", "oneOf", "anyOf"] {
        if let Some(arms) = obj.get(branch).and_then(Value::as_array) {
            let base = pointer_push(pointer, branch);
            for (i, arm) in arms.iter().enumerate() {
                let ctx = Some(UnitContext::Branch(BranchContext {
                    branch: branch.to_string(),
                    index: i,
                }));
                walk(docs, doc_key, &format!("{base}/{i}"), arm, &ctx, out)?;
            }
        }
    }

    // Conditional sub-schemas — then/else carry the condition's own pointer.
    let if_pointer = pointer_push(pointer, "if");
    if let Some(v) = obj.get("if") {
        if v.is_object() {
            walk(docs, doc_key, &if_pointer, v, &None, out)?;
        }
    }
    for clause in ["then", "else"] {
        if let Some(v) = obj.get(clause) {
            if v.is_object() {
                let ctx = Some(UnitContext::Condition(ConditionContext {
                    condition: if_pointer.clone(),
                }));
                walk(docs, doc_key, &pointer_push(pointer, clause), v, &ctx, out)?;
            }
        }
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Substance builders
// ---------------------------------------------------------------------------

/// Build the `type` unit substance, flagging `nullable` when `"null"` is a member of a
/// type union (data-model §2: `{"type": [...], "nullable": true}`).
fn type_substance(t: &Value) -> Value {
    let mut substance = serde_json::json!({ "type": t.clone() });
    if let Some(members) = t.as_array() {
        if members.iter().any(|m| m.as_str() == Some("null")) {
            substance["nullable"] = Value::Bool(true);
        }
    }
    substance
}

/// Collect the present keywords from `keywords` into a single substance object
/// (verbatim values), marking each handled. Returns `None` when none are present.
fn collect_present<'a>(
    obj: &'a Map<String, Value>,
    keywords: &[&'a str],
    handled: &mut Vec<&'a str>,
) -> Option<Value> {
    let mut map = Map::new();
    for &kw in keywords {
        if let Some(v) = obj.get(kw) {
            handled.push(kw);
            map.insert(kw.to_string(), v.clone());
        }
    }
    if map.is_empty() {
        None
    } else {
        Some(Value::Object(map))
    }
}

/// Build the combined `array-shape` substance: scalar keywords verbatim plus a
/// presence marker (`true`) for each sub-schema-valued array keyword (whose content is
/// extracted by recursion at the child pointer, never inlined).
fn array_shape_substance<'a>(
    obj: &'a Map<String, Value>,
    handled: &mut Vec<&'a str>,
) -> Option<Value> {
    let mut map = Map::new();
    for &kw in ARRAY_SHAPE_SCALARS {
        if let Some(v) = obj.get(kw) {
            handled.push(kw);
            map.insert(kw.to_string(), v.clone());
        }
    }
    for &kw in ARRAY_SHAPE_SUBSCHEMAS {
        if obj.contains_key(kw) {
            handled.push(kw);
            map.insert(kw.to_string(), Value::Bool(true));
        }
    }
    if map.is_empty() {
        None
    } else {
        Some(Value::Object(map))
    }
}

/// Build the `additional-properties` tri-state substance for `additionalProperties` /
/// `unevaluatedProperties`: `closed` (`false`), `open` (`true`), or `schema` (an object
/// whose content is recursed at the child pointer).
fn additional_properties_substance(keyword: &str, value: &Value) -> Value {
    let mode = match value {
        Value::Bool(false) => "closed",
        Value::Bool(true) => "open",
        Value::Object(_) => "schema",
        // Any other shape is unusual; record it verbatim under a distinct mode so it is
        // never silently normalized away.
        _ => "other",
    };
    serde_json::json!({ "keyword": keyword, "mode": mode })
}

/// Object entries sorted by key — deterministic iteration for object-keyed keywords.
fn sorted_entries(map: &Map<String, Value>) -> Vec<(&String, &Value)> {
    let mut entries: Vec<(&String, &Value)> = map.iter().collect();
    entries.sort_by(|a, b| a.0.cmp(b.0));
    entries
}
