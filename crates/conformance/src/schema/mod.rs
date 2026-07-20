//! Schema document model + RFC 6901 JSON Pointer helpers (T010,
//! 020-schema-constraint-inventory).
//!
//! The constraint-inventory extractor operates on a set of vendored, pinned JSON
//! Schema documents (`conformance/schemas/<rev>/`). This module provides the shared
//! primitives every extraction stage builds on:
//!
//! - [`SchemaDocument`] — one parsed schema document plus its manifest key/file;
//! - [`DocumentSet`] — the collection of documents an extraction runs over, with
//!   key- and file-based lookup (file lookup supports cross-document relative refs,
//!   research Decision 5 / User Story 4);
//! - RFC 6901 JSON Pointer building/escaping ([`escape_token`], [`pointer_push`])
//!   and resolution ([`resolve_pointer`]).
//!
//! The [`resolve`](crate::schema::resolve) submodule layers reference resolution and
//! cycle detection on top; [`extract`](crate::schema::extract) is the single-visit
//! definition-site walk (research Decision 3).

pub mod extract;
pub mod resolve;

use std::collections::HashMap;

use serde_json::Value;

/// One parsed, pinned schema document. `root` is the raw parsed JSON (never mutated
/// or ref-inlined — Decision 3); `key`/`file` come from the manifest
/// ([`crate::model::ManifestDocument`]).
#[derive(Debug, Clone)]
pub struct SchemaDocument {
    /// Manifest document key (`base`, `feature`, …) — the `<doc>` component of
    /// constraint IDs.
    pub key: String,
    /// Vendored file name (a sibling of the manifest) — the relative-ref target name
    /// for cross-document references.
    pub file: String,
    /// Parsed schema root.
    pub root: Value,
}

/// The set of documents a single extraction/resolution run operates over.
///
/// Keyed by manifest document key AND by vendored file name: fragment refs (`#/…`)
/// resolve within the current key; relative-path refs (`./other.json#/…`) resolve to
/// another document by file name (research Decision 5). Anything else — a live URL or
/// a path naming no document in the set — is an unresolved external reference.
#[derive(Debug, Clone, Default)]
pub struct DocumentSet {
    docs: Vec<SchemaDocument>,
    by_key: HashMap<String, usize>,
    by_file: HashMap<String, usize>,
}

impl DocumentSet {
    /// Build a document set from parsed documents in manifest order.
    pub fn new(docs: Vec<SchemaDocument>) -> DocumentSet {
        let mut by_key = HashMap::new();
        let mut by_file = HashMap::new();
        for (i, d) in docs.iter().enumerate() {
            by_key.insert(d.key.clone(), i);
            by_file.insert(d.file.clone(), i);
        }
        DocumentSet {
            docs,
            by_key,
            by_file,
        }
    }

    /// Every document, in manifest order.
    pub fn documents(&self) -> &[SchemaDocument] {
        &self.docs
    }

    /// Look up a document by its manifest key.
    pub fn by_key(&self, key: &str) -> Option<&SchemaDocument> {
        self.by_key.get(key).map(|&i| &self.docs[i])
    }

    /// Look up a document by its vendored file name (cross-document ref target).
    pub fn by_file(&self, file: &str) -> Option<&SchemaDocument> {
        self.by_file.get(file).map(|&i| &self.docs[i])
    }
}

/// Escape one path segment for embedding in an RFC 6901 JSON Pointer: `~` → `~0` and
/// `/` → `~1`. The `~` substitution MUST run first so a literal `/` that becomes `~1`
/// is not itself re-escaped.
pub fn escape_token(token: &str) -> String {
    token.replace('~', "~0").replace('/', "~1")
}

/// Un-escape one RFC 6901 reference token: `~1` → `/` then `~0` → `~` (the reverse
/// order of [`escape_token`], per RFC 6901 §4).
pub fn unescape_token(token: &str) -> String {
    token.replace("~1", "/").replace("~0", "~")
}

/// Append `token` (a raw, un-escaped path segment) to the JSON Pointer `base`,
/// escaping the token. `base` is `""` for the document root.
pub fn pointer_push(base: &str, token: &str) -> String {
    format!("{base}/{}", escape_token(token))
}

/// Resolve an RFC 6901 JSON Pointer against `root`, returning the referenced value or
/// `None` if any segment is absent. The empty pointer `""` denotes the whole
/// document. Array indices are decimal; a non-numeric token never resolves into an
/// array (RFC 6901 §4).
pub fn resolve_pointer<'a>(root: &'a Value, pointer: &str) -> Option<&'a Value> {
    if pointer.is_empty() {
        return Some(root);
    }
    // A non-empty pointer MUST begin with '/', and each subsequent '/'-delimited part
    // is one (escaped) reference token.
    let rest = pointer.strip_prefix('/')?;
    let mut current = root;
    for raw in rest.split('/') {
        let token = unescape_token(raw);
        current = match current {
            Value::Object(map) => map.get(&token)?,
            Value::Array(items) => {
                // RFC 6901: array indices are `0` or a non-leading-zero decimal.
                let idx: usize = parse_array_index(&token)?;
                items.get(idx)?
            }
            _ => return None,
        };
    }
    Some(current)
}

/// Parse an RFC 6901 array index: `"0"` or a decimal with no leading zero, no sign.
fn parse_array_index(token: &str) -> Option<usize> {
    if token == "0" {
        return Some(0);
    }
    if token.starts_with('0') || token.is_empty() {
        return None;
    }
    token.parse::<usize>().ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn escape_round_trips_tilde_and_slash() {
        // RFC 6901 §3 examples plus the two escape metacharacters together.
        assert_eq!(escape_token("a/b"), "a~1b");
        assert_eq!(escape_token("m~n"), "m~0n");
        assert_eq!(escape_token("~/"), "~0~1");
        // Ordering matters: a literal '/' must not be double-escaped via '~1' -> '~'.
        assert_eq!(unescape_token(&escape_token("a/b")), "a/b");
        assert_eq!(unescape_token(&escape_token("m~n")), "m~n");
        assert_eq!(unescape_token(&escape_token("~1literal")), "~1literal");
        assert_eq!(
            unescape_token(&escape_token("weird ~0/ key")),
            "weird ~0/ key"
        );
    }

    #[test]
    fn pointer_push_escapes_segments() {
        assert_eq!(pointer_push("", "definitions"), "/definitions");
        assert_eq!(
            pointer_push("/definitions", "devContainerCommon"),
            "/definitions/devContainerCommon"
        );
        // A property name containing '/' or '~' is escaped into a single token.
        assert_eq!(
            pointer_push("/patternProperties", "a/b"),
            "/patternProperties/a~1b"
        );
        assert_eq!(pointer_push("/x", "~weird"), "/x/~0weird");
    }

    #[test]
    fn resolve_pointer_walks_objects_and_arrays() {
        let doc = json!({
            "definitions": {
                "Mount": { "type": "object", "required": ["type", "target"] }
            },
            "oneOf": [ { "a": 1 }, { "b": 2 } ]
        });
        assert!(resolve_pointer(&doc, "").is_some());
        assert_eq!(
            resolve_pointer(&doc, "/definitions/Mount/type").unwrap(),
            &json!("object")
        );
        assert_eq!(
            resolve_pointer(&doc, "/definitions/Mount/required/1").unwrap(),
            &json!("target")
        );
        assert_eq!(resolve_pointer(&doc, "/oneOf/0/a").unwrap(), &json!(1));
        // Absent paths and out-of-range indices resolve to None.
        assert!(resolve_pointer(&doc, "/definitions/Nope").is_none());
        assert!(resolve_pointer(&doc, "/oneOf/5").is_none());
        // A leading-zero index is not a valid RFC 6901 array index.
        assert!(resolve_pointer(&doc, "/oneOf/01").is_none());
    }

    #[test]
    fn resolve_pointer_handles_escaped_property_names() {
        // Property names literally containing '/' and '~' must be reached via their
        // escaped tokens (~1 and ~0).
        let doc = json!({
            "a/b": { "leaf": true },
            "m~n": 7
        });
        assert_eq!(resolve_pointer(&doc, "/a~1b/leaf").unwrap(), &json!(true));
        assert_eq!(resolve_pointer(&doc, "/m~0n").unwrap(), &json!(7));
        // The un-escaped spelling must NOT resolve (it would name different tokens).
        assert!(resolve_pointer(&doc, "/a/b/leaf").is_none());
    }

    #[test]
    fn document_set_lookup_by_key_and_file() {
        let set = DocumentSet::new(vec![
            SchemaDocument {
                key: "base".into(),
                file: "devContainer.base.schema.json".into(),
                root: json!({ "type": "object" }),
            },
            SchemaDocument {
                key: "feature".into(),
                file: "devContainerFeature.schema.json".into(),
                root: json!({ "type": "object" }),
            },
        ]);
        assert_eq!(set.documents().len(), 2);
        assert_eq!(
            set.by_key("feature").unwrap().file,
            "devContainerFeature.schema.json"
        );
        assert_eq!(
            set.by_file("devContainer.base.schema.json").unwrap().key,
            "base"
        );
        assert!(set.by_key("missing").is_none());
        assert!(set.by_file("missing.json").is_none());
    }
}
