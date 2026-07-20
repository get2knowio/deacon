//! Reference resolution + cycle detection (T011, research Decision 5).
//!
//! References are never inlined (Decision 3): resolution here answers "does this
//! `$ref` name a real location in the explicitly pinned document set?" and "do any
//! pure `$ref` chains form an unproductive cycle?". Both are fail-loud with typed,
//! cause-specific [`LoadError`]s (FR-009):
//!
//! - [`LoadError::MalformedRef`] — the `$ref` is not a string, or its fragment is not
//!   a valid RFC 6901 JSON Pointer (a non-empty fragment not starting with `/`).
//! - [`LoadError::UnresolvedRef`] — a fragment/relative ref whose target pointer is
//!   absent from the (existing) target document.
//! - [`LoadError::UnresolvedExternalRef`] — a live URL, or a relative path naming no
//!   document in the pinned set. NEVER fetched — a hard error by design (Decision 2).
//! - [`LoadError::RefCycle`] — a pure `$ref` chain `a -> b -> … -> a` where every node
//!   is a ref-only schema object (`{"$ref": …}` and nothing else). Productive
//!   recursion through structural keywords is NOT a cycle (Decision 3).

use serde_json::Value;

use super::{DocumentSet, resolve_pointer};
use crate::load::LoadError;

/// A parsed `$ref`, before target existence is checked.
#[derive(Debug, Clone, PartialEq, Eq)]
enum ParsedRef {
    /// `#/…` — a JSON Pointer within the current document (`#` alone → whole doc).
    Local { pointer: String },
    /// `path#/…` — names another document by vendored file name, plus a fragment.
    CrossDocument { file: String, pointer: String },
    /// A live URL (`scheme://…`) — outside the pinned set, never fetched.
    External,
}

/// A resolved reference: which document key it lands in and the target JSON Pointer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedRef {
    /// Manifest document key of the target.
    pub document: String,
    /// Target JSON Pointer within that document.
    pub pointer: String,
}

/// Whether `value` is a "ref-only" schema object: an object whose ONLY key is a
/// string `$ref`. Only chains between such nodes can form an unproductive cycle
/// (Decision 5); an object mixing `$ref` with other keywords is productive content.
pub fn is_ref_only(value: &Value) -> bool {
    match value.as_object() {
        Some(map) => map.len() == 1 && map.get("$ref").is_some_and(Value::is_string),
        None => false,
    }
}

/// Parse a `$ref` string into its structural shape (no existence check yet).
///
/// `MalformedRef` is signalled by `Ok(None)` here so the caller can attach the full
/// document/pointer/reference context to the error.
fn parse_ref(reference: &str) -> Option<ParsedRef> {
    // Split off the fragment on the FIRST '#'. `a#b#c` is malformed (two fragments)
    // — a second '#' inside `after` makes the fragment invalid below.
    let (before, fragment) = match reference.split_once('#') {
        Some((b, f)) => (b, Some(f)),
        None => (reference, None),
    };

    // A fragment, when present, must be empty or a valid JSON Pointer (start with
    // '/'), and must not itself contain a second '#'.
    let pointer = match fragment {
        None => String::new(),
        Some(f) => {
            if f.contains('#') {
                return None;
            }
            if !f.is_empty() && !f.starts_with('/') {
                // Plain-name (draft-04) fragments are not supported → malformed.
                return None;
            }
            f.to_string()
        }
    };

    if before.is_empty() {
        // `#/…` or `#` — same document.
        return Some(ParsedRef::Local { pointer });
    }
    // A scheme (`http://`, `https://`, …) marks a live URL: external, never fetched.
    if before.contains("://") {
        return Some(ParsedRef::External);
    }
    // Otherwise a relative path naming another vendored file. Strip a single leading
    // `./` for the file-name lookup; anything else is used verbatim.
    let file = before.strip_prefix("./").unwrap_or(before).to_string();
    Some(ParsedRef::CrossDocument { file, pointer })
}

/// Resolve a `$ref` found at `pointer` in document `current` to its target document
/// key + pointer, verifying the target location exists (Decision 5). Never fetches.
pub fn resolve_ref(
    docs: &DocumentSet,
    current: &str,
    pointer: &str,
    reference: &str,
) -> Result<ResolvedRef, LoadError> {
    let parsed = parse_ref(reference).ok_or_else(|| LoadError::MalformedRef {
        document: current.to_string(),
        pointer: pointer.to_string(),
        reference: reference.to_string(),
    })?;

    let (target_key, target_pointer) = match parsed {
        ParsedRef::Local { pointer: p } => (current.to_string(), p),
        ParsedRef::CrossDocument { file, pointer: p } => match docs.by_file(&file) {
            Some(doc) => (doc.key.clone(), p),
            // A relative path naming no pinned document is external (Decision 2/5).
            None => {
                return Err(LoadError::UnresolvedExternalRef {
                    document: current.to_string(),
                    reference: reference.to_string(),
                });
            }
        },
        ParsedRef::External => {
            return Err(LoadError::UnresolvedExternalRef {
                document: current.to_string(),
                reference: reference.to_string(),
            });
        }
    };

    // The target document is in the pinned set by construction here; verify the
    // pointer resolves within it.
    let root = &docs
        .by_key(&target_key)
        .expect("resolved target key is always a pinned document")
        .root;
    if resolve_pointer(root, &target_pointer).is_none() {
        return Err(LoadError::UnresolvedRef {
            document: current.to_string(),
            reference: reference.to_string(),
            target: target_pointer,
        });
    }

    Ok(ResolvedRef {
        document: target_key,
        pointer: target_pointer,
    })
}

/// Detect unproductive pure-`$ref` cycles across every document in the set.
///
/// Walks each document collecting ref-only objects, then follows each one's chain
/// with a visited path; a revisit is a [`LoadError::RefCycle`] listing the full chain
/// of `document#pointer` nodes. Chains that reach a non-ref-only (productive) target
/// terminate cleanly. Nodes are visited in a deterministic (document, pointer) order
/// so the first reported cycle is stable.
pub fn check_ref_cycles(docs: &DocumentSet) -> Result<(), LoadError> {
    // Collect every ref-only node as (document key, pointer), deterministically.
    let mut nodes: Vec<(String, String)> = Vec::new();
    for doc in docs.documents() {
        collect_ref_only_nodes(&doc.root, "", &doc.key, &mut nodes);
    }
    nodes.sort();

    for (doc_key, pointer) in &nodes {
        follow_chain(docs, doc_key, pointer)?;
    }
    Ok(())
}

/// Recursively collect the pointers of every ref-only object in `value`.
fn collect_ref_only_nodes(
    value: &Value,
    pointer: &str,
    doc_key: &str,
    out: &mut Vec<(String, String)>,
) {
    match value {
        Value::Object(map) => {
            if is_ref_only(value) {
                out.push((doc_key.to_string(), pointer.to_string()));
                // A ref-only object has no other children to descend into.
                return;
            }
            for (k, v) in map {
                collect_ref_only_nodes(v, &super::pointer_push(pointer, k), doc_key, out);
            }
        }
        Value::Array(items) => {
            for (i, v) in items.iter().enumerate() {
                collect_ref_only_nodes(v, &format!("{pointer}/{i}"), doc_key, out);
            }
        }
        _ => {}
    }
}

/// Follow a pure-`$ref` chain starting at a ref-only node, reporting a cycle if the
/// chain revisits a node before reaching productive content.
fn follow_chain(docs: &DocumentSet, start_doc: &str, start_pointer: &str) -> Result<(), LoadError> {
    let mut chain: Vec<String> = Vec::new();
    let mut cur_doc = start_doc.to_string();
    let mut cur_ptr = start_pointer.to_string();

    loop {
        let node = render_node(docs, &cur_doc, &cur_ptr);
        if chain.contains(&node) {
            chain.push(node);
            return Err(LoadError::RefCycle { chain });
        }
        chain.push(node);

        // The current node is ref-only by construction; read its $ref and resolve.
        let root = &docs
            .by_key(&cur_doc)
            .expect("chain node is a pinned document")
            .root;
        let obj = resolve_pointer(root, &cur_ptr)
            .and_then(Value::as_object)
            .expect("chain node is a ref-only object");
        let reference = obj
            .get("$ref")
            .and_then(Value::as_str)
            .expect("ref-only object has a string $ref");

        let resolved = resolve_ref(docs, &cur_doc, &cur_ptr, reference)?;
        let target_root = &docs
            .by_key(&resolved.document)
            .expect("resolved target is a pinned document")
            .root;
        let target_val = resolve_pointer(target_root, &resolved.pointer)
            .expect("resolve_ref verified the target exists");
        if is_ref_only(target_val) {
            cur_doc = resolved.document;
            cur_ptr = resolved.pointer;
            continue;
        }
        // Reached productive content — the chain is finite and productive.
        return Ok(());
    }
}

/// Render a chain node. Within a single-document set the document key is redundant, so
/// the pointer alone is shown (keeping cycle messages readable); multi-document sets
/// disambiguate with a `key#pointer` form.
fn render_node(docs: &DocumentSet, doc_key: &str, pointer: &str) -> String {
    if docs.documents().len() > 1 {
        format!("{doc_key}#{pointer}")
    } else {
        pointer.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::SchemaDocument;
    use serde_json::json;

    fn single(root: Value) -> DocumentSet {
        DocumentSet::new(vec![SchemaDocument {
            key: "base".into(),
            file: "base.json".into(),
            root,
        }])
    }

    #[test]
    fn is_ref_only_distinguishes_pure_refs() {
        assert!(is_ref_only(&json!({ "$ref": "#/a" })));
        assert!(!is_ref_only(&json!({ "$ref": "#/a", "type": "object" })));
        assert!(!is_ref_only(&json!({ "type": "object" })));
        assert!(!is_ref_only(&json!({ "$ref": 3 })));
        assert!(!is_ref_only(&json!("#/a")));
    }

    #[test]
    fn resolves_local_fragment_ref() {
        let docs = single(json!({
            "definitions": { "Mount": { "type": "object" } },
            "properties": { "m": { "$ref": "#/definitions/Mount" } }
        }));
        let r = resolve_ref(&docs, "base", "/properties/m", "#/definitions/Mount").unwrap();
        assert_eq!(r.document, "base");
        assert_eq!(r.pointer, "/definitions/Mount");
    }

    #[test]
    fn unresolved_fragment_ref_errors() {
        let docs = single(json!({ "definitions": {} }));
        let err = resolve_ref(&docs, "base", "/x", "#/definitions/Nope").unwrap_err();
        match err {
            LoadError::UnresolvedRef {
                target, reference, ..
            } => {
                assert_eq!(target, "/definitions/Nope");
                assert_eq!(reference, "#/definitions/Nope");
            }
            other => panic!("expected UnresolvedRef, got {other:?}"),
        }
    }

    #[test]
    fn live_url_ref_is_external_and_never_fetched() {
        let docs = single(json!({}));
        let err = resolve_ref(
            &docs,
            "base",
            "/x",
            "https://raw.githubusercontent.com/microsoft/vscode/main/x.json",
        )
        .unwrap_err();
        assert!(matches!(err, LoadError::UnresolvedExternalRef { .. }));
    }

    #[test]
    fn relative_path_to_unknown_document_is_external() {
        let docs = single(json!({}));
        let err =
            resolve_ref(&docs, "base", "/x", "./other.schema.json#/definitions/X").unwrap_err();
        assert!(matches!(err, LoadError::UnresolvedExternalRef { .. }));
    }

    #[test]
    fn relative_path_to_pinned_document_resolves() {
        let docs = DocumentSet::new(vec![
            SchemaDocument {
                key: "base".into(),
                file: "base.json".into(),
                root: json!({ "definitions": { "X": { "type": "string" } } }),
            },
            SchemaDocument {
                key: "extra".into(),
                file: "extra.json".into(),
                root: json!({ "properties": { "x": { "$ref": "./base.json#/definitions/X" } } }),
            },
        ]);
        let r = resolve_ref(
            &docs,
            "extra",
            "/properties/x",
            "./base.json#/definitions/X",
        )
        .unwrap();
        assert_eq!(r.document, "base");
        assert_eq!(r.pointer, "/definitions/X");
    }

    #[test]
    fn malformed_ref_fragment_errors() {
        let docs = single(json!({}));
        // A plain-name (non-'/') fragment is unsupported → malformed.
        let err = resolve_ref(&docs, "base", "/x", "#definitions").unwrap_err();
        assert!(matches!(err, LoadError::MalformedRef { .. }));
        // A double fragment is malformed too.
        let err2 = resolve_ref(&docs, "base", "/x", "#/a#/b").unwrap_err();
        assert!(matches!(err2, LoadError::MalformedRef { .. }));
    }

    #[test]
    fn productive_recursion_is_not_a_cycle() {
        // A self-referential-but-productive schema: `node` refers to itself through a
        // structural `properties` keyword, never a bare $ref loop.
        let docs = single(json!({
            "definitions": {
                "node": {
                    "type": "object",
                    "properties": {
                        "child": { "$ref": "#/definitions/node" }
                    }
                }
            }
        }));
        // `/definitions/node/properties/child` is ref-only, but its target
        // (`/definitions/node`) is productive → no cycle.
        check_ref_cycles(&docs).expect("productive recursion is not a cycle");
    }

    #[test]
    fn pure_ref_cycle_is_detected_with_full_chain() {
        let docs = single(json!({
            "definitions": {
                "a": { "$ref": "#/definitions/b" },
                "b": { "$ref": "#/definitions/a" }
            }
        }));
        let err = check_ref_cycles(&docs).unwrap_err();
        match err {
            LoadError::RefCycle { chain } => {
                // The chain returns to its start (a -> b -> a or b -> a -> b).
                assert_eq!(chain.first(), chain.last());
                assert!(
                    chain.len() >= 3,
                    "chain should list the full loop: {chain:?}"
                );
                assert!(chain.iter().any(|c| c.contains("/definitions/a")));
                assert!(chain.iter().any(|c| c.contains("/definitions/b")));
            }
            other => panic!("expected RefCycle, got {other:?}"),
        }
    }

    #[test]
    fn self_referential_pure_ref_is_a_cycle() {
        let docs = single(json!({
            "definitions": { "loop": { "$ref": "#/definitions/loop" } }
        }));
        assert!(matches!(
            check_ref_cycles(&docs).unwrap_err(),
            LoadError::RefCycle { .. }
        ));
    }
}
