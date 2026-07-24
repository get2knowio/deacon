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
//! - `sanitize_dynamic_values` and the config `prune` helper live ONLY here (the
//!   unrelated `core::port_forward::registry::prune`, which reaps dead daemon
//!   records, is not normalization);
//! - the three Python corpus runners that carried duplicate `prune` copies were
//!   deleted in T030 — no `fixtures/**` script normalizes output.
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

/// Keys the reference adds that carry no cross-CLI meaning (pure noise).
const DROP_KEYS: &[&str] = &["configFilePath"];

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

/// The `<WORKSPACE>` / `<PROJECT>` path token substitution context for `path_token`
/// (FR-024). Each `(path, token)` pair rewrites occurrences of an absolute temp path to
/// a stable token so evidence is portable across machines/recordings. Substitutions are
/// applied longest-path-first so a nested path tokenizes before its parent.
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

/// Apply the named rules the contract lists for `channel` (observer-channel.md). An
/// unknown channel is identity (never blanket-removed).
fn apply_channel_rules(channel: &str, value: &Value, tokens: &TokenMap) -> Value {
    use deacon_conformance::model::{
        CHAN_EXIT_CODE, CHAN_FILE_CONTENT, CHAN_FILESYSTEM, CHAN_IMAGE, CHAN_INJECTED_PROCESS,
        CHAN_PROCESS_GRAPH, CHAN_STDERR, CHAN_STDOUT, CHAN_STRUCTURED_OUTPUT, CHAN_TEMPORAL,
    };
    match channel {
        // No rule: an exit code carries no path/label/PATH content.
        CHAN_EXIT_CODE => value.clone(),
        CHAN_STDOUT | CHAN_STDERR => path_token(value, tokens),
        CHAN_STRUCTURED_OUTPUT | CHAN_FILE_CONTENT => null_preserving(&path_token(value, tokens)),
        CHAN_FILESYSTEM => path_token(value, tokens),
        CHAN_IMAGE => normalize_image(value, tokens),
        CHAN_PROCESS_GRAPH => normalize_process_graph(value, tokens),
        CHAN_INJECTED_PROCESS => normalize_injected_process(value, tokens),
        CHAN_TEMPORAL => null_preserving(value),
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
/// `{configuration}` wrapper, prune noise, sanitize dynamic ids.
pub fn config(case: &str, raw: &str) -> Result<Value, HarnessError> {
    let v = parse(case, raw)?;
    let inner = match &v {
        Value::Object(o) => match o.get("configuration") {
            Some(c @ Value::Object(_)) => c.clone(),
            _ => v.clone(),
        },
        _ => v.clone(),
    };
    let mut pruned = prune(&inner);
    sanitize_dynamic_values(&mut pruned);
    Ok(pruned)
}

/// Normalize the `mergedConfiguration` block (Tier 1b): the same prune + sanitize
/// rules applied to that block. A non-object top-level is a normalization failure.
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
    let mut pruned = prune(&block);
    sanitize_dynamic_values(&mut pruned);
    Ok(pruned)
}

fn parse(case: &str, raw: &str) -> Result<Value, HarnessError> {
    serde_json::from_str(raw.trim()).map_err(|e| HarnessError::Normalization {
        case: case.to_string(),
        cause: format!("output is not valid JSON: {e}"),
    })
}

/// Recursively drop nulls, empty arrays/objects/strings, and [`DROP_KEYS`] — but
/// only when they are object *values*; list elements are preserved verbatim
/// (mirroring the ported Python `prune`).
fn prune(v: &Value) -> Value {
    match v {
        Value::Object(map) => {
            let mut out = Map::new();
            for (k, val) in map {
                if DROP_KEYS.contains(&k.as_str()) {
                    continue;
                }
                let pv = prune(val);
                if pv.is_null() {
                    continue;
                }
                let empty = match &pv {
                    Value::Object(o) => o.is_empty(),
                    Value::Array(a) => a.is_empty(),
                    Value::String(s) => s.is_empty(),
                    _ => false,
                };
                if empty {
                    continue;
                }
                out.insert(k.clone(), pv);
            }
            Value::Object(out)
        }
        Value::Array(arr) => Value::Array(arr.iter().map(prune).collect()),
        other => other.clone(),
    }
}

/// Recursively sanitize dynamic ids so outputs are comparable: `${devcontainerId}`
/// and any 12-char lowercase-hex run become `<ID>`. Applied identically to both
/// CLIs' output, so a real divergence still surfaces.
fn sanitize_dynamic_values(v: &mut Value) {
    match v {
        Value::Object(map) => {
            for val in map.values_mut() {
                sanitize_dynamic_values(val);
            }
        }
        Value::Array(arr) => {
            for val in arr.iter_mut() {
                sanitize_dynamic_values(val);
            }
        }
        Value::String(s) => {
            let replaced = replace_hex12(&s.replace("${devcontainerId}", "<ID>"));
            *s = replaced;
        }
        _ => {}
    }
}

/// Replace each 12-char contiguous lowercase-hex run with `<ID>` (char-safe).
fn replace_hex12(input: &str) -> String {
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

/// Divergence class, ranked most-significant first: a `ref-only` key means deacon
/// dropped data the reference kept (highest signal); `deacon-only` is usually
/// default noise (lowest).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiffKind {
    RefOnly,
    Value,
    DeaconOnly,
}

impl DiffKind {
    fn rank(self) -> u8 {
        match self {
            DiffKind::RefOnly => 0,
            DiffKind::Value => 1,
            DiffKind::DeaconOnly => 2,
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

/// Diff two normalized configs, ranked ref-only → value → deacon-only.
pub fn diff(deacon: &Value, reference: &Value) -> Vec<ConfigDivergence> {
    let mut out = Vec::new();
    diff_rec(deacon, reference, "", &mut out);
    out.sort_by_key(|d| d.kind.rank());
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
                "deacon-only {loc} = {}",
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

/// Label namespaces both CLIs stamp by design and differently (identity, per-CLI
/// metadata blob, compose bookkeeping, Docker Desktop). Subtracted before diffing
/// labels so only semantic image/config labels remain.
pub const INTENTIONAL_LABEL_PREFIXES: &[&str] = &[
    "devcontainer.",
    "com.docker.",
    "desktop.",
    "dev.containers.",
];

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

/// **NAMED, SCOPED legacy rule `strip_intentional_labels` — chan-container-state ONLY**
/// (research D6, FR-029). Whether `key` is a label both CLIs stamp by design and
/// differently ([`INTENTIONAL_LABEL_PREFIXES`]). The ONLY sanctioned label subtraction,
/// scoped to the legacy observable-state channel. The NEW `chan-image` normalization
/// ([`label_semantic`]) NEVER blanket-removes a label — it parses labels to key/value
/// and preserves them, deferring intentional differences to scoped allowed-differences.
pub fn is_intentional_label(key: &str) -> bool {
    INTENTIONAL_LABEL_PREFIXES
        .iter()
        .any(|p| key.starts_with(p))
}

/// Normalized single-mount state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MountState {
    pub mount_type: String,
    pub ro: bool,
    /// Normalized source descriptor for REPORTING only (bind: leaf component;
    /// volume: name with compose-project prefix stripped). NOT compared.
    pub source_tail: String,
}

/// Normalized snapshot of a container's observable state.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct StateSnapshot {
    /// destination -> mount state
    pub mounts: BTreeMap<String, MountState>,
    /// `KEY=VALUE` entries, noise keys removed
    pub env: BTreeSet<String>,
    /// labels with CLI-namespaced keys stripped
    pub labels: BTreeMap<String, String>,
    pub user: String,
    pub working_dir: String,
    /// `Config.ExposedPorts` keys (image `EXPOSE` + declared), e.g. `3000/tcp`.
    pub exposed_ports: BTreeSet<String>,
    /// `HostConfig.PortBindings` keys actually PUBLISHED to the host.
    pub published_ports: BTreeSet<String>,
    /// Captured for debugging; NOT diffed. The container process shape is a
    /// deacon-internal keep-alive/entrypoint-wrapper detail with no observable
    /// behavioral difference — both CLIs keep the container running so `exec`,
    /// lifecycle hooks, and feature entrypoints work identically. deacon uses a
    /// PATH-robust `sh -c '… sleep infinity || tail -f /dev/null'`; the reference
    /// an `exec "$@"` keep-alive loop. Intentional, characterized divergence (#290);
    /// the behaviorally-significant cases (overrideCommand exit #291, feature
    /// entrypoint composition #292) ARE observable and covered elsewhere.
    pub entrypoint: Vec<String>,
    /// Captured for debugging; NOT diffed — see `entrypoint` (#290).
    pub cmd: Vec<String>,
    /// Captured (compose-project-prefix-normalized) for debugging; NOT diffed.
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

    // Legacy chan-container-state label subtraction via the NAMED, SCOPED rule
    // `strip_intentional_labels` ([`is_intentional_label`]) — the only sanctioned label
    // removal (D6).
    let labels = raw["Config"]["Labels"]
        .as_object()
        .map(|o| {
            o.iter()
                .filter(|(k, _)| !is_intentional_label(k))
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
        assert_eq!(NORMALIZER_VERSION, "2", "US3 named-rule normalizer");
    }

    #[test]
    fn legacy_noise_rules_are_named_and_scoped() {
        assert!(is_noise_env_key("PATH") && !is_noise_env_key("MY_VAR"));
        assert!(
            is_intentional_label("com.docker.compose.project")
                && !is_intentional_label("org.opencontainers.image.title")
        );
    }

    #[test]
    fn prune_drops_nulls_empties_and_configfilepath() {
        let raw = r#"{
            "configFilePath": "/x/.devcontainer/devcontainer.json",
            "name": "demo",
            "empty_str": "",
            "empty_obj": {},
            "empty_arr": [],
            "null_val": null,
            "nested": { "keep": 1, "drop": null },
            "list_keeps_nulls": [1, null, ""]
        }"#;
        let normalized = config("prune", raw).expect("normalize");
        let obj = normalized.as_object().expect("object");
        assert_eq!(obj.get("name"), Some(&json!("demo")));
        assert!(!obj.contains_key("configFilePath"));
        assert!(!obj.contains_key("empty_str"));
        assert!(!obj.contains_key("empty_obj"));
        assert!(!obj.contains_key("empty_arr"));
        assert!(!obj.contains_key("null_val"));
        assert_eq!(obj.get("nested"), Some(&json!({ "keep": 1 })));
        // List elements are preserved verbatim, including nulls/empties.
        assert_eq!(obj.get("list_keeps_nulls"), Some(&json!([1, null, ""])));
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
    fn dynamic_id_sanitization() {
        let raw = r#"{ "a": "id-${devcontainerId}-x", "b": "vol_0123456789ab_tail" }"#;
        let n = config("dyn", raw).unwrap();
        assert_eq!(n["a"], json!("id-<ID>-x"));
        assert_eq!(n["b"], json!("vol_<ID>_tail"));
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
        let raw = r#"{ "configuration": {"name":"x"}, "mergedConfiguration": { "onCreateCommand": "echo hi", "empty": {} } }"#;
        let n = merged_config("m", raw).unwrap();
        assert_eq!(n, json!({ "onCreateCommand": "echo hi" }));
    }

    #[test]
    fn diff_ranks_ref_only_first() {
        let deacon = json!({ "name": "x", "extra": 1 });
        let reference = json!({ "name": "y", "dropped": 2 });
        let divs = diff(&deacon, &reference);
        // ref-only (dropped) ranks before value (name) before deacon-only (extra).
        let kinds: Vec<_> = divs.iter().map(|d| d.kind).collect();
        assert_eq!(
            kinds,
            vec![DiffKind::RefOnly, DiffKind::Value, DiffKind::DeaconOnly]
        );
        assert_eq!(divs[0].path, "dropped");
        let summary = summarize(&divs);
        assert!(summary.contains("ref-only"));
        assert!(summary.contains("deacon drops this"));
    }

    #[test]
    fn diff_identical_after_prune_is_empty() {
        let a = config("a", r#"{ "name": "x", "n": null }"#).unwrap();
        let b = config("b", r#"{ "name": "x" }"#).unwrap();
        assert!(diff(&a, &b).is_empty());
    }

    #[test]
    fn container_state_missing_config_is_normalization_error() {
        let err = container_state("nostate", &json!({ "Mounts": [] })).expect_err("must fail");
        assert!(matches!(err, HarnessError::Normalization { .. }));
    }

    #[test]
    fn container_state_subtracts_noise_and_label_prefixes() {
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
        // CLI-namespaced labels stripped; app label kept.
        assert_eq!(
            snap.labels.get("my.app.tier").map(String::as_str),
            Some("web")
        );
        assert!(!snap.labels.contains_key("devcontainer.local_folder"));
        assert!(!snap.labels.contains_key("com.docker.compose.project"));
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
