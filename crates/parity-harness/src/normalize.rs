//! THE single equivalence definition (research D7, FR-019).
//!
//! There is exactly ONE normalization per comparison type here — [`config`],
//! [`merged_config`], and [`container_state`] — replacing the three divergent
//! copies the harness carried before (a Rust key-allowlist plus two Python
//! `prune` implementations). For configuration output the **prune semantics**
//! win: unwrap the reference's `{configuration}` wrapper, drop `configFilePath`,
//! prune nulls / empty containers, and sanitize dynamic ids — a full-shape
//! compare with documented pruning, not a permissive allowlist that would ignore
//! divergences in every unlisted key. Every function returns `Result`; a
//! normalization failure is a hard [`HarnessError::Normalization`], never a
//! fallback to raw comparison.
//!
//! # Single-module guarantee (FR-019, T041 audit)
//!
//! This module is the ONLY place equivalence is defined for the whole harness.
//! The residual-duplication audit (T041) verifies that no second implementation
//! survives anywhere in the repository:
//! - the retired Rust key-allowlist `extract_core_config` exists nowhere (it was
//!   deleted, not kept "because it was stable" — an allowlist silently ignores
//!   divergences in every unlisted key);
//! - the blanket config `prune` and `sanitize_dynamic_values`/`replace_hex12` helpers
//!   are GONE (023 T062/T063, research D3): `prune` removed every null/empty value at
//!   every depth plus `configFilePath`, and `replace_hex12` rewrote any 12-char hex run
//!   anywhere. They are replaced by the named, enumerated, justified rules
//!   [`drop_absent_optional`] and [`devcontainer_id_token`]. (The unrelated
//!   `core::port_forward::registry::prune`, which reaps dead daemon records, is not
//!   normalization.);
//! - the three Python corpus runners that carried duplicate `prune` copies were
//!   deleted in T030 — no `fixtures/**` script normalizes output;
//! - the resolved-configuration rule chain is ONE function,
//!   [`config_document_rules`], shared by the legacy `config`/`merged_config` entry
//!   points and the declarative `chan-structured-output` channel.
//!
//! Cross-runner equivalence is proven by `tests/normalize_consistency.rs`
//! (SC-005): the same output pair yields the same verdict regardless of which
//! runner calls in, and `merged_config` agrees with `config` on the shared block.
//! Any new comparison type MUST be added here, never re-implemented in a runner.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use serde_json::{Map, Value};

use crate::HarnessError;
use crate::evidence::{NormalizedChannelEvidence, RawChannelEvidence};

// ===========================================================================
// Declarative conformance runner: THE single channel-normalization entry point
// and its named, field-specific rules (022-conformance-runner, D6, T042/T043).
//
// Named rules — `path_token`, `label_semantic`, `mount_source_canonical`,
// `path_env_segmented`, `null_preserving` — REPLACE the T011 pass-through. Each rule
// REWRITES or CANONICALIZES; NONE blanket-removes env vars, labels, mount sources,
// entrypoints, commands, or networks (FR-029). The null/empty/default distinction is
// preserved (FR-025). This is the ONLY normalizer (Constitution VIII).
// ===========================================================================

/// The normalizer version, recorded in snapshot provenance and participating in
/// staleness (FR-030). It is bumped whenever ANY named normalization rule changes so a
/// snapshot recorded under an older normalizer replays as stale (data-model §7).
///
/// SINGLE SOURCE OF TRUTH: re-exported from [`deacon_conformance::snapshot`] so the
/// snapshot provenance (conformance, the lower crate) and this normalizer never drift.
/// `"1"` was the T011 pass-through; `"2"` is the US3 named-rule normalizer. The runner
/// stamps it into the verdict report (`VerdictReport::new`) and the refresh bin records
/// it into `Provenance.normalizerVersion`; staleness compares the recorded value
/// against it (T032).
pub use deacon_conformance::snapshot::NORMALIZER_VERSION;

/// The FINITE, ENUMERATED key names [`drop_absent_optional`] may remove when their value
/// carries no information (023 T062).
///
/// Measured, not guessed: these are exactly the keys observed to be present-but-absent
/// in deacon's `read-configuration` output and omitted by the pinned reference across
/// all 24 Tier-1 corpus workspaces in both plain and `--include-merged-configuration`
/// modes. Every one is a modeled `devcontainer.json` property (or a nested property of
/// `hostRequirements` / `portsAttributes`).
///
/// **This list is the whole safety property.** It is why the rule is not `prune`: a key
/// not named here is compared, so a newly added property cannot silently disappear.
/// Extending it is a reviewable, deliberate act — never a side effect.
pub const ABSENT_OPTIONAL_KEYS: &[&str] = &[
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
];

/// The FINITE, ENUMERATED properties inside which [`devcontainer_id_token`] applies its
/// hex rewrite (023 T063) — the fields a substituted `${devcontainerId}` can reach.
///
/// Everywhere else a 12-char lowercase-hex run is left alone, so two genuinely different
/// digests can no longer be collapsed to one token and mask a divergence.
pub const DEVCONTAINER_ID_FIELDS: &[&str] = &[
    "containerEnv",
    "mounts",
    "name",
    "remoteEnv",
    "runArgs",
    "workspaceMount",
];

/// The `<WORKSPACE>` / `<PROJECT>` path token substitution context for `path_token`
/// (FR-024).
///
/// Each `(path, token)` pair rewrites occurrences of an absolute temp path to a stable
/// token so evidence is portable across machines/recordings. Substitutions are applied
/// longest-path-first so a nested path tokenizes before its parent.
#[derive(Debug, Clone, Default)]
pub struct TokenMap {
    subs: Vec<(String, String)>,
}

impl TokenMap {
    /// An empty token map (no substitutions).
    pub fn new() -> TokenMap {
        TokenMap::default()
    }

    /// A token map that rewrites the workspace path to `<WORKSPACE>`.
    pub fn workspace(workspace: &Path) -> TokenMap {
        let mut m = TokenMap::new();
        m.insert(workspace.to_string_lossy(), "<WORKSPACE>");
        m
    }

    /// **Rule `workspace_basename_token`** — [`TokenMap::workspace`] plus the workspace
    /// directory's BASENAME → `<WORKSPACE_NAME>` (024 Phase 4).
    ///
    /// Each side of a differential runs in its OWN isolated temp workspace, so a config
    /// with no explicit `workspaceFolder` yields a container path derived from the
    /// basename — `/workspaces/deacon-conf-aaa` on deacon's side versus
    /// `/workspaces/deacon-conf-bbb` on the reference's. The full-path substitution
    /// cannot reach those: the container-side path contains only the basename, never the
    /// host path. Without this rule EVERY container-state comparison would report a mount
    /// destination divergence that is purely an artifact of the isolation the runner
    /// itself imposes.
    ///
    /// Rewrite, never delete. Because [`path_token`] rewrites object KEYS as well as
    /// values, mount destinations keyed by the container path normalize on both sides;
    /// the same substitution also reaches a bind mount's `sourceTail` (the host workspace
    /// directory's leaf component). Scoped to `chan-container-state` (the registry rule's
    /// declared scope) — the plain [`TokenMap::workspace`] is used everywhere else, so no
    /// other channel's evidence changes meaning.
    pub fn workspace_with_basename(workspace: &Path) -> TokenMap {
        let mut m = TokenMap::workspace(workspace);
        if let Some(name) = workspace.file_name().and_then(|s| s.to_str()) {
            m.insert(name, "<WORKSPACE_NAME>");
        }
        m
    }

    /// Add a `(path → token)` substitution. Empty paths are ignored.
    pub fn insert(&mut self, path: impl Into<String>, token: impl Into<String>) {
        let path = path.into();
        if path.is_empty() {
            return;
        }
        self.subs.push((path, token.into()));
        // Longest path first: a nested path must tokenize before its parent prefix.
        self.subs.sort_by_key(|(p, _)| std::cmp::Reverse(p.len()));
    }

    /// Apply every substitution to `s` (rewrite, never delete).
    fn apply(&self, s: &str) -> String {
        let mut out = s.to_string();
        for (path, token) in &self.subs {
            if out.contains(path.as_str()) {
                out = out.replace(path.as_str(), token);
            }
        }
        out
    }
}

/// **Rule `path_token`** (FR-024): rewrite temp workspace/project paths to stable tokens
/// in every string within `value`, recursively (object keys AND values, array
/// elements). Rewrite, NEVER delete; structure, null, and empty are preserved.
pub fn path_token(value: &Value, tokens: &TokenMap) -> Value {
    match value {
        Value::String(s) => Value::String(tokens.apply(s)),
        Value::Array(items) => Value::Array(items.iter().map(|v| path_token(v, tokens)).collect()),
        Value::Object(map) => Value::Object(
            map.iter()
                .map(|(k, v)| (tokens.apply(k), path_token(v, tokens)))
                .collect(),
        ),
        other => other.clone(),
    }
}

/// **Rule `null_preserving`** (FR-025): the channel normalizer NEVER prunes
/// null / missing / empty / defaulted fields (unlike the config `prune`). This named
/// rule is identity on the value — it exists so the preservation guarantee is explicit
/// and auditable wherever the contract lists it. Only a named rule may ever collapse a
/// specific field; nothing is dropped implicitly.
pub fn null_preserving(value: &Value) -> Value {
    value.clone()
}

/// **Rule `label_semantic`** (FR-026): parse container labels into a canonical
/// key/value object so labels compare SEMANTICALLY, not as opaque strings. Accepts an
/// object (`{k: v}`) or a Docker-style array of `"k=v"` strings and yields an object.
/// NEVER blanket-removes a label (FR-029) — every label is preserved.
pub fn label_semantic(labels: &Value) -> Value {
    match labels {
        Value::Array(items) => {
            let mut map = Map::new();
            for item in items {
                if let Some(s) = item.as_str() {
                    let (k, v) = s.split_once('=').unwrap_or((s, ""));
                    map.insert(k.to_string(), Value::String(v.to_string()));
                }
            }
            Value::Object(map)
        }
        other => other.clone(),
    }
}

/// **Rule `mount_source_canonical`** (FR-027): path-substitute each mount `source`
/// before compare, so two mounts that differ ONLY by a temp path compare equal. Given a
/// mounts array `[{ source, target, ... }]`, rewrites each `source` via the token map.
/// NEVER removes a mount (FR-029).
pub fn mount_source_canonical(mounts: &Value, tokens: &TokenMap) -> Value {
    match mounts {
        Value::Array(items) => Value::Array(
            items
                .iter()
                .map(|m| match m {
                    Value::Object(obj) => {
                        let mut obj = obj.clone();
                        if let Some(src) = obj.get("source").and_then(Value::as_str) {
                            obj.insert("source".to_string(), Value::String(tokens.apply(src)));
                        }
                        Value::Object(obj)
                    }
                    other => path_token(other, tokens),
                })
                .collect(),
        ),
        other => path_token(other, tokens),
    }
}

/// **Rule `path_env_segmented`** (FR-028): compare a PATH-like value SEGMENT-WISE, not as
/// one string. Accepts a `:`-joined string OR an array of segments and yields an array
/// of (path-tokenized) segments, so equality compares element-by-element. The optional
/// executable probe (resolving which segment holds the invoked executable) is a seam for
/// the injected-process channel (US5, FR-028) — the segmentation itself is the rule's
/// core and is what US3 needs; no US3 channel requires the probe.
pub fn path_env_segmented(path_value: &Value, tokens: &TokenMap) -> Value {
    let segments: Vec<Value> = match path_value {
        Value::String(s) => s
            .split(':')
            .map(|seg| Value::String(tokens.apply(seg)))
            .collect(),
        Value::Array(items) => items
            .iter()
            .map(|v| match v.as_str() {
                Some(seg) => Value::String(tokens.apply(seg)),
                None => path_token(v, tokens),
            })
            .collect(),
        other => return path_token(other, tokens),
    };
    Value::Array(segments)
}

/// THE single channel-normalization entry point for the declarative runner
/// (Constitution VIII — one normalizer). Applies the per-channel named rules
/// (contract observer-channel.md) to a channel's [`RawChannelEvidence`], yielding
/// [`NormalizedChannelEvidence`]. `present` is preserved verbatim — a not-captured
/// channel (`present:false`) stays distinct from a captured-empty value (FR-018) — and
/// nothing is blanket-removed (FR-029).
pub fn normalize_channel(
    channel: &str,
    raw: &RawChannelEvidence,
    tokens: &TokenMap,
) -> NormalizedChannelEvidence {
    debug_assert_eq!(
        channel, raw.channel,
        "normalize_channel: `channel` must match the evidence's channel"
    );
    NormalizedChannelEvidence {
        channel: raw.channel.clone(),
        operation: raw.operation.clone(),
        present: raw.present,
        value: apply_channel_rules(channel, &raw.value, tokens),
    }
}

/// The token map a channel's normalization runs under — the ONE place the per-channel
/// token policy lives (Constitution VIII: channel-specific normalization belongs in the
/// normalizer, not in the runner).
///
/// `chan-container-state` additionally tokenizes the workspace BASENAME
/// (`workspace_basename_token`), because its evidence carries container-side paths
/// derived from the per-side temp workspace name. Every other channel gets the plain
/// full-path map, so this change cannot alter what any existing channel compares.
pub fn tokens_for_channel(channel: &str, workspace: &Path) -> TokenMap {
    if channel == deacon_conformance::model::CHAN_CONTAINER_STATE {
        TokenMap::workspace_with_basename(workspace)
    } else {
        TokenMap::workspace(workspace)
    }
}

/// Apply the named rules the contract lists for `channel` (observer-channel.md). An
/// unknown channel is identity (never blanket-removed).
fn apply_channel_rules(channel: &str, value: &Value, tokens: &TokenMap) -> Value {
    use deacon_conformance::model::{
        CHAN_CONTAINER_STATE, CHAN_EXIT_CODE, CHAN_FILE_CONTENT, CHAN_FILESYSTEM, CHAN_IMAGE,
        CHAN_INJECTED_PROCESS, CHAN_PROCESS_GRAPH, CHAN_STDERR, CHAN_STDOUT,
        CHAN_STRUCTURED_OUTPUT, CHAN_TEMPORAL,
    };
    match channel {
        // No rule: an exit code carries no path/label/PATH content.
        CHAN_EXIT_CODE => value.clone(),
        CHAN_STDOUT | CHAN_STDERR => path_token(value, tokens),
        // The resolved-configuration document: the SAME named rule chain the legacy
        // `config`/`merged_config` entry points apply, so the two comparison paths
        // share ONE definition of equivalence (constitution VIII, 023 T062).
        CHAN_STRUCTURED_OUTPUT => config_document_rules(&path_token(value, tokens)),
        CHAN_FILE_CONTENT => null_preserving(&path_token(value, tokens)),
        CHAN_FILESYSTEM => path_token(value, tokens),
        CHAN_IMAGE => normalize_image(value, tokens),
        CHAN_PROCESS_GRAPH => normalize_process_graph(value, tokens),
        CHAN_INJECTED_PROCESS => normalize_injected_process(value, tokens),
        CHAN_TEMPORAL => null_preserving(value),
        // `chan-container-state`: `workspace_basename_token` (carried by the token map
        // from `tokens_for_channel`) + `path_token` over the whole snapshot — object KEYS
        // included, so mount destinations keyed by the container-side workspace path
        // normalize on both sides — then `null_preserving`. NOTHING is removed: labels,
        // entrypoint, cmd and networks are emitted verbatim and any characterized
        // difference is covered by a scoped, backed `allowedDifference` (024 Phase 4).
        CHAN_CONTAINER_STATE => null_preserving(&path_token(value, tokens)),
        _ => value.clone(),
    }
}

/// `chan-image`: `label_semantic` on the `labels` field, `path_token` elsewhere,
/// `null_preserving` overall.
fn normalize_image(value: &Value, tokens: &TokenMap) -> Value {
    let mut v = path_token(value, tokens);
    if let Value::Object(obj) = &mut v {
        if let Some(labels) = obj.get("labels") {
            let semantic = label_semantic(labels);
            obj.insert("labels".to_string(), semantic);
        }
    }
    null_preserving(&v)
}

/// `chan-process-graph`: `mount_source_canonical` on `mounts`, `path_token` elsewhere.
fn normalize_process_graph(value: &Value, tokens: &TokenMap) -> Value {
    let mut v = value.clone();
    if let Value::Object(obj) = &mut v {
        if let Some(mounts) = obj.get("mounts") {
            let canonical = mount_source_canonical(mounts, tokens);
            obj.insert("mounts".to_string(), canonical);
        }
    }
    path_token(&v, tokens)
}

/// `chan-injected-process`: `path_env_segmented` on `path`, `path_token` + `null_preserving`.
fn normalize_injected_process(value: &Value, tokens: &TokenMap) -> Value {
    let mut v = value.clone();
    if let Value::Object(obj) = &mut v {
        if let Some(path) = obj.get("path") {
            let segmented = path_env_segmented(path, tokens);
            obj.insert("path".to_string(), segmented);
        }
    }
    null_preserving(&path_token(&v, tokens))
}

// ===========================================================================
// Configuration normalization (Tier 1 / Tier 1b)
// ===========================================================================

/// Normalize `read-configuration` output for comparison: unwrap the reference's
/// `{configuration}` wrapper, then apply the SAME named rule chain the declarative
/// `chan-structured-output` channel applies ([`config_document_rules`]).
///
/// `prune` is gone (023 T062, research D3). It removed every null, empty object, empty
/// array and empty string ANYWHERE in the document, plus `configFilePath`
/// unconditionally — an unbounded removal set that hid a deacon-only field simply for
/// being empty, and hid `configFilePath` outright. What replaces it is
/// [`drop_absent_optional`]: a finite, enumerated key list, dropped only when the value
/// carries no information. A field NOT on that list now surfaces, which is the point.
pub fn config(case: &str, raw: &str) -> Result<Value, HarnessError> {
    let v = parse(case, raw)?;
    let inner = match &v {
        Value::Object(o) => match o.get("configuration") {
            Some(c @ Value::Object(_)) => c.clone(),
            _ => v.clone(),
        },
        _ => v.clone(),
    };
    Ok(config_document_rules(&inner))
}

/// Normalize the `mergedConfiguration` block (Tier 1b): the same named rule chain
/// applied to that block. A non-object top-level is a normalization failure.
pub fn merged_config(case: &str, raw: &str) -> Result<Value, HarnessError> {
    let v = parse(case, raw)?;
    let block = match &v {
        Value::Object(o) => o
            .get("mergedConfiguration")
            .cloned()
            .unwrap_or_else(|| Value::Object(Map::new())),
        _ => {
            return Err(HarnessError::Normalization {
                case: case.to_string(),
                cause: "top-level output is not a JSON object".to_string(),
            });
        }
    };
    Ok(config_document_rules(&block))
}

/// THE named rule chain for a resolved-configuration document — the SINGLE definition
/// shared by the legacy [`config`] / [`merged_config`] entry points and the declarative
/// `chan-structured-output` channel (constitution VIII: one normalizer, not two).
///
/// In order: [`devcontainer_id_token`] (rewrite), [`drop_absent_optional`] (drop, finite
/// enumerated key set), then [`null_preserving`] — which is identity and exists to state,
/// at the end of the chain, that NOTHING else is removed.
pub fn config_document_rules(value: &Value) -> Value {
    null_preserving(&drop_absent_optional(&devcontainer_id_token(value)))
}

/// **Rule `drop_absent_optional`** (023 T062, action `drop`, scope
/// `field:/configuration` + `field:/mergedConfiguration`).
///
/// Removes a key **named in [`ABSENT_OPTIONAL_KEYS`]** when its value carries no
/// information (`null`, `[]`, `{}`, `""`). Nothing else, ever.
///
/// **Why it exists**: deacon serializes every modeled optional property of
/// `devcontainer.json` unconditionally, while the reference omits keys that were not
/// authored. The two documents therefore describe the SAME resolved configuration in
/// different JSON shapes, and without this rule that one serializer difference produces
/// ~48 spurious divergences per corpus case and buries the real ones.
///
/// **Why it is not `prune`**: the removal set is a finite, enumerated list of key names
/// (FR-021). A property added to `DevContainerConfig` tomorrow is NOT on the list, so it
/// surfaces as a divergence rather than vanishing — which is the exact regression
/// `prune` made invisible. The value guard means a populated `appPort` is always
/// compared; only an absent one is elided.
///
/// **It compensates for a deacon defect and is deleted when that defect is fixed** —
/// deacon should apply `skip_serializing_if` so absent optionals are omitted, matching
/// the reference. Tracked in `specs/023-migrate-parity-to-conformance/tasks.md#T111`.
pub fn drop_absent_optional(value: &Value) -> Value {
    // ANCHOR the rule at its declared scope — `field:/configuration` and
    // `field:/mergedConfiguration` — not at whatever object it happens to be handed.
    //
    // Two shapes reach this function. `config`/`merged_config` extract the inner document
    // first, so the value IS the configuration. The `chan-structured-output` channel
    // passes the whole CLI document, where the configuration sits one level down under
    // `configuration` / `mergedConfiguration`. Treating the wrapper as the root would
    // apply the rule at the wrong level in the second case (eliding nothing inside the
    // configuration, and eliding wrapper keys that share a name).
    if let Value::Object(map) = value {
        let wrapped: Vec<&str> = SCOPE_FIELDS
            .iter()
            .copied()
            .filter(|f| map.contains_key(*f))
            .collect();
        if !wrapped.is_empty() {
            let mut out = map.clone();
            for field in wrapped {
                if let Some(inner) = map.get(field) {
                    out.insert(
                        field.to_string(),
                        drop_absent_optional_scoped(inner, DropScope::Root),
                    );
                }
            }
            return Value::Object(out);
        }
    }
    drop_absent_optional_scoped(value, DropScope::Root)
}

/// The document fields this rule is registered against (`field:/configuration`,
/// `field:/mergedConfiguration`). Each names a configuration document root.
const SCOPE_FIELDS: &[&str] = &["configuration", "mergedConfiguration"];

/// The two spec-defined nested containers whose own properties were part of the measured
/// set (see [`ABSENT_OPTIONAL_KEYS`]). The rule applies inside these, and nowhere else
/// below the document root.
const NESTED_CONTAINERS: &[&str] = &["hostRequirements", "portsAttributes"];

/// Where [`drop_absent_optional`] is permitted to elide a key.
#[derive(Clone, Copy, PartialEq, Eq)]
enum DropScope {
    /// The configuration document's own top level.
    Root,
    /// Anywhere inside a [`NESTED_CONTAINERS`] entry (these are small, spec-defined
    /// structures — `portsAttributes` nests its attribute objects one level down).
    Container,
    /// Everything else: arbitrary sub-documents the rule was never measured against.
    Off,
}

/// The scoped implementation.
///
/// **Why this is scoped rather than recursive-everywhere**: the earlier version walked the
/// whole document, so an enumerated key NAME was elided at any depth — including inside
/// `customizations.vscode.settings`, which is arbitrary user data that merely happens to
/// contain keys called `label` or `description`. The key list was measured, but the
/// LOCATION was unbounded, which is the blanket behavior FR-029 forbids and understated
/// what the rule's registered `field:/configuration` + `field:/mergedConfiguration` scope
/// claimed. A rule a reviewer cannot bound by reading its registry entry is not auditable,
/// which is the property V24 exists to provide.
fn drop_absent_optional_scoped(value: &Value, scope: DropScope) -> Value {
    match value {
        Value::Object(map) => {
            let mut out = Map::new();
            for (k, v) in map {
                if scope != DropScope::Off
                    && ABSENT_OPTIONAL_KEYS.contains(&k.as_str())
                    && carries_no_information(v)
                {
                    continue;
                }
                let child = match scope {
                    DropScope::Root if NESTED_CONTAINERS.contains(&k.as_str()) => {
                        DropScope::Container
                    }
                    DropScope::Container => DropScope::Container,
                    _ => DropScope::Off,
                };
                out.insert(k.clone(), drop_absent_optional_scoped(v, child));
            }
            Value::Object(out)
        }
        Value::Array(items) => Value::Array(
            items
                .iter()
                .map(|v| drop_absent_optional_scoped(v, scope))
                .collect(),
        ),
        other => other.clone(),
    }
}

/// Whether a value carries no information: JSON `null`, or an empty array / object /
/// string. Deliberately exact — a `0`, a `false`, or a one-element array all carry
/// information and are always compared.
fn carries_no_information(v: &Value) -> bool {
    match v {
        Value::Null => true,
        Value::Array(a) => a.is_empty(),
        Value::Object(o) => o.is_empty(),
        Value::String(s) => s.is_empty(),
        _ => false,
    }
}

/// **Rule `devcontainer_id_token`** (023 T063, action `rewrite`, scope
/// `field:/configuration` + `field:/mergedConfiguration`).
///
/// Rewrites the literal `${devcontainerId}` template token to `<ID>` everywhere, and a
/// 12-character lowercase-hex run to `<ID>` **only inside the enumerated
/// [`DEVCONTAINER_ID_FIELDS`]** — the properties a substituted devcontainer id can
/// legitimately reach.
///
/// The retired `replace_hex12` rewrote ANY 12-char lowercase-hex run in ANY string in
/// the document. Applied to both sides it could not manufacture a false pass on equal
/// inputs, but it could — and this is the defect research D3 names — collapse two
/// GENUINELY DIFFERENT hex values (a short digest, a hash, a hex-looking identifier) to
/// the same token and mask a real divergence. Scoping the hex rewrite to the fields that
/// actually carry a devcontainer id removes that blast radius; the literal-token rewrite
/// is an exact string match and was never open-ended.
pub fn devcontainer_id_token(value: &Value) -> Value {
    rewrite_ids(value, false)
}

/// Recursive worker: `in_id_field` is true once we are inside one of
/// [`DEVCONTAINER_ID_FIELDS`], which is where the hex rewrite applies.
fn rewrite_ids(value: &Value, in_id_field: bool) -> Value {
    match value {
        Value::String(s) => {
            let literal = s.replace("${devcontainerId}", "<ID>");
            Value::String(if in_id_field {
                tokenize_hex12(&literal)
            } else {
                literal
            })
        }
        Value::Array(items) => {
            Value::Array(items.iter().map(|v| rewrite_ids(v, in_id_field)).collect())
        }
        Value::Object(map) => Value::Object(
            map.iter()
                .map(|(k, v)| {
                    let inside = in_id_field || DEVCONTAINER_ID_FIELDS.contains(&k.as_str());
                    (k.clone(), rewrite_ids(v, inside))
                })
                .collect(),
        ),
        other => other.clone(),
    }
}

fn parse(case: &str, raw: &str) -> Result<Value, HarnessError> {
    serde_json::from_str(raw.trim()).map_err(|e| HarnessError::Normalization {
        case: case.to_string(),
        cause: format!("output is not valid JSON: {e}"),
    })
}

/// Rewrite each 12-char contiguous lowercase-hex run to `<ID>` (char-safe).
///
/// Deliberately NOT named `replace_hex12` any more: that name belonged to the retired
/// BLANKET rule that applied this to every string in the document (023 T063). This is
/// the same mechanism confined to [`DEVCONTAINER_ID_FIELDS`] by
/// [`devcontainer_id_token`], and the name says so — a scoped helper reading as a
/// document-wide replacement is how the blanket behavior would creep back.
fn tokenize_hex12(input: &str) -> String {
    let chars: Vec<char> = input.chars().collect();
    let mut out = String::with_capacity(input.len());
    let mut i = 0;
    while i < chars.len() {
        if i + 12 <= chars.len()
            && chars[i..i + 12]
                .iter()
                .all(|c| matches!(c, '0'..='9' | 'a'..='f'))
        {
            out.push_str("<ID>");
            i += 12;
        } else {
            out.push(chars[i]);
            i += 1;
        }
    }
    out
}

// ===========================================================================
// Configuration diff (ranked): ref-only / value / deacon-only
// ===========================================================================

/// Divergence class. **All three are reported with equal significance** (023 T065,
/// FR-020).
///
/// This enum used to rank `deacon-only` LAST, documented as "usually default noise".
/// That was the deacon-only-as-serialization-noise assumption research D3 identifies as
/// the migration's central normalization defect: a field deacon emits and the reference
/// does not is either a genuine extension or a genuine over-emission, and neither is
/// noise. Combined with `prune` — which deleted such a field outright whenever it
/// happened to be empty — it meant deacon-only data was hidden if empty and buried if
/// populated.
///
/// Ordering is now a **display order only**, chosen for deterministic output, and no
/// class sorts below another on the grounds of being less interesting.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum DiffKind {
    RefOnly,
    DeaconOnly,
    Value,
}

impl DiffKind {
    /// The stable wire spelling, used for deterministic ordering and reporting.
    pub fn as_str(self) -> &'static str {
        match self {
            DiffKind::RefOnly => "ref-only",
            DiffKind::DeaconOnly => "deacon-only",
            DiffKind::Value => "value",
        }
    }
}

/// A single normalized-config divergence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigDivergence {
    pub kind: DiffKind,
    pub path: String,
    pub deacon: Option<Value>,
    pub reference: Option<Value>,
}

/// Diff two normalized configs.
///
/// Output order is `(class, path)` — deterministic, and NOT a significance ranking: every
/// class is reported, none is de-prioritized (023 T065, FR-020).
pub fn diff(deacon: &Value, reference: &Value) -> Vec<ConfigDivergence> {
    let mut out = Vec::new();
    diff_rec(deacon, reference, "", &mut out);
    out.sort_by(|a, b| a.kind.cmp(&b.kind).then_with(|| a.path.cmp(&b.path)));
    out
}

fn diff_rec(d: &Value, r: &Value, path: &str, out: &mut Vec<ConfigDivergence>) {
    match (d, r) {
        (Value::Object(dm), Value::Object(rm)) => {
            let keys: BTreeSet<&String> = dm.keys().chain(rm.keys()).collect();
            for k in keys {
                let p = if path.is_empty() {
                    k.clone()
                } else {
                    format!("{path}.{k}")
                };
                match (dm.get(k), rm.get(k)) {
                    (Some(dv), None) => out.push(ConfigDivergence {
                        kind: DiffKind::DeaconOnly,
                        path: p,
                        deacon: Some(dv.clone()),
                        reference: None,
                    }),
                    (None, Some(rv)) => out.push(ConfigDivergence {
                        kind: DiffKind::RefOnly,
                        path: p,
                        deacon: None,
                        reference: Some(rv.clone()),
                    }),
                    (Some(dv), Some(rv)) => diff_rec(dv, rv, &p, out),
                    (None, None) => unreachable!("key came from the union of both maps"),
                }
            }
        }
        _ => {
            if d != r {
                out.push(ConfigDivergence {
                    kind: DiffKind::Value,
                    path: path.to_string(),
                    deacon: Some(d.clone()),
                    reference: Some(r.clone()),
                });
            }
        }
    }
}

/// A compact, ranked, human-readable summary of config divergences (used for the
/// report fragment's `diff_summary` and the test failure message).
pub fn summarize(divs: &[ConfigDivergence]) -> String {
    fn snip(v: &Value) -> String {
        let s = v.to_string();
        if s.len() > 200 {
            format!("{}…", &s[..200])
        } else {
            s
        }
    }
    let mut lines = Vec::new();
    for d in divs {
        let loc = if d.path.is_empty() { "<root>" } else { &d.path };
        match d.kind {
            DiffKind::RefOnly => lines.push(format!(
                "ref-only    {loc} = {} (deacon drops this)",
                d.reference.as_ref().map(snip).unwrap_or_default()
            )),
            DiffKind::Value => lines.push(format!(
                "value       {loc}: deacon={} ref={}",
                d.deacon.as_ref().map(snip).unwrap_or_default(),
                d.reference.as_ref().map(snip).unwrap_or_default()
            )),
            DiffKind::DeaconOnly => lines.push(format!(
                "deacon-only {loc} = {} (the reference does not emit this)",
                d.deacon.as_ref().map(snip).unwrap_or_default()
            )),
        }
    }
    lines.join("\n")
}

// ===========================================================================
// Container observable-state normalization (observable-state parity)
//
// Ported verbatim (semantics-preserving) from the sole prior implementation in
// crates/deacon/tests/parity_utils.rs (L488–981): noise-env subtraction,
// intentional-label-prefix subtraction, compose project-prefix stripping, and
// user normalization. The KNOWN_* const classifier lists are intentionally NOT
// ported — divergence classification moves to the waiver system (US2).
// ===========================================================================

/// Env keys present in every container / runtime-injected; not meaningful for
/// cross-CLI outcome parity. Subtracted before diffing env.
pub const NOISE_ENV_KEYS: &[&str] = &["PATH", "HOME", "HOSTNAME", "TERM", "container"];

/// **NAMED, SCOPED legacy rule `drop_noise_env` — chan-container-state ONLY** (research
/// D6, FR-029). Whether `key` is a runtime-injected env var present in every container
/// with no cross-CLI outcome meaning ([`NOISE_ENV_KEYS`]). This is the ONLY sanctioned
/// env subtraction, scoped to the legacy observable-state channel and carrying the
/// rationale above. The NEW per-channel `chan-injected-process` normalization
/// ([`path_env_segmented`] + [`null_preserving`]) NEVER blanket-removes env — it
/// preserves every var and characterizes intentional differences via scoped
/// allowed-differences (US4), never a blanket ignore list.
pub fn is_noise_env_key(key: &str) -> bool {
    NOISE_ENV_KEYS.contains(&key)
}

/// Normalized single-mount state.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MountState {
    pub mount_type: String,
    pub ro: bool,
    /// Normalized source descriptor for REPORTING only (bind: leaf component;
    /// volume: name with compose-project prefix stripped). NOT compared.
    pub source_tail: String,
}

/// Normalized snapshot of a container's observable state.
///
/// `Serialize` (camelCase, ordered maps/sets) is what the declarative
/// `chan-container-state` observer emits: the observer DELEGATES to
/// [`container_state`] rather than re-deriving any of this, so the legacy comparison and
/// the declarative channel read one definition of container state (Constitution VIII).
/// `BTreeMap`/`BTreeSet` give byte-stable field ordering for free.
#[derive(Debug, Clone, PartialEq, Eq, Default, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StateSnapshot {
    /// destination -> mount state
    pub mounts: BTreeMap<String, MountState>,
    /// `KEY=VALUE` entries, noise keys removed
    pub env: BTreeSet<String>,
    /// Every label, VERBATIM (024 Phase 4). The retired `strip_intentional_labels` rule
    /// removed four label NAMESPACES by prefix — a category, not an enumerated set, so
    /// any label a future release added under `devcontainer.` / `com.docker.` /
    /// `desktop.` / `dev.containers.` would have vanished from the comparison silently
    /// (FR-021). Identity/bookkeeping labels the two CLIs stamp differently are now
    /// characterized where a reader can see them — a scoped, backed `allowedDifference`
    /// on the case — rather than elided inside the normalizer.
    pub labels: BTreeMap<String, String>,
    pub user: String,
    pub working_dir: String,
    /// `Config.ExposedPorts` keys (image `EXPOSE` + declared), e.g. `3000/tcp`.
    pub exposed_ports: BTreeSet<String>,
    /// `HostConfig.PortBindings` keys actually PUBLISHED to the host.
    pub published_ports: BTreeSet<String>,
    /// The container process shape: a keep-alive/entrypoint-wrapper detail with no
    /// observable behavioral difference — both CLIs keep the container running so
    /// `exec`, lifecycle hooks and feature entrypoints work identically. deacon uses a
    /// PATH-robust `sh -c '… sleep infinity || tail -f /dev/null'`; the reference an
    /// `exec "$@"` keep-alive loop. Intentional, characterized divergence (#290); the
    /// behaviorally-significant cases (overrideCommand exit #291, feature entrypoint
    /// composition #292) ARE observable and covered elsewhere.
    ///
    /// EMITTED on `chan-container-state` and therefore COMPARED (024 Phase 4). The legacy
    /// `diff_states` documented it as "captured but NOT diffed" — an undeclared
    /// non-comparison, invisible to anyone reading the case. A declarative case declares
    /// the tolerance instead, so the elision is visible, backed and stale-checked.
    pub entrypoint: Vec<String>,
    /// The container command — see [`StateSnapshot::entrypoint`] (#290): emitted and
    /// compared, with any characterized difference declared on the case.
    pub cmd: Vec<String>,
    /// Network names (compose-project prefix normalized) — emitted and compared, with the
    /// project-naming difference characterized on the case
    /// (`bhv-compose-project-name-robust`) rather than silently not diffed.
    pub networks: BTreeSet<String>,
}

/// A single field-level observable-state divergence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Divergence {
    /// Stable field identifier, e.g. `mount:/feat-mnt`, `env:FOO`, `user`.
    pub field: String,
    pub detail: String,
}

/// Build a normalized snapshot from a single `docker inspect` object. Pure —
/// unit-testable without Docker. A missing `Config` object is a normalization
/// failure (never a silent empty snapshot).
pub fn container_state(case: &str, raw: &Value) -> Result<StateSnapshot, HarnessError> {
    if raw.get("Config").and_then(Value::as_object).is_none() {
        return Err(HarnessError::Normalization {
            case: case.to_string(),
            cause: format!("docker inspect object has no Config object; got: {raw}"),
        });
    }

    let project = raw["Config"]["Labels"]["com.docker.compose.project"]
        .as_str()
        .unwrap_or("")
        .to_string();

    let mut mounts = BTreeMap::new();
    if let Some(arr) = raw["Mounts"].as_array() {
        for m in arr {
            let dest = m["Destination"].as_str().unwrap_or("").to_string();
            if dest.is_empty() {
                continue;
            }
            let mount_type = m["Type"].as_str().unwrap_or("").to_string();
            let ro = !m["RW"].as_bool().unwrap_or(true);
            let source_tail = if mount_type == "volume" {
                strip_project_prefix(m["Name"].as_str().unwrap_or(""), &project)
            } else if mount_type == "bind" {
                Path::new(m["Source"].as_str().unwrap_or(""))
                    .file_name()
                    .and_then(|s| s.to_str())
                    .unwrap_or("")
                    .to_string()
            } else {
                String::new()
            };
            mounts.insert(
                dest,
                MountState {
                    mount_type,
                    ro,
                    source_tail,
                },
            );
        }
    }

    // Legacy chan-container-state env subtraction via the NAMED, SCOPED rule
    // `drop_noise_env` ([`is_noise_env_key`]) — the only sanctioned env removal (D6).
    let env = str_array(&raw["Config"]["Env"])
        .into_iter()
        .filter(|e| {
            let key = e.split_once('=').map(|(k, _)| k).unwrap_or(e.as_str());
            !is_noise_env_key(key)
        })
        .collect();

    // Labels VERBATIM (024 Phase 4): the `strip_intentional_labels` prefix-drop is
    // retired. Capture removes nothing; a characterized identity-label difference is
    // declared on the case that compares it, never elided here.
    let labels = raw["Config"]["Labels"]
        .as_object()
        .map(|o| {
            o.iter()
                .map(|(k, v)| (k.clone(), v.as_str().unwrap_or("").to_string()))
                .collect()
        })
        .unwrap_or_default();

    let exposed_ports = raw["Config"]["ExposedPorts"]
        .as_object()
        .map(|o| o.keys().cloned().collect())
        .unwrap_or_default();

    let published_ports = raw["HostConfig"]["PortBindings"]
        .as_object()
        .map(|o| {
            o.iter()
                .filter(|(_, v)| v.as_array().is_some_and(|a| !a.is_empty()))
                .map(|(k, _)| k.clone())
                .collect()
        })
        .unwrap_or_default();

    let networks = raw["NetworkSettings"]["Networks"]
        .as_object()
        .map(|o| {
            o.keys()
                .map(|k| strip_project_prefix(k, &project))
                .collect()
        })
        .unwrap_or_default();

    Ok(StateSnapshot {
        mounts,
        env,
        labels,
        user: raw["Config"]["User"].as_str().unwrap_or("").to_string(),
        working_dir: raw["Config"]["WorkingDir"]
            .as_str()
            .unwrap_or("")
            .to_string(),
        exposed_ports,
        published_ports,
        entrypoint: str_array(&raw["Config"]["Entrypoint"]),
        cmd: str_array(&raw["Config"]["Cmd"]),
        networks,
    })
}

fn str_array(v: &Value) -> Vec<String> {
    v.as_array()
        .map(|a| {
            a.iter()
                .filter_map(|x| x.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default()
}

fn strip_project_prefix(name: &str, project: &str) -> String {
    if !project.is_empty() {
        if let Some(rest) = name.strip_prefix(&format!("{project}_")) {
            return rest.to_string();
        }
    }
    name.to_string()
}

/// An empty `Config.User` means "image default" (root for the Linux bases used
/// here); treat "" and "root" as equivalent so a cosmetic difference is not
/// flagged, while a real non-root `remoteUser`/`containerUser` still diverges.
fn norm_user(u: &str) -> &str {
    if u.is_empty() { "root" } else { u }
}

fn env_map(set: &BTreeSet<String>) -> BTreeMap<String, String> {
    set.iter()
        .map(|e| match e.split_once('=') {
            Some((k, v)) => (k.to_string(), v.to_string()),
            None => (e.clone(), String::new()),
        })
        .collect()
}

/// Field-by-field diff of two normalized snapshots (mounts by
/// destination+type+read-only, env by key, labels by key, exposed/published
/// ports as sets, scalar user/working_dir). Deliberately does NOT compare mount
/// SOURCES, cmd/entrypoint, or networks — see the [`StateSnapshot`] field docs.
pub fn diff_states(deacon: &StateSnapshot, upstream: &StateSnapshot) -> Vec<Divergence> {
    let mut out = Vec::new();

    let dests: BTreeSet<&String> = deacon.mounts.keys().chain(upstream.mounts.keys()).collect();
    for dest in dests {
        match (deacon.mounts.get(dest), upstream.mounts.get(dest)) {
            (Some(d), Some(u)) => {
                if d.mount_type != u.mount_type {
                    out.push(Divergence {
                        field: format!("mount:{dest}"),
                        detail: format!(
                            "type differs: deacon={} upstream={}",
                            d.mount_type, u.mount_type
                        ),
                    });
                }
                if d.ro != u.ro {
                    out.push(Divergence {
                        field: format!("mount:{dest}"),
                        detail: format!("read-only differs: deacon={} upstream={}", d.ro, u.ro),
                    });
                }
            }
            (Some(d), None) => out.push(Divergence {
                field: format!("mount:{dest}"),
                detail: format!("present on deacon ({}), absent upstream", d.mount_type),
            }),
            (None, Some(u)) => out.push(Divergence {
                field: format!("mount:{dest}"),
                detail: format!("present upstream ({}), absent deacon", u.mount_type),
            }),
            (None, None) => unreachable!("dest came from the union of both maps"),
        }
    }

    diff_kv(
        "env",
        &env_map(&deacon.env),
        &env_map(&upstream.env),
        &mut out,
    );
    diff_kv("label", &deacon.labels, &upstream.labels, &mut out);

    for p in deacon.exposed_ports.difference(&upstream.exposed_ports) {
        out.push(Divergence {
            field: format!("port:{p}"),
            detail: "exposed on deacon, not upstream".to_string(),
        });
    }
    for p in upstream.exposed_ports.difference(&deacon.exposed_ports) {
        out.push(Divergence {
            field: format!("port:{p}"),
            detail: "exposed upstream, not deacon".to_string(),
        });
    }

    for p in deacon.published_ports.difference(&upstream.published_ports) {
        out.push(Divergence {
            field: format!("pubport:{p}"),
            detail: "published on deacon, not upstream".to_string(),
        });
    }
    for p in upstream.published_ports.difference(&deacon.published_ports) {
        out.push(Divergence {
            field: format!("pubport:{p}"),
            detail: "published upstream, not deacon".to_string(),
        });
    }

    if norm_user(&deacon.user) != norm_user(&upstream.user) {
        out.push(Divergence {
            field: "user".to_string(),
            detail: format!("deacon={:?} upstream={:?}", deacon.user, upstream.user),
        });
    }
    if deacon.working_dir != upstream.working_dir {
        out.push(Divergence {
            field: "workingdir".to_string(),
            detail: format!(
                "deacon={:?} upstream={:?}",
                deacon.working_dir, upstream.working_dir
            ),
        });
    }

    out
}

fn diff_kv(
    kind: &str,
    deacon: &BTreeMap<String, String>,
    upstream: &BTreeMap<String, String>,
    out: &mut Vec<Divergence>,
) {
    let keys: BTreeSet<&String> = deacon.keys().chain(upstream.keys()).collect();
    for k in keys {
        match (deacon.get(k), upstream.get(k)) {
            (Some(dv), Some(uv)) => {
                if dv != uv {
                    out.push(Divergence {
                        field: format!("{kind}:{k}"),
                        detail: format!("value differs: deacon={dv:?} upstream={uv:?}"),
                    });
                }
            }
            (Some(dv), None) => out.push(Divergence {
                field: format!("{kind}:{k}"),
                detail: format!("present on deacon ({dv:?}), absent upstream"),
            }),
            (None, Some(uv)) => out.push(Divergence {
                field: format!("{kind}:{k}"),
                detail: format!("present upstream ({uv:?}), absent deacon"),
            }),
            (None, None) => unreachable!("key came from the union of both maps"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    use deacon_conformance::model::{
        CHAN_EXIT_CODE, CHAN_IMAGE, CHAN_INJECTED_PROCESS, CHAN_PROCESS_GRAPH, CHAN_STDOUT,
        CHAN_STRUCTURED_OUTPUT,
    };

    fn raw(channel: &str, value: Value) -> RawChannelEvidence {
        RawChannelEvidence {
            channel: channel.to_string(),
            operation: "op-1".to_string(),
            present: true,
            value,
        }
    }

    // -- T039: path_token rewrites (never deletes); null states distinct; no removal ---

    #[test]
    fn path_token_rewrites_temp_paths_to_a_stable_token_without_deleting() {
        let tokens = TokenMap::workspace(std::path::Path::new("/tmp/ws-abc"));
        let value = json!({
            "rootFolderPath": "/tmp/ws-abc",
            "mount": "source=/tmp/ws-abc/proj,target=/w",
            "nested": ["/tmp/ws-abc/x", "/unrelated/y"],
            "null_val": null, "empty_str": "", "empty_arr": [], "empty_obj": {}
        });
        let out = path_token(&value, &tokens);
        assert_eq!(out["rootFolderPath"], json!("<WORKSPACE>"));
        assert_eq!(out["mount"], json!("source=<WORKSPACE>/proj,target=/w"));
        assert_eq!(out["nested"][0], json!("<WORKSPACE>/x"));
        assert_eq!(out["nested"][1], json!("/unrelated/y"), "rewrite is scoped");
        // The four null states remain DISTINCT and PRESENT (FR-025) — none deleted.
        assert_eq!(out["null_val"], Value::Null);
        assert_eq!(out["empty_str"], json!(""));
        assert_eq!(out["empty_arr"], json!([]));
        assert_eq!(out["empty_obj"], json!({}));
        let obj = out.as_object().unwrap();
        for k in ["null_val", "empty_str", "empty_arr", "empty_obj"] {
            assert!(obj.contains_key(k), "field {k} must not be dropped");
        }
    }

    #[test]
    fn normalize_channel_preserves_present_false() {
        let not_captured = RawChannelEvidence {
            channel: CHAN_STRUCTURED_OUTPUT.to_string(),
            operation: "op-1".to_string(),
            present: false,
            value: Value::Null,
        };
        let out = normalize_channel(CHAN_STRUCTURED_OUTPUT, &not_captured, &TokenMap::new());
        assert!(
            !out.present,
            "present:false (not captured) is preserved (FR-018)"
        );
    }

    #[test]
    fn no_channel_blanket_removes_env_labels_mounts_entrypoint_command_networks() {
        let tokens = TokenMap::new();
        // Image: labels + env + entrypoint all survive normalization.
        let img = raw(
            CHAN_IMAGE,
            json!({ "labels": {"a":"1"}, "env": ["A=1"], "entrypoint": ["/bin/sh"] }),
        );
        let n = normalize_channel(CHAN_IMAGE, &img, &tokens);
        assert!(n.value.get("labels").is_some() && n.value.get("env").is_some());
        assert!(
            n.value.get("entrypoint").is_some(),
            "entrypoint not removed"
        );
        // Process graph: mounts + networks + volumes survive.
        let graph = raw(
            CHAN_PROCESS_GRAPH,
            json!({ "mounts": [{"source":"/s","target":"/t"}], "networks": ["n"], "volumes": ["v"] }),
        );
        let g = normalize_channel(CHAN_PROCESS_GRAPH, &graph, &tokens);
        assert_eq!(g.value["mounts"].as_array().unwrap().len(), 1);
        assert!(g.value.get("networks").is_some() && g.value.get("volumes").is_some());
        // Injected process: env + command survive.
        let inj = raw(
            CHAN_INJECTED_PROCESS,
            json!({ "env": {"A":"1"}, "command": ["run"], "path": "/usr/bin:/bin" }),
        );
        let i = normalize_channel(CHAN_INJECTED_PROCESS, &inj, &tokens);
        assert!(i.value.get("env").is_some() && i.value.get("command").is_some());
    }

    // -- T040: label_semantic / mount_source_canonical / path_env_segmented ------------

    #[test]
    fn label_semantic_parses_and_compares_key_value() {
        let as_array = label_semantic(&json!(["k1=v1", "k2=v2"]));
        let as_object = json!({ "k1": "v1", "k2": "v2" });
        assert_eq!(
            as_array, as_object,
            "labels compare semantically, not as strings"
        );
        // Already-object labels pass through unchanged; nothing removed.
        assert_eq!(label_semantic(&as_object), as_object);
    }

    #[test]
    fn mount_source_canonical_makes_temp_path_mounts_equal() {
        let mut tokens = TokenMap::new();
        tokens.insert("/tmp/ws-abc", "<WORKSPACE>");
        tokens.insert("/tmp/ws-def", "<WORKSPACE>");
        let a = mount_source_canonical(
            &json!([{ "source": "/tmp/ws-abc/proj", "target": "/w" }]),
            &tokens,
        );
        let b = mount_source_canonical(
            &json!([{ "source": "/tmp/ws-def/proj", "target": "/w" }]),
            &tokens,
        );
        assert_eq!(
            a, b,
            "two mounts differing only by temp path compare equal (FR-027)"
        );
        assert_eq!(a[0]["source"], json!("<WORKSPACE>/proj"));
    }

    #[test]
    fn path_env_segmented_compares_segment_wise() {
        let tokens = TokenMap::new();
        let out = path_env_segmented(&json!("/usr/local/bin:/usr/bin:/bin"), &tokens);
        assert_eq!(out, json!(["/usr/local/bin", "/usr/bin", "/bin"]));
        // An array PATH normalizes to the same segmented form → segment-wise equality.
        let from_array =
            path_env_segmented(&json!(["/usr/local/bin", "/usr/bin", "/bin"]), &tokens);
        assert_eq!(out, from_array);
    }

    #[test]
    fn normalize_channel_applies_per_channel_rules() {
        let tokens = TokenMap::workspace(std::path::Path::new("/tmp/ws"));
        // structured-output: paths tokenized, structure preserved.
        let s = normalize_channel(
            CHAN_STRUCTURED_OUTPUT,
            &raw(
                CHAN_STRUCTURED_OUTPUT,
                json!({ "root": "/tmp/ws", "keep": null }),
            ),
            &tokens,
        );
        assert_eq!(s.value["root"], json!("<WORKSPACE>"));
        assert_eq!(s.value["keep"], Value::Null, "null preserved");
        // exit-code: no rule (a number is untouched).
        let e = normalize_channel(CHAN_EXIT_CODE, &raw(CHAN_EXIT_CODE, json!(0)), &tokens);
        assert_eq!(e.value, json!(0));
        // stdout: path_token on the string.
        let o = normalize_channel(
            CHAN_STDOUT,
            &raw(CHAN_STDOUT, json!("at /tmp/ws/x")),
            &tokens,
        );
        assert_eq!(o.value, json!("at <WORKSPACE>/x"));
    }

    #[test]
    fn normalizer_version_is_bumped_for_named_rules() {
        assert_eq!(
            NORMALIZER_VERSION, "5",
            "the 024 review bounded `drop_absent_optional` to the document root plus \
             `hostRequirements`/`portsAttributes`; it previously elided an enumerated key \
             name at ANY depth, including inside `customizations` — a change to what \
             \"equal\" means, so every recorded snapshot must go stale and be re-reviewed"
        );
    }

    #[test]
    fn legacy_noise_rules_are_named_and_scoped() {
        assert!(is_noise_env_key("PATH") && !is_noise_env_key("MY_VAR"));
    }

    #[test]
    fn container_state_captures_every_label_verbatim() {
        // The retired `strip_intentional_labels` removed four label NAMESPACES by prefix.
        // Capture now removes NOTHING: an identity label both CLIs stamp differently is
        // characterized on the case that compares it, where a reader can see it.
        let snap = container_state(
            "labels",
            &json!({
                "Config": {
                    "User": "",
                    "Labels": {
                        "devcontainer.local_folder": "/tmp/ws",
                        "com.docker.compose.project": "p",
                        "desktop.docker.io/x": "1",
                        "dev.containers.id": "abc",
                        "org.opencontainers.image.title": "demo"
                    }
                }
            }),
        )
        .expect("snapshot");
        assert_eq!(
            snap.labels.len(),
            5,
            "every label survives capture: {:?}",
            snap.labels
        );
        for key in [
            "devcontainer.local_folder",
            "com.docker.compose.project",
            "desktop.docker.io/x",
            "dev.containers.id",
        ] {
            assert!(
                snap.labels.contains_key(key),
                "{key} must NOT be dropped by the normalizer"
            );
        }
    }

    #[test]
    fn workspace_basename_token_normalizes_two_different_temp_workspaces() {
        // The whole point: each side runs in its OWN temp workspace, so the container
        // path derived from the basename differs. `TokenMap::workspace` alone cannot
        // reach it (the container path never contains the host path), so the two sides
        // would diverge on a mount KEY that is an artifact of the runner's isolation.
        use deacon_conformance::model::CHAN_CONTAINER_STATE;
        let state = |name: &str| {
            json!({
                "mounts": { format!("/workspaces/{name}"): { "mountType": "bind", "ro": false, "sourceTail": name } },
                "workspaceBindTargets": [format!("/workspaces/{name}")]
            })
        };
        let a = apply_channel_rules(
            CHAN_CONTAINER_STATE,
            &state("deacon-conf-aaa"),
            &tokens_for_channel(CHAN_CONTAINER_STATE, Path::new("/tmp/deacon-conf-aaa")),
        );
        let b = apply_channel_rules(
            CHAN_CONTAINER_STATE,
            &state("deacon-conf-bbb"),
            &tokens_for_channel(CHAN_CONTAINER_STATE, Path::new("/tmp/deacon-conf-bbb")),
        );
        assert_eq!(a, b, "two temp workspaces must normalize equal");
        assert_eq!(
            a["workspaceBindTargets"],
            json!(["/workspaces/<WORKSPACE_NAME>"]),
            "the basename is TOKENIZED, not removed: {a}"
        );

        // Only this channel gets the basename token — no other channel's meaning changes.
        let plain = tokens_for_channel(CHAN_STDOUT, Path::new("/tmp/deacon-conf-aaa"));
        assert_eq!(
            path_token(&json!("/workspaces/deacon-conf-aaa"), &plain),
            json!("/workspaces/deacon-conf-aaa"),
            "chan-stdout keeps the plain full-path token map"
        );
    }

    #[test]
    fn drop_absent_optional_removes_only_enumerated_absent_keys() {
        let raw = r#"{
            "configFilePath": "/x/.devcontainer/devcontainer.json",
            "name": "demo",
            "image": "",
            "customizations": {},
            "forwardPorts": [],
            "appPort": null,
            "unlistedEmpty": {},
            "unlistedNull": null,
            "hostRequirements": { "gpu": null, "cpus": 4 },
            "list_keeps_nulls": [1, null, ""]
        }"#;
        let normalized = config("drop", raw).expect("normalize");
        let obj = normalized.as_object().expect("object");

        // Enumerated + absent → removed.
        for key in ["image", "customizations", "forwardPorts", "appPort"] {
            assert!(!obj.contains_key(key), "{key} should be elided");
        }
        // Nested enumerated + absent → removed; its populated sibling survives.
        assert_eq!(obj.get("hostRequirements"), Some(&json!({ "cpus": 4 })));

        // NOT enumerated → preserved, even when empty. This is the property `prune`
        // destroyed.
        assert_eq!(obj.get("unlistedEmpty"), Some(&json!({})));
        assert_eq!(obj.get("unlistedNull"), Some(&json!(null)));
        // No longer dropped: `configFilePath` is a compared value now.
        assert_eq!(
            obj.get("configFilePath"),
            Some(&json!("/x/.devcontainer/devcontainer.json"))
        );
        assert_eq!(obj.get("name"), Some(&json!("demo")));
        // List elements are preserved verbatim.
        assert_eq!(obj.get("list_keeps_nulls"), Some(&json!([1, null, ""])));
    }

    /// The rule's REACH is bounded, not just its key list (024 review finding).
    ///
    /// An enumerated key name outside the document root and the two nested containers is
    /// compared, not elided. Before this was scoped, the walk was unbounded: a `label: ""`
    /// inside `customizations.vscode.settings` — arbitrary user data — was dropped merely
    /// for sharing a name with a modeled property, so the registered
    /// `field:/configuration` scope understated what the rule removed.
    #[test]
    fn drop_absent_optional_is_bounded_to_the_root_and_named_containers() {
        let raw = r#"{
            "image": "",
            "customizations": { "vscode": { "settings": { "label": "", "init": null } } },
            "hostRequirements": { "gpu": null, "cpus": 4 },
            "portsAttributes": { "3000": { "label": "", "protocol": "https" } }
        }"#;
        let obj = config("bounded", raw).expect("normalize");

        // Root: enumerated + absent → elided.
        assert!(!obj.as_object().expect("object").contains_key("image"));

        // Inside an arbitrary sub-document → PRESERVED, despite the names being on the
        // list. This is the property the unbounded walk destroyed.
        assert_eq!(
            obj.pointer("/customizations/vscode/settings"),
            Some(&json!({ "label": "", "init": null })),
        );

        // Inside the two named containers → still elided, at whatever depth they nest.
        assert_eq!(obj.pointer("/hostRequirements"), Some(&json!({"cpus": 4})));
        assert_eq!(
            obj.pointer("/portsAttributes/3000"),
            Some(&json!({ "protocol": "https" })),
        );
    }

    #[test]
    fn the_enumerated_key_list_is_sorted_and_unique() {
        // The list IS the safety property; keeping it sorted and duplicate-free keeps it
        // reviewable.
        let mut sorted = ABSENT_OPTIONAL_KEYS.to_vec();
        sorted.sort_unstable();
        assert_eq!(ABSENT_OPTIONAL_KEYS, sorted.as_slice());
        let unique: std::collections::BTreeSet<&&str> = ABSENT_OPTIONAL_KEYS.iter().collect();
        assert_eq!(unique.len(), ABSENT_OPTIONAL_KEYS.len());
    }

    #[test]
    fn config_unwraps_configuration_wrapper() {
        let wrapped = r#"{ "configuration": { "name": "x" }, "configFilePath": "/p" }"#;
        let bare = r#"{ "name": "x" }"#;
        assert_eq!(
            config("w", wrapped).unwrap(),
            config("b", bare).unwrap(),
            "the reference's {{configuration}} wrapper must be unwrapped to match deacon's bare output"
        );
    }

    #[test]
    fn dynamic_id_token_rewrites_the_literal_everywhere() {
        // The literal `${devcontainerId}` is an EXACT string match, never a pattern, so
        // it is safe to rewrite anywhere.
        let raw = r#"{ "a": "id-${devcontainerId}-x", "name": "n-${devcontainerId}" }"#;
        let n = config("dyn", raw).unwrap();
        assert_eq!(n["a"], json!("id-<ID>-x"));
        assert_eq!(n["name"], json!("n-<ID>"));
    }

    #[test]
    fn hex_rewrite_is_confined_to_the_enumerated_id_fields() {
        // 023 T063: the retired `replace_hex12` rewrote ANY 12-char lowercase-hex run in
        // ANY string, which could collapse two genuinely different digests to one token
        // and mask a divergence. It now applies ONLY inside DEVCONTAINER_ID_FIELDS.
        let raw = r#"{
            "mounts": [ "source=vol_0123456789ab_tail,target=/d,type=volume" ],
            "customizations": { "vscode": { "digest": "0123456789ab" } }
        }"#;
        let n = config("dyn", raw).unwrap();
        assert_eq!(
            n["mounts"][0],
            json!("source=vol_<ID>_tail,target=/d,type=volume"),
            "a devcontainer id inside `mounts` is still tokenized"
        );
        assert_eq!(
            n["customizations"]["vscode"]["digest"],
            json!("0123456789ab"),
            "a hex value OUTSIDE the enumerated fields is left alone, so two different \
             digests still compare unequal"
        );

        // The masking the narrow rule prevents: two DIFFERENT digests outside the id
        // fields must still diverge.
        let a = config("a", r#"{ "customizations": { "d": "0123456789ab" } }"#).unwrap();
        let b = config("b", r#"{ "customizations": { "d": "ffffffffffff" } }"#).unwrap();
        assert_eq!(diff(&a, &b).len(), 1, "distinct digests must not collapse");
    }

    #[test]
    fn normalization_failure_on_invalid_json() {
        let err = config("bad", "{ not json").expect_err("must fail");
        assert!(matches!(err, HarnessError::Normalization { .. }));
        // merged_config on a non-object also fails, not falls back.
        assert!(matches!(
            merged_config("arr", "[1,2,3]"),
            Err(HarnessError::Normalization { .. })
        ));
    }

    #[test]
    fn merged_config_extracts_block() {
        // `empty` is NOT on the enumerated list, so it survives — only listed keys with
        // an absent value are elided (023 T062).
        let raw = r#"{ "configuration": {"name":"x"}, "mergedConfiguration": { "onCreateCommand": "echo hi", "empty": {}, "image": null } }"#;
        let n = merged_config("m", raw).unwrap();
        assert_eq!(n, json!({ "onCreateCommand": "echo hi", "empty": {} }));
    }

    #[test]
    fn diff_order_is_deterministic_and_deacon_only_is_not_last() {
        let deacon = json!({ "name": "x", "extra": 1 });
        let reference = json!({ "name": "y", "dropped": 2 });
        let divs = diff(&deacon, &reference);
        let kinds: Vec<_> = divs.iter().map(|d| d.kind).collect();
        // 023 T065 / FR-020: `deacon-only` no longer sorts below `value` as "default
        // noise" — the order is a deterministic display order, nothing more.
        assert_eq!(
            kinds,
            vec![DiffKind::RefOnly, DiffKind::DeaconOnly, DiffKind::Value]
        );
        assert_eq!(divs[0].path, "dropped");
        let summary = summarize(&divs);
        assert!(summary.contains("ref-only"));
        assert!(summary.contains("deacon drops this"));
        assert!(
            summary.contains("the reference does not emit this"),
            "deacon-only must be reported as a finding, not shrugged off: {summary}"
        );
    }

    #[test]
    fn an_absent_enumerated_optional_no_longer_diverges() {
        // `image` IS on the enumerated list: absent on one side, omitted on the other →
        // the same resolved configuration, so no divergence.
        let a = config("a", r#"{ "name": "x", "image": null }"#).unwrap();
        let b = config("b", r#"{ "name": "x" }"#).unwrap();
        assert!(diff(&a, &b).is_empty());
    }

    #[test]
    fn an_absent_key_not_on_the_enumerated_list_still_diverges() {
        // The whole safety property of retiring `prune` (023 T062): an unlisted key is
        // COMPARED even when empty, so a newly added property cannot vanish silently.
        let a = config("a", r#"{ "name": "x", "someNewProperty": null }"#).unwrap();
        let b = config("b", r#"{ "name": "x" }"#).unwrap();
        let divs = diff(&a, &b);
        assert_eq!(divs.len(), 1, "{divs:?}");
        assert_eq!(divs[0].kind, DiffKind::DeaconOnly);
        assert_eq!(divs[0].path, "someNewProperty");
    }

    #[test]
    fn a_populated_enumerated_optional_is_always_compared() {
        // The value guard: the rule elides an ABSENT `appPort`, never a populated one.
        let a = config("a", r#"{ "appPort": [3000] }"#).unwrap();
        let b = config("b", r#"{ }"#).unwrap();
        let divs = diff(&a, &b);
        assert_eq!(divs.len(), 1);
        assert_eq!(divs[0].path, "appPort");
    }

    #[test]
    fn config_file_path_is_no_longer_dropped() {
        // `prune` removed `configFilePath` unconditionally; it is now a compared value,
        // so the reference emitting it and deacon not is REPORTED (research D3).
        let deacon = config("a", r#"{ "name": "x" }"#).unwrap();
        let reference = config(
            "b",
            r#"{ "name": "x", "configFilePath": "/w/.devcontainer/devcontainer.json" }"#,
        )
        .unwrap();
        let divs = diff(&deacon, &reference);
        assert_eq!(divs.len(), 1, "{divs:?}");
        assert_eq!(divs[0].kind, DiffKind::RefOnly);
        assert_eq!(divs[0].path, "configFilePath");
    }

    #[test]
    fn container_state_missing_config_is_normalization_error() {
        let err = container_state("nostate", &json!({ "Mounts": [] })).expect_err("must fail");
        assert!(matches!(err, HarnessError::Normalization { .. }));
    }

    #[test]
    fn container_state_subtracts_only_the_enumerated_noise_env() {
        let inspect = json!({
            "Config": {
                "Env": ["PATH=/bin", "FOO=bar", "HOME=/root"],
                "Labels": {
                    "devcontainer.local_folder": "/ws",
                    "com.docker.compose.project": "proj",
                    "my.app.tier": "web"
                },
                "User": "",
                "WorkingDir": "/workspace"
            },
            "Mounts": [
                { "Destination": "/workspace", "Type": "bind", "RW": true, "Source": "/tmp/abc/ws" }
            ]
        });
        let snap = container_state("state", &inspect).expect("snapshot");
        // Noise env keys removed; meaningful ones kept.
        assert!(snap.env.contains("FOO=bar"));
        assert!(!snap.env.iter().any(|e| e.starts_with("PATH=")));
        assert!(!snap.env.iter().any(|e| e.starts_with("HOME=")));
        // EVERY label is kept — the app label AND the CLI-namespaced ones (024 Phase 4;
        // see `container_state_captures_every_label_verbatim`).
        assert_eq!(
            snap.labels.get("my.app.tier").map(String::as_str),
            Some("web")
        );
        assert!(snap.labels.contains_key("devcontainer.local_folder"));
        assert!(snap.labels.contains_key("com.docker.compose.project"));
        // Bind mount source reported as leaf only.
        assert_eq!(snap.mounts["/workspace"].source_tail, "ws");
    }

    #[test]
    fn diff_states_flags_env_and_normalizes_root_user() {
        let mut deacon = StateSnapshot::default();
        let mut upstream = StateSnapshot::default();
        deacon.user = String::new(); // image default
        upstream.user = "root".to_string();
        deacon.env.insert("A=1".to_string());
        upstream.env.insert("A=2".to_string());
        let divs = diff_states(&deacon, &upstream);
        // "" and "root" are equivalent → no user divergence.
        assert!(!divs.iter().any(|d| d.field == "user"));
        // Env value differs → flagged.
        assert!(divs.iter().any(|d| d.field == "env:A"));
    }

    // The following four cases preserve the pure-differ coverage that previously
    // lived in `crates/deacon/tests/integration_state_diff.rs` (deleted when its
    // sole dependency, `parity_utils.rs`, was removed). The classifier-branch
    // tests from that file are intentionally NOT ported — divergence
    // classification moves to the waiver system (US2), not this module.

    #[test]
    fn container_state_strips_compose_project_prefix_on_volume_source_tail() {
        let inspect = json!({
            "Config": {
                "Labels": { "com.docker.compose.project": "deacon_1_2" },
                "User": ""
            },
            "Mounts": [
                { "Type": "volume", "Name": "deacon_1_2_feat-probe-vol",
                  "Source": "/var/lib/docker/volumes/x/_data", "Destination": "/feat-mnt", "RW": true }
            ]
        });
        let snap = container_state("vol", &inspect).expect("snapshot");
        // The project prefix is stripped from the reporting source tail so it is
        // comparable to upstream's differently-prefixed volume name.
        assert_eq!(
            snap.mounts.get("/feat-mnt").map(|m| m.source_tail.as_str()),
            Some("feat-probe-vol")
        );
    }

    #[test]
    fn diff_states_detects_missing_mount_and_env_but_ignores_bind_source() {
        let deacon = container_state(
            "d",
            &json!({
                "Config": { "Env": ["FOO=bar"], "User": "" },
                "Mounts": [ { "Type": "bind", "Source": "/tmp/ws-a", "Destination": "/workspace", "RW": true } ]
            }),
        )
        .unwrap();
        let upstream = container_state(
            "u",
            &json!({
                "Config": { "Env": ["FOO=bar", "SECRET=1"], "User": "" },
                "Mounts": [
                    { "Type": "bind", "Source": "/tmp/ws-b", "Destination": "/workspace", "RW": true },
                    { "Type": "volume", "Name": "up_data", "Source": "/x", "Destination": "/data", "RW": true }
                ]
            }),
        )
        .unwrap();
        let divs = diff_states(&deacon, &upstream);
        // Missing mount and missing env are both flagged...
        assert!(divs.iter().any(|d| d.field == "mount:/data"));
        assert!(divs.iter().any(|d| d.field == "env:SECRET"));
        // ...but a differing bind SOURCE (per-workspace temp path) is NOT.
        assert!(!divs.iter().any(|d| d.field == "mount:/workspace"));
    }

    #[test]
    fn diff_states_captures_and_diffs_published_ports() {
        let with_port = container_state(
            "w",
            &json!({
                "Config": { "Env": [], "User": "" },
                "HostConfig": { "PortBindings": { "3000/tcp": [{ "HostIp": "", "HostPort": "3000" }] } }
            }),
        )
        .unwrap();
        let without_port = container_state(
            "wo",
            &json!({
                "Config": { "Env": [], "User": "" },
                "HostConfig": { "PortBindings": {} }
            }),
        )
        .unwrap();
        assert!(with_port.published_ports.contains("3000/tcp"));
        let divs = diff_states(&with_port, &without_port);
        assert!(divs.iter().any(|d| d.field == "pubport:3000/tcp"));
        // Identical published ports → no divergence.
        assert!(diff_states(&with_port, &with_port).is_empty());
    }

    #[test]
    fn diff_states_default_snapshot_has_no_self_divergence() {
        let s = StateSnapshot::default();
        assert!(diff_states(&s, &s).is_empty());
    }
}
