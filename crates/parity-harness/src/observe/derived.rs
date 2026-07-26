//! Derived observer fields (024 US5, T122).
//!
//! # The 023 hard line
//!
//! When a comparison needs a **search**, a **grouping**, or a **classification**, the
//! OBSERVER computes it into a named field and the assertion language stays declarative
//! equality. The alternative — a predicate or query bolted onto `expected` — turns the
//! case record into a small programming language: once `∃` exists, `∀`, negation and
//! composition follow, and a reviewer can no longer read a case and know what it claims.
//!
//! `chan-container-state`'s `workspaceBindTargets` (024 Phase 4) established the rule;
//! this module is where the US5 fields that need the same treatment live, so the
//! computations are unit-testable without Docker and shared rather than re-derived per
//! observer.
//!
//! Every function here is **additive**: it derives a new field from evidence that is
//! already captured. None of them removes or replaces a raw field — a derived summary
//! that REPLACED its source would be a collapse wearing a different name, which is
//! exactly what US5 exists to retire.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use serde_json::{Map, Value};

/// Parse a Docker `Env` array (or a set of `"KEY=VALUE"` strings) into a canonical
/// `{ key: value }` object.
///
/// **Why derived**: `chan-container-state` serializes `env` as a sorted array of
/// `"KEY=VALUE"` strings. Pinning ONE variable against an array requires either an exact
/// whole-array assertion (which breaks whenever an unrelated variable changes) or a
/// search predicate. As an object, `envMap.CONF_LAYER` is ordinary equality.
pub fn env_map(env: &Value) -> Value {
    let mut map = Map::new();
    if let Some(items) = env.as_array() {
        for item in items {
            if let Some(s) = item.as_str() {
                let (k, v) = s.split_once('=').unwrap_or((s, ""));
                map.insert(k.to_string(), Value::String(v.to_string()));
            }
        }
    }
    Value::Object(map)
}

/// The `PATH` variable of an [`env_map`] object, split into its `:`-separated segments.
///
/// `Value::Null` when the container declares no `PATH` at all — distinct from a declared
/// but empty `PATH` (`[""]`), which FR-018/FR-025 require to stay distinguishable.
///
/// **Why derived**: FR-050 asks that PATH *construction* be compared — specifically that
/// a segment contributed by a Feature is present. A whole-string equality on `PATH` fails
/// on any base-image difference; a `contains` on the joined string would match a
/// substring of an unrelated segment (`/opt/a/bin` inside `/opt/a/bin-extra`). Segments
/// make "this segment is present" an order-insensitive array subset, which the assertion
/// language already evaluates.
pub fn path_segments(env_map: &Value) -> Value {
    match env_map.get("PATH").and_then(Value::as_str) {
        Some(path) => Value::Array(
            path.split(':')
                .map(|s| Value::String(s.to_string()))
                .collect(),
        ),
        None => Value::Null,
    }
}

/// `{ destination: source }` for every mount, carrying the **full** source.
///
/// - bind → the absolute host path (the shared normalizer's `path_token` rewrites the
///   per-run temp workspace afterwards, so it is portable but not truncated);
/// - volume → the volume name;
/// - tmpfs and anything else → the empty string (tmpfs has no source).
///
/// **Why derived**: `StateSnapshot::mounts` carries `sourceTail` — the bind's LEAF
/// component and the volume's project-relative name. That is a collapse: two mounts whose
/// host paths differ but whose leaf components agree compare equal, so a wrong mount
/// source is invisible. FR-053 requires distinguishing a differing SOURCE PATH from a
/// differing mount SHAPE; `mountSources` is the source axis and `mounts.<dest>.{mountType,
/// ro}` remains the shape axis.
pub fn mount_sources(inspect: &Value) -> Value {
    let mut map = Map::new();
    if let Some(arr) = inspect["Mounts"].as_array() {
        for m in arr {
            let dest = m["Destination"].as_str().unwrap_or("");
            if dest.is_empty() {
                continue;
            }
            let source = match m["Type"].as_str().unwrap_or("") {
                "volume" => m["Name"].as_str().unwrap_or("").to_string(),
                "bind" => m["Source"].as_str().unwrap_or("").to_string(),
                _ => String::new(),
            };
            map.insert(dest.to_string(), Value::String(source));
        }
    }
    Value::Object(map)
}

/// `{ namespace: [full label key, …] }` — every label key grouped by its dotted
/// namespace (everything before the LAST dot; `<none>` for an undotted key).
///
/// **Why derived**: FR-052 asks that labels be compared BY NAMESPACE, "including labels
/// one side emits and the other does not". Per-key equality answers "do these two
/// labels agree"; it cannot answer "does one side populate a namespace the other leaves
/// empty" without enumerating every key either side might ever emit. The grouped key
/// SET makes that an ordinary comparison — and makes a one-sided label visible as a
/// difference in the namespace's membership rather than as an absence nobody asserted on.
pub fn label_namespaces(labels: &Value) -> Value {
    let mut grouped: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    if let Some(obj) = labels.as_object() {
        for key in obj.keys() {
            let ns = match key.rfind('.') {
                Some(idx) => key[..idx].to_string(),
                None => "<none>".to_string(),
            };
            grouped.entry(ns).or_default().insert(key.clone());
        }
    }
    Value::Object(
        grouped
            .into_iter()
            .map(|(ns, keys)| {
                (
                    ns,
                    Value::Array(keys.into_iter().map(Value::String).collect()),
                )
            })
            .collect(),
    )
}

/// `{ name, uid, group, gid }` parsed from a Docker `Config.User` string.
///
/// Docker's `User` is a single overloaded string: `""`, `"root"`, `"1000"`,
/// `"1000:1000"`, `"vscode:vscode"`, `"vscode:1000"` are all legal and mean different
/// things. A numeric component populates `uid`/`gid`; a non-numeric one populates
/// `name`/`group`. The unset component stays `null` — an unset gid is NOT a gid of 0.
///
/// **Why derived**: FR-051 asks for "the effective user and the observable effects of UID
/// and GID". Asserting on the raw string forces a case to know which of the six spellings
/// the CLI happened to choose, so a case pinning `"1000:1000"` fails against an
/// equivalent `"vscode"` for a reason that has nothing to do with the effective identity.
pub fn user_spec(user: &str) -> Value {
    let (user_part, group_part) = match user.split_once(':') {
        Some((u, g)) => (u, Some(g)),
        None => (user, None),
    };
    let split = |part: &str| -> (Value, Value) {
        if part.is_empty() {
            (Value::Null, Value::Null)
        } else if let Ok(n) = part.parse::<u64>() {
            (Value::Null, Value::from(n))
        } else {
            (Value::String(part.to_string()), Value::Null)
        }
    };
    let (name, uid) = split(user_part);
    let (group, gid) = match group_part {
        Some(g) => split(g),
        None => (Value::Null, Value::Null),
    };
    let mut map = Map::new();
    map.insert("name".to_string(), name);
    map.insert("uid".to_string(), uid);
    map.insert("group".to_string(), group);
    map.insert("gid".to_string(), gid);
    Value::Object(map)
}

/// `{ project, networks, volumes }` — the Compose project a container belongs to and the
/// project-RELATIVE names of the resources attached to it.
///
/// **Why derived**: `strip_project_prefix` (the `compose_project_prefix` rule) rewrites a
/// project-prefixed network/volume name to its project-relative tail so two CLIs that
/// derive different project names still compare. Until now the project name itself was
/// discarded with no trace, so nothing in the evidence recorded WHAT was stripped — the
/// rule was invisible to a reader of the snapshot. Emitting `project` alongside the
/// stripped names keeps the rewrite auditable, and gives FR-054 a field to compare.
pub fn compose_project_resources(inspect: &Value, project: &str) -> Value {
    let strip = |name: &str| -> String {
        if project.is_empty() {
            return name.to_string();
        }
        name.strip_prefix(&format!("{project}_"))
            .unwrap_or(name)
            .to_string()
    };

    let mut networks: BTreeSet<String> = BTreeSet::new();
    if let Some(obj) = inspect["NetworkSettings"]["Networks"].as_object() {
        for key in obj.keys() {
            networks.insert(strip(key));
        }
    }
    let mut volumes: BTreeSet<String> = BTreeSet::new();
    if let Some(arr) = inspect["Mounts"].as_array() {
        for m in arr {
            if m["Type"].as_str() == Some("volume") {
                if let Some(name) = m["Name"].as_str() {
                    volumes.insert(strip(name));
                }
            }
        }
    }

    let mut map = Map::new();
    map.insert("project".to_string(), Value::String(project.to_string()));
    map.insert(
        "networks".to_string(),
        Value::Array(networks.into_iter().map(Value::String).collect()),
    );
    map.insert(
        "volumes".to_string(),
        Value::Array(volumes.into_iter().map(Value::String).collect()),
    );
    Value::Object(map)
}

// ---------------------------------------------------------------------------
// null / empty / omitted (FR-055)
// ---------------------------------------------------------------------------

/// How a CLI reported one configuration property, as one of four distinguishable states.
///
/// `null`, `empty` and `omitted` are the three FR-055 requires to stay distinct; `present`
/// is everything else. They are strings rather than a richer shape so the classification
/// compares as ordinary scalar equality.
fn classify(value: Option<&Value>) -> &'static str {
    match value {
        None => "omitted",
        Some(Value::Null) => "null",
        Some(Value::Array(a)) if a.is_empty() => "empty",
        Some(Value::Object(o)) if o.is_empty() => "empty",
        Some(Value::String(s)) if s.is_empty() => "empty",
        Some(_) => "present",
    }
}

/// `{ property: "null" | "empty" | "omitted" | "present" }` for the properties the
/// workspace's `devcontainer.json` **authors** — the FR-055 three-state classification of
/// a resolved-configuration document.
///
/// **Why derived**: "did this CLI preserve the author's `null`, their `[]`, or their
/// omission?" is a three-way classification of a value's SHAPE. Expressed in the
/// assertion language it needs a type predicate; computed here it is a string a case
/// compares with `equals`. Recording it also means the distinction survives in the
/// evidence even where a document-level rule later elides the property.
///
/// **Why only the AUTHORED properties**: classifying every modeled optional would report
/// the ~40 properties deacon serializes unconditionally as `empty`/`null` against the
/// reference's `omitted` on EVERY configuration case — one real defect (characterized
/// once) restated as thousands of per-case differences, which is noise, not evidence.
/// Restricted to what the fixture actually wrote, the field answers the question the
/// author posed and nothing else. A fixture that wants the OMITTED state observed as
/// well names the property in the sidecar (see [`authored_properties`]) — the scope is
/// declared by the fixture, in data, rather than inferred.
pub fn null_empty_omitted(document: &Value, authored: &BTreeSet<String>) -> Value {
    let config = document
        .get("configuration")
        .or_else(|| document.get("mergedConfiguration"));
    let Some(Value::Object(config)) = config else {
        return Value::Object(Map::new());
    };
    let mut map = Map::new();
    for key in authored {
        map.insert(
            key.clone(),
            Value::String(classify(config.get(key)).to_string()),
        );
    }
    Value::Object(map)
}

/// Whether `document` is a resolved-configuration document at all (it carries a
/// `configuration` or `mergedConfiguration` object).
///
/// The structured-output channel also carries `upgrade`'s lockfile document and other
/// non-configuration results; deriving a configuration-shaped field onto those would add
/// a key that means nothing there and would change what an exact `jsonEquals` on that
/// document compares.
pub fn is_configuration_document(document: &Value) -> bool {
    document.get("configuration").is_some_and(Value::is_object)
        || document
            .get("mergedConfiguration")
            .is_some_and(Value::is_object)
}

/// The sidecar by which a fixture declares EXTRA properties to classify — one property
/// name per line, blank lines and `#` comments ignored.
///
/// It is deliberately NOT part of `devcontainer.json`: neither CLI must see it, and a key
/// added to the configuration to steer an observation would change the very document
/// under observation.
pub const OBSERVED_PROPERTIES_SIDECAR: &str = ".devcontainer/.conformance-observed-properties";

/// The property names [`null_empty_omitted`] classifies: those the workspace's
/// `devcontainer.json` **authors**, plus any the fixture names in
/// [`OBSERVED_PROPERTIES_SIDECAR`].
///
/// Looks in the two spec discovery locations (`.devcontainer/devcontainer.json` and the
/// root `.devcontainer.json`); a plain `devcontainer.json` at the workspace root is NOT a
/// discovery location. A file that cannot be read or parsed yields an EMPTY set, which
/// makes [`null_empty_omitted`] an empty object on BOTH sides — never a one-sided field.
///
/// The sidecar is what lets a case observe the **omitted** state: a property the author
/// left out is not in the document, so nothing else can name it, and without a declared
/// name the only alternative would be classifying every modeled optional — which restates
/// one characterized defect as a difference on every configuration case.
pub fn authored_properties(workspace: &Path) -> BTreeSet<String> {
    let mut names = BTreeSet::new();
    if let Ok(text) = std::fs::read_to_string(workspace.join(OBSERVED_PROPERTIES_SIDECAR)) {
        for line in text.lines() {
            let line = line.trim();
            if !line.is_empty() && !line.starts_with('#') {
                names.insert(line.to_string());
            }
        }
    }
    for rel in [".devcontainer/devcontainer.json", ".devcontainer.json"] {
        let path = workspace.join(rel);
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        let Ok(Value::Object(map)) = serde_json::from_str::<Value>(&text) else {
            // JSONC (comments / trailing commas) is deliberately NOT parsed here: a
            // best-effort comment stripper is its own source of wrong answers, and a
            // fixture that needs this field can be written as strict JSON.
            return names;
        };
        names.extend(map.keys().cloned());
        break;
    }
    names
}

#[cfg(test)]
mod tests {
    use super::*;

    use serde_json::json;

    #[test]
    fn env_map_parses_key_value_strings() {
        let env = json!(["A=1", "B=", "NOEQUALS"]);
        assert_eq!(
            env_map(&env),
            json!({ "A": "1", "B": "", "NOEQUALS": "" }),
            "a valueless entry stays present with an empty value, never dropped"
        );
    }

    #[test]
    fn path_segments_distinguish_absent_from_empty() {
        assert_eq!(
            path_segments(&json!({ "OTHER": "x" })),
            Value::Null,
            "no PATH at all is null"
        );
        assert_eq!(
            path_segments(&json!({ "PATH": "" })),
            json!([""]),
            "a declared but empty PATH is a one-element list, distinct from null"
        );
        assert_eq!(
            path_segments(&json!({ "PATH": "/a:/b" })),
            json!(["/a", "/b"])
        );
    }

    #[test]
    fn mount_sources_keeps_the_whole_source_not_the_leaf() {
        let inspect = json!({
            "Mounts": [
                { "Type": "bind", "Source": "/host/one/ws", "Destination": "/w", "RW": true },
                { "Type": "bind", "Source": "/host/two/ws", "Destination": "/x", "RW": true },
                { "Type": "volume", "Name": "vol-a", "Source": "/var/lib/x", "Destination": "/v" },
                { "Type": "tmpfs", "Destination": "/t" },
            ]
        });
        let sources = mount_sources(&inspect);
        // The two binds share a LEAF ("ws"), which is all `sourceTail` records. The
        // derived field keeps them distinguishable — the whole point of FR-053.
        assert_eq!(sources["/w"], json!("/host/one/ws"));
        assert_eq!(sources["/x"], json!("/host/two/ws"));
        assert_ne!(sources["/w"], sources["/x"]);
        assert_eq!(
            sources["/v"],
            json!("vol-a"),
            "a volume's source is its name"
        );
        assert_eq!(sources["/t"], json!(""), "tmpfs has no source");
    }

    #[test]
    fn label_namespaces_group_by_the_leading_dotted_prefix() {
        let labels = json!({
            "devcontainer.local_folder": "/w",
            "devcontainer.config_file": "/w/.devcontainer/devcontainer.json",
            "com.docker.compose.project": "p",
            "undotted": "v",
        });
        assert_eq!(
            label_namespaces(&labels),
            json!({
                "com.docker.compose": ["com.docker.compose.project"],
                "devcontainer": ["devcontainer.config_file", "devcontainer.local_folder"],
                "<none>": ["undotted"],
            })
        );
    }

    #[test]
    fn user_spec_separates_numeric_ids_from_names() {
        assert_eq!(
            user_spec(""),
            json!({ "name": null, "uid": null, "group": null, "gid": null })
        );
        assert_eq!(
            user_spec("root"),
            json!({ "name": "root", "uid": null, "group": null, "gid": null })
        );
        assert_eq!(
            user_spec("1000:1000"),
            json!({ "name": null, "uid": 1000, "group": null, "gid": 1000 })
        );
        assert_eq!(
            user_spec("vscode:1000"),
            json!({ "name": "vscode", "uid": null, "group": null, "gid": 1000 })
        );
        // An unset gid is null, NOT 0 — root's gid and "no gid declared" are different
        // claims and a case must be able to tell them apart.
        assert_eq!(user_spec("1000")["gid"], Value::Null);
    }

    #[test]
    fn compose_project_resources_record_what_the_prefix_rule_stripped() {
        let inspect = json!({
            "NetworkSettings": { "Networks": { "proj_default": {} } },
            "Mounts": [ { "Type": "volume", "Name": "proj_data", "Destination": "/d" } ],
        });
        assert_eq!(
            compose_project_resources(&inspect, "proj"),
            json!({ "project": "proj", "networks": ["default"], "volumes": ["data"] }),
            "the project name is retained; without it the strip leaves no trace"
        );
        assert_eq!(
            compose_project_resources(&inspect, "")["networks"],
            json!(["proj_default"]),
            "no project means no strip"
        );
    }

    #[test]
    fn null_empty_omitted_distinguishes_all_four_states() {
        let doc = json!({
            "configuration": {
                "name": null,
                "forwardPorts": [],
                "image": "alpine:3.19",
            }
        });
        let authored: BTreeSet<String> = ["name", "forwardPorts", "image", "mounts"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        assert_eq!(
            null_empty_omitted(&doc, &authored),
            json!({
                "forwardPorts": "empty",
                "image": "present",
                "mounts": "omitted",
                "name": "null",
            }),
            "null, empty, omitted and present are four distinguishable observations"
        );
    }

    #[test]
    fn a_non_configuration_document_derives_nothing() {
        assert!(!is_configuration_document(&json!({ "features": {} })));
        assert!(is_configuration_document(&json!({ "configuration": {} })));
        assert!(is_configuration_document(
            &json!({ "mergedConfiguration": {} })
        ));
    }
}
