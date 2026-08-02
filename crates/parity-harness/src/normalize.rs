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
use crate::exec::Side;

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
/// SINGLE SOURCE OF TRUTH: re-exported from [`crate::provenance`] so the
/// snapshot provenance (conformance, the lower crate) and this normalizer never drift.
/// `"1"` was the T011 pass-through; `"2"` is the US3 named-rule normalizer. The runner
/// stamps it into the verdict report (`VerdictReport::new`) and the refresh bin records
/// it into `Provenance.normalizerVersion`; staleness compares the recorded value
/// against it (T032).
pub use crate::provenance::NORMALIZER_VERSION;

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

/// **Rule `user_default_root`, declarative half** (024 US5, action `canonicalize`, scope
/// `channel:chan-container-state` `field:/user` + `field:/userSpec/name`): an EMPTY
/// container user is spelled `root`.
///
/// Docker records "the image's default user" as the empty string, and both CLIs mean the
/// same thing by it — but they do not always write the same one. Measured at oracle 0.87.0
/// over a Feature-extended image: deacon leaves `Config.User` empty, the reference's
/// generated Dockerfile sets `USER root`, and the two containers run as the same user.
/// Comparing the raw spelling reports a difference with no observable consequence, which
/// RULES.md places out of scope entirely.
///
/// It CANONICALIZES —
/// nothing is removed, a genuinely non-root user still compares, and the rewrite is
/// confined to the exact value `""` on two named fields.
fn default_user_root(value: &Value) -> Value {
    let Value::Object(map) = value else {
        return value.clone();
    };
    let mut out = map.clone();
    if let Some(Value::String(user)) = map.get("user") {
        if user.is_empty() {
            out.insert("user".to_string(), Value::String("root".to_string()));
        }
    }
    if let Some(Value::Object(spec)) = map.get("userSpec") {
        let mut spec = spec.clone();
        if matches!(spec.get("name"), None | Some(Value::Null))
            && spec.get("uid") == Some(&Value::Null)
        {
            spec.insert("name".to_string(), Value::String("root".to_string()));
        }
        out.insert("userSpec".to_string(), Value::Object(spec));
    }
    Value::Object(out)
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
    side: Side,
) -> NormalizedChannelEvidence {
    debug_assert_eq!(
        channel, raw.channel,
        "normalize_channel: `channel` must match the evidence's channel"
    );
    NormalizedChannelEvidence {
        channel: raw.channel.clone(),
        operation: raw.operation.clone(),
        present: raw.present,
        value: apply_channel_rules(channel, &raw.value, tokens, side),
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
///
/// **Rule `image_tag_token`.** When the case built into a runner-assigned tag
/// (`--image-name ${IMAGE_TAG}`), that tag rewrites to `<IMAGE_TAG>`. The tag is unique per
/// case run AND per side — it has to be, or the second side's build would overwrite the
/// first — so without this every build's reported `imageName` would differ by construction
/// and no case could assert the tag it asked for. Rewritten, never dropped: an operation
/// that reported NO image name still differs from one that reported the tag.
pub fn tokens_for_channel(channel: &str, workspace: &Path, image_tag: Option<&str>) -> TokenMap {
    let mut m = if channel == crate::model::CHAN_CONTAINER_STATE {
        TokenMap::workspace_with_basename(workspace)
    } else {
        TokenMap::workspace(workspace)
    };
    if let Some(tag) = image_tag {
        m.insert(tag, "<IMAGE_TAG>");
        // The reported name is sometimes the bare repository, without the `:latest` the
        // runner appends — tokenize that too so both forms land on the same token.
        if let Some(repo) = tag.strip_suffix(":latest") {
            m.insert(repo, "<IMAGE_TAG>");
        }
    }
    m
}

/// Apply the named rules the contract lists for `channel` (observer-channel.md). An
/// unknown channel is identity (never blanket-removed).
fn apply_channel_rules(channel: &str, value: &Value, tokens: &TokenMap, side: Side) -> Value {
    use crate::model::{
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
        CHAN_STRUCTURED_OUTPUT => {
            config_document_rules(&path_token(value, tokens), side, DocumentBlock::Wrapper)
        }
        CHAN_FILE_CONTENT => null_preserving(&path_token(value, tokens)),
        CHAN_FILESYSTEM => path_token(value, tokens),
        CHAN_IMAGE => normalize_image(value, tokens),
        CHAN_PROCESS_GRAPH => normalize_process_graph(value, tokens),
        CHAN_INJECTED_PROCESS => normalize_injected_process(value, tokens),
        CHAN_TEMPORAL => null_preserving(value),
        // `chan-container-state`: `workspace_basename_token` (carried by the token map
        // from `tokens_for_channel`) + `path_token` over the whole snapshot — object KEYS
        // included, so mount destinations keyed by the container-side workspace path
        // normalize on both sides — then `container_hostname_token` (024 US5: the one
        // variable two containers can never agree on, REWRITTEN rather than deleted with
        // `PATH` and `HOME` alongside it) and `null_preserving`. NOTHING is removed:
        // env, labels, entrypoint, cmd and networks are emitted verbatim and any
        // characterized difference is covered by a scoped, backed `allowedDifference`.
        CHAN_CONTAINER_STATE => null_preserving(&default_user_root(&container_hostname_token(
            &path_token(value, tokens),
        ))),
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
pub fn config(case: &str, raw: &str, side: Side) -> Result<Value, HarnessError> {
    let v = parse(case, raw)?;
    let inner = match &v {
        Value::Object(o) => match o.get("configuration") {
            Some(c @ Value::Object(_)) => c.clone(),
            _ => v.clone(),
        },
        _ => v.clone(),
    };
    Ok(config_document_rules(
        &inner,
        side,
        DocumentBlock::Configuration,
    ))
}

/// Normalize the `mergedConfiguration` block (Tier 1b): the same named rule chain
/// applied to that block. A non-object top-level is a normalization failure.
pub fn merged_config(case: &str, raw: &str, side: Side) -> Result<Value, HarnessError> {
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
    Ok(config_document_rules(&block, side, DocumentBlock::Merged))
}

/// Which resolved-configuration block a normalization is being applied to.
///
/// The two blocks are NOT interchangeable for [`drop_absent_optional`] (024 US5, T123):
/// `configuration` is an echo of what the author wrote, so an empty value there is
/// authorship information; `mergedConfiguration` is synthesized by both CLIs, so an empty
/// value there is a computed default on both sides.
///
/// [`DocumentBlock::Wrapper`] is the whole CLI document, where each block is resolved by
/// its own key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DocumentBlock {
    /// The `configuration` block (or a document already unwrapped to it).
    Configuration,
    /// The `mergedConfiguration` block (or a document already unwrapped to it).
    Merged,
    /// The whole CLI document, carrying `configuration` and/or `mergedConfiguration`.
    Wrapper,
}

/// THE named rule chain for a resolved-configuration document — the SINGLE definition
/// shared by the legacy [`config`] / [`merged_config`] entry points and the declarative
/// `chan-structured-output` channel (constitution VIII: one normalizer, not two).
///
/// In order: [`devcontainer_id_token`] (rewrite), [`drop_absent_optional`] (drop, finite
/// enumerated key set, narrowed to deacon's `configuration` block), then
/// [`null_preserving`] — which is identity and exists to state, at the end of the chain,
/// that NOTHING else is removed.
pub fn config_document_rules(value: &Value, side: Side, block: DocumentBlock) -> Value {
    null_preserving(&drop_absent_optional(
        &devcontainer_id_token(value),
        side,
        block,
    ))
}

/// **Rule `drop_absent_optional`** (023 T062, action `drop`, scope
/// `field:/configuration` + `field:/mergedConfiguration`).
///
/// Removes a key **named in [`ABSENT_OPTIONAL_KEYS`]** when its value carries no
/// information (`null`, `[]`, `{}`, `""`). Nothing else, ever.
///
/// **Why it exists**: deacon serialized every modeled optional property of
/// `devcontainer.json` unconditionally, while the reference omits keys that were not
/// authored. The two documents therefore described the SAME resolved configuration in
/// different JSON shapes, and without this rule that one serializer difference produced
/// ~48 spurious divergences per corpus case and buried the real ones.
///
/// **Why it is not `prune`**: the removal set is a finite, enumerated list of key names
/// (FR-021). A property added to `DevContainerConfig` tomorrow is NOT on the list, so it
/// surfaces as a divergence rather than vanishing — which is the exact regression
/// `prune` made invisible. The value guard means a populated `appPort` is always
/// compared; only an absent one is elided.
///
/// # The `configuration` half is RETIRED (#398)
///
/// #398 made every modeled optional genuinely optional — `Option<_>` plus
/// `skip_serializing_if`, including the eleven properties that had been typed as bare
/// collections (`capAdd`, `containerEnv`, `customizations`, `features`, `forwardPorts`,
/// `mounts`, `portsAttributes`, `remoteEnv`, `runArgs`, `runServices`, `securityOpt`).
/// deacon now emits exactly the keys the author wrote, so on the `configuration` block
/// the compensation has nothing left to compensate. [`applies_to`] returns `false` there.
///
/// Retiring it was not merely tidying. Once deacon emits an authored `"capAdd": []` and
/// the reference emits the same, a deacon-side-only drop turns that AGREEMENT into a
/// reported divergence. A compensation kept past its defect does not go quiet; it starts
/// lying in the other direction.
///
/// # What the rule still does: `mergedConfiguration`, both sides
///
/// `mergedConfiguration` is SYNTHESIZED rather than echoed, and there the two CLIs still
/// disagree — but the other way around. For a configuration authoring only `image`, the
/// pinned reference emits computed empties deacon omits:
///
/// ```text
/// reference   remoteEnv {}   containerEnv {}   portsAttributes {}
///             hostRequirements { cpus: null, memory: "-Infinity", storage: "-Infinity" }
/// deacon      (all four omitted)
/// ```
///
/// That is a distinct deacon gap from the one this rule was written for, and it is
/// characterized in the registry rather than papered over here — see
/// `bhv-readconfig-merged-computed-empties-omitted`. Until it is closed the rule keeps
/// the merged comparison legible; every name in [`ABSENT_OPTIONAL_KEYS`] is load-bearing
/// on that block, which is why the list is not pruned to match the retired half.
///
/// # The FR-055 narrowing this preserves (024 US5, T123)
///
/// The rule used to run on BOTH sides of every differential, which made the three FR-055
/// states indistinguishable: an authored `"forwardPorts": null`, an authored
/// `"forwardPorts": []`, and an omitted `forwardPorts` all ended as "the key is absent on
/// both sides". It was, in effect, deleting the reference's answer to the question. On
/// `mergedConfiguration` that objection does not apply — an empty value there is a
/// computed default on either side, never an authorship signal.
pub fn drop_absent_optional(value: &Value, side: Side, block: DocumentBlock) -> Value {
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
                let field_block = match field {
                    "configuration" => DocumentBlock::Configuration,
                    _ => DocumentBlock::Merged,
                };
                if !applies_to(side, field_block) {
                    continue;
                }
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
    if !applies_to(side, block) {
        return value.clone();
    }
    drop_absent_optional_scoped(value, DropScope::Root)
}

/// Whether [`drop_absent_optional`] applies to `block` on `side` — the whole of the 024
/// US5 narrowing, in one place a reviewer can read.
fn applies_to(_side: Side, block: DocumentBlock) -> bool {
    match block {
        // RETIRED in #398. The rule compensated for deacon serializing every modeled
        // optional unconditionally; deacon now omits what the author did not write, so
        // there is nothing left here to elide — and leaving the rule on would MANUFACTURE
        // divergences: an authored `"capAdd": []` is now emitted by both CLIs, and a
        // deacon-side-only drop would report the agreement as a difference.
        DocumentBlock::Configuration => false,
        DocumentBlock::Merged => true,
        // A wrapper reaching here has neither block key, so there is nothing to elide.
        DocumentBlock::Wrapper => false,
    }
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
// Container observable-state normalization (observable-state parity)
//
// Ported verbatim (semantics-preserving) from the sole prior implementation in
// crates/deacon/tests/parity_utils.rs (L488–981): noise-env subtraction,
// intentional-label-prefix subtraction, compose project-prefix stripping, and
// user normalization. The KNOWN_* const classifier lists are intentionally NOT
// ported — divergence classification moves to the waiver system (US2).
// ===========================================================================

/// **Rule `container_hostname_token`** (024 US5, T123, action `rewrite`, scope
/// `channel:chan-container-state`): rewrite a container-id-shaped `HOSTNAME` value to
/// `<CONTAINER_HOSTNAME>`.
///
/// Docker defaults a container's hostname to its own 12-character short id, so two
/// containers created by two CLIs from the same configuration ALWAYS disagree here, for a
/// reason that says nothing about either CLI. The retired `drop_noise_env` rule handled
/// this by deleting the variable outright — along with `PATH` and `HOME`, which carry real
/// information.
///
/// Rewrite, never delete, and only when the value LOOKS like a container id: a hostname
/// the configuration actually set (`runArgs: ["--hostname", "dev"]`) is not 12 hex
/// characters, so it is left alone and compared. Applied to the `env` array entry and the
/// derived `envMap` value, which are two encodings of the same observation.
fn container_hostname_token(value: &Value) -> Value {
    fn is_container_id(v: &str) -> bool {
        v.len() == 12 && v.chars().all(|c| matches!(c, '0'..='9' | 'a'..='f'))
    }
    const TOKEN: &str = "<CONTAINER_HOSTNAME>";

    let Value::Object(map) = value else {
        return value.clone();
    };
    let mut out = map.clone();
    if let Some(Value::Array(items)) = map.get("env") {
        out.insert(
            "env".to_string(),
            Value::Array(
                items
                    .iter()
                    .map(
                        |item| match item.as_str().and_then(|s| s.strip_prefix("HOSTNAME=")) {
                            Some(v) if is_container_id(v) => {
                                Value::String(format!("HOSTNAME={TOKEN}"))
                            }
                            _ => item.clone(),
                        },
                    )
                    .collect(),
            ),
        );
    }
    if let Some(Value::Object(env_map)) = map.get("envMap") {
        let mut env_map = env_map.clone();
        if let Some(Value::String(v)) = env_map.get("HOSTNAME") {
            if is_container_id(v) {
                env_map.insert("HOSTNAME".to_string(), Value::String(TOKEN.to_string()));
            }
        }
        out.insert("envMap".to_string(), Value::Object(env_map));
    }
    Value::Object(out)
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
    /// The container process shape: the keep-alive/entrypoint wrapper each CLI installs.
    ///
    /// This was recorded as "a keep-alive detail with NO observable behavioral
    /// difference … intentional, characterized divergence (#290)". **That was wrong, and
    /// the claim is what hid the defect.** deacon ran a foreground
    /// `sleep infinity || tail -f /dev/null` as PID 1, which cannot service SIGTERM, so
    /// `docker stop` waited the full 10s grace period and then SIGKILLed the container:
    /// 10,258 ms versus the reference CLI's 215 ms, exit 137 versus exit 0. Measured the
    /// first time a declarative case actually COMPARED this field (024) — the retired
    /// state comparison captured it and skipped it, on the strength of the same
    /// assumption.
    /// deacon now uses the same `trap` + background + `wait` shape, and both paths stop in
    /// ~200 ms.
    ///
    /// The lesson generalizes: "captured but not compared, because it cannot matter" is a
    /// claim about behavior, and an uncompared field is exactly where such a claim never
    /// gets tested.
    ///
    /// EMITTED on `chan-container-state` and therefore COMPARED (024 Phase 4). The retired
    /// state comparison documented it as "captured but NOT diffed" — an undeclared
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

    // Env VERBATIM (024 US5, T123). `drop_noise_env` used to subtract five variables
    // HERE, at capture, which also stripped them from the declarative
    // `chan-container-state` evidence (this function is what that observer delegates to)
    // — including `PATH`, the field FR-050 exists to compare. The rule retired with the
    // legacy comparison it was written for; capture now removes nothing.
    let env = str_array(&raw["Config"]["Env"]).into_iter().collect();

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

/// **Rule `compose_project_prefix`** (registered in 024 US5, T123, action `rewrite`,
/// scope `channel:chan-container-state`): strip a Compose project's `<project>_` prefix
/// from a network or volume name so two CLIs that derive different project names still
/// compare on the resource.
///
/// It was applied here since the observable-state port and was NOT in the rule registry —
/// an unregistered rewrite is exactly what V24 exists to make impossible to miss, and its
/// effect (discarding the project identity with no trace in the evidence) was invisible.
/// It is registered now, and the derived `composeProjectResources` field records the
/// project name it stripped, so the rewrite is auditable from the snapshot alone.
fn strip_project_prefix(name: &str, project: &str) -> String {
    if !project.is_empty() {
        if let Some(rest) = name.strip_prefix(&format!("{project}_")) {
            return rest.to_string();
        }
    }
    name.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    use crate::model::{
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

    /// The observable paths on which two normalized configuration documents differ,
    /// reached through the SAME comparison the runner uses
    /// ([`crate::compare::verdict_differential`]) rather than a second differ written
    /// for the tests.
    ///
    /// The ranked `diff`/`summarize` pair this replaced classified each difference as
    /// `ref-only` / `deacon-only` / `value`. The declarative comparison names the
    /// diverging PATH instead and leaves the two sides' values in the preserved
    /// evidence, so the class is read off the evidence rather than stamped on the
    /// verdict. What must not be lost is that a one-sided difference is reported in
    /// BOTH directions — a comparison that treated the reference as the truth would
    /// drop deacon-only keys, which is the failure FR-020 names.
    fn diverging_paths(deacon: &Value, reference: &Value) -> Vec<String> {
        use crate::compare::{Tolerances, verdict_differential};
        use crate::evidence::{NormalizedChannelEvidence, Outcome};
        use crate::model::CHAN_STRUCTURED_OUTPUT;

        let side = |value: &Value| NormalizedChannelEvidence {
            channel: CHAN_STRUCTURED_OUTPUT.to_string(),
            operation: "op-read".to_string(),
            present: true,
            value: value.clone(),
        };
        let no_tolerances = Tolerances::new(&[], &[]);
        let mut consumed = std::collections::HashSet::new();
        let verdict = verdict_differential(
            CHAN_STRUCTURED_OUTPUT,
            &side(deacon),
            &side(reference),
            &no_tolerances,
            &mut consumed,
        );
        let prefix = format!("{CHAN_STRUCTURED_OUTPUT}.");
        match verdict.outcome {
            Outcome::Agree => Vec::new(),
            Outcome::Diverge => verdict
                .detail
                .as_ref()
                .and_then(|d| d.get("divergingPaths"))
                .and_then(Value::as_array)
                .expect("a differential divergence names its paths")
                .iter()
                .map(|p| {
                    p.as_str()
                        .expect("a diverging path is a string")
                        .strip_prefix(&prefix)
                        .unwrap_or_else(|| p.as_str().expect("a diverging path is a string"))
                        .to_string()
                })
                .collect(),
            other => panic!("unexpected outcome with no tolerances declared: {other:?}"),
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
        let out = normalize_channel(
            CHAN_STRUCTURED_OUTPUT,
            &not_captured,
            &TokenMap::new(),
            Side::Deacon,
        );
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
        let n = normalize_channel(CHAN_IMAGE, &img, &tokens, Side::Deacon);
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
        let g = normalize_channel(CHAN_PROCESS_GRAPH, &graph, &tokens, Side::Deacon);
        assert_eq!(g.value["mounts"].as_array().unwrap().len(), 1);
        assert!(g.value.get("networks").is_some() && g.value.get("volumes").is_some());
        // Injected process: env + command survive.
        let inj = raw(
            CHAN_INJECTED_PROCESS,
            json!({ "env": {"A":"1"}, "command": ["run"], "path": "/usr/bin:/bin" }),
        );
        let i = normalize_channel(CHAN_INJECTED_PROCESS, &inj, &tokens, Side::Deacon);
        assert!(i.value.get("env").is_some() && i.value.get("command").is_some());
    }

    #[test]
    fn image_tag_token_makes_two_sides_build_output_comparable() {
        // The two sides build into DIFFERENT tags by construction (a shared tag would have
        // the second build overwrite the first), so their reported image names can only be
        // compared through the token.
        let deacon = tokens_for_channel(
            CHAN_STRUCTURED_OUTPUT,
            Path::new("/tmp/deacon-conf-aaa"),
            Some("dcr-1-0-img:latest"),
        );
        let reference = tokens_for_channel(
            CHAN_STRUCTURED_OUTPUT,
            Path::new("/tmp/deacon-conf-bbb"),
            Some("dcr-1-1-img:latest"),
        );
        let out = |t: &TokenMap, tag: &str| {
            normalize_channel(
                CHAN_STRUCTURED_OUTPUT,
                &raw(CHAN_STRUCTURED_OUTPUT, json!({ "imageName": [tag] })),
                t,
                Side::Deacon,
            )
            .value
        };
        assert_eq!(
            out(&deacon, "dcr-1-0-img:latest"),
            out(&reference, "dcr-1-1-img:latest"),
            "per-side build tags compare equal once tokenized"
        );
        assert_eq!(
            out(&deacon, "dcr-1-0-img:latest")["imageName"][0],
            "<IMAGE_TAG>"
        );
        // The bare repository (no `:latest`) lands on the same token, so a CLI that reports
        // one form and a CLI that reports the other still compare equal.
        assert_eq!(out(&deacon, "dcr-1-0-img")["imageName"][0], "<IMAGE_TAG>");
        // Rewritten, never dropped: an operation that reported NO name still differs.
        assert_ne!(
            out(&deacon, "dcr-1-0-img:latest"),
            normalize_channel(
                CHAN_STRUCTURED_OUTPUT,
                &raw(CHAN_STRUCTURED_OUTPUT, json!({ "imageName": [] })),
                &deacon,
                Side::Deacon,
            )
            .value
        );
    }

    #[test]
    fn image_tag_token_is_absent_when_the_case_built_no_image() {
        // No tag → the map is exactly the workspace map, so no existing channel's
        // normalization changes meaning.
        let with_none = tokens_for_channel(CHAN_STDOUT, Path::new("/tmp/ws"), None);
        let plain = TokenMap::workspace(Path::new("/tmp/ws"));
        assert_eq!(
            with_none.apply("dcr-1-0-img:latest /tmp/ws"),
            plain.apply("dcr-1-0-img:latest /tmp/ws")
        );
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
            Side::Deacon,
        );
        assert_eq!(s.value["root"], json!("<WORKSPACE>"));
        assert_eq!(s.value["keep"], Value::Null, "null preserved");
        // exit-code: no rule (a number is untouched).
        let e = normalize_channel(
            CHAN_EXIT_CODE,
            &raw(CHAN_EXIT_CODE, json!(0)),
            &tokens,
            Side::Deacon,
        );
        assert_eq!(e.value, json!(0));
        // stdout: path_token on the string.
        let o = normalize_channel(
            CHAN_STDOUT,
            &raw(CHAN_STDOUT, json!("at /tmp/ws/x")),
            &tokens,
            Side::Deacon,
        );
        assert_eq!(o.value, json!("at <WORKSPACE>/x"));
    }

    #[test]
    fn normalizer_version_is_bumped_for_named_rules() {
        assert_eq!(
            NORMALIZER_VERSION, "7",
            "#398 retired `drop_absent_optional` on the `configuration` block entirely — \
             deacon now omits the properties the author did not write, so the \
             compensation had nothing left to compensate, and keeping it would report an \
             authored `\"capAdd\": []` that both CLIs emit as a divergence. A change to \
             what \"equal\" means, so every recorded snapshot must go stale and be \
             re-reviewed"
        );
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
        use crate::model::CHAN_CONTAINER_STATE;
        let state = |name: &str| {
            json!({
                "mounts": { format!("/workspaces/{name}"): { "mountType": "bind", "ro": false, "sourceTail": name } },
                "workspaceBindTargets": [format!("/workspaces/{name}")]
            })
        };
        let a = apply_channel_rules(
            CHAN_CONTAINER_STATE,
            &state("deacon-conf-aaa"),
            &tokens_for_channel(
                CHAN_CONTAINER_STATE,
                Path::new("/tmp/deacon-conf-aaa"),
                None,
            ),
            Side::Deacon,
        );
        let b = apply_channel_rules(
            CHAN_CONTAINER_STATE,
            &state("deacon-conf-bbb"),
            &tokens_for_channel(
                CHAN_CONTAINER_STATE,
                Path::new("/tmp/deacon-conf-bbb"),
                None,
            ),
            Side::Deacon,
        );
        assert_eq!(a, b, "two temp workspaces must normalize equal");
        assert_eq!(
            a["workspaceBindTargets"],
            json!(["/workspaces/<WORKSPACE_NAME>"]),
            "the basename is TOKENIZED, not removed: {a}"
        );

        // Only this channel gets the basename token — no other channel's meaning changes.
        let plain = tokens_for_channel(CHAN_STDOUT, Path::new("/tmp/deacon-conf-aaa"), None);
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
        // Through `mergedConfiguration`, the block the rule still applies to (#398).
        let wrapped = format!(r#"{{ "mergedConfiguration": {raw} }}"#);
        let normalized = merged_config("drop", &wrapped, Side::Deacon).expect("normalize");
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
        // Applied through the block the rule still runs on (#398 retired the
        // `configuration` half); the boundedness property is the same either way.
        let wrapped = format!(r#"{{ "mergedConfiguration": {raw} }}"#);
        let obj = merged_config("bounded", &wrapped, Side::Deacon).expect("normalize");

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
            config("w", wrapped, Side::Deacon).unwrap(),
            config("b", bare, Side::Deacon).unwrap(),
            "the reference's {{configuration}} wrapper must be unwrapped to match deacon's bare output"
        );
    }

    #[test]
    fn dynamic_id_token_rewrites_the_literal_everywhere() {
        // The literal `${devcontainerId}` is an EXACT string match, never a pattern, so
        // it is safe to rewrite anywhere.
        let raw = r#"{ "a": "id-${devcontainerId}-x", "name": "n-${devcontainerId}" }"#;
        let n = config("dyn", raw, Side::Deacon).unwrap();
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
        let n = config("dyn", raw, Side::Deacon).unwrap();
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
        let a = config(
            "a",
            r#"{ "customizations": { "d": "0123456789ab" } }"#,
            Side::Deacon,
        )
        .unwrap();
        let b = config(
            "b",
            r#"{ "customizations": { "d": "ffffffffffff" } }"#,
            Side::Deacon,
        )
        .unwrap();
        assert_eq!(
            diverging_paths(&a, &b),
            vec!["customizations.d".to_string()],
            "distinct digests must not collapse"
        );
    }

    #[test]
    fn normalization_failure_on_invalid_json() {
        let err = config("bad", "{ not json", Side::Deacon).expect_err("must fail");
        assert!(matches!(err, HarnessError::Normalization { .. }));
        // merged_config on a non-object also fails, not falls back.
        assert!(matches!(
            merged_config("arr", "[1,2,3]", Side::Deacon),
            Err(HarnessError::Normalization { .. })
        ));
    }

    #[test]
    fn merged_config_extracts_block() {
        // `empty` is NOT on the enumerated list, so it survives — only listed keys with
        // an absent value are elided (023 T062).
        let raw = r#"{ "configuration": {"name":"x"}, "mergedConfiguration": { "onCreateCommand": "echo hi", "empty": {}, "image": null } }"#;
        let n = merged_config("m", raw, Side::Deacon).unwrap();
        assert_eq!(n, json!({ "onCreateCommand": "echo hi", "empty": {} }));
    }

    #[test]
    fn every_one_sided_difference_is_reported_in_both_directions() {
        // FR-020: a key only the reference emits and a key only deacon emits are BOTH
        // findings. A comparison that treated the reference as the truth would report
        // the first and drop the second, and a deacon-only key is either a genuine
        // extension or a genuine over-emission — never noise.
        let deacon = json!({ "name": "x", "extra": 1 });
        let reference = json!({ "name": "y", "dropped": 2 });
        assert_eq!(
            diverging_paths(&deacon, &reference),
            vec![
                "dropped".to_string(), // reference-only
                "extra".to_string(),   // deacon-only
                "name".to_string(),    // same key, differing value
            ],
            "all three differences are reported, ordered deterministically by path"
        );
        // Deterministic regardless of which side is passed first.
        let mut swapped = diverging_paths(&reference, &deacon);
        swapped.sort();
        assert_eq!(
            swapped,
            vec![
                "dropped".to_string(),
                "extra".to_string(),
                "name".to_string()
            ]
        );
    }

    #[test]
    fn an_absent_enumerated_optional_no_longer_diverges_on_the_merged_block() {
        // `image` IS on the enumerated list, and `mergedConfiguration` is synthesized by
        // both CLIs: absent on one side, omitted on the other → the same computed
        // configuration, so no divergence.
        let a = merged_config(
            "a",
            r#"{ "mergedConfiguration": { "name": "x", "image": null } }"#,
            Side::Deacon,
        )
        .unwrap();
        let b = merged_config(
            "b",
            r#"{ "mergedConfiguration": { "name": "x" } }"#,
            Side::Deacon,
        )
        .unwrap();
        assert!(diverging_paths(&a, &b).is_empty());
    }

    #[test]
    fn an_absent_enumerated_optional_still_diverges_on_the_configuration_block() {
        // #398: the `configuration` block is an ECHO, so an authored `"image": null` is
        // the author's and must be compared against the side that wrote no `image` at
        // all. Eliding it is what made those two documents indistinguishable (FR-055).
        let a = config("a", r#"{ "name": "x", "image": null }"#, Side::Deacon).unwrap();
        let b = config("b", r#"{ "name": "x" }"#, Side::Deacon).unwrap();
        assert_eq!(diverging_paths(&a, &b), vec!["image".to_string()]);
    }

    #[test]
    fn an_absent_key_not_on_the_enumerated_list_still_diverges() {
        // The whole safety property of retiring `prune` (023 T062): an unlisted key is
        // COMPARED even when empty, so a newly added property cannot vanish silently.
        let a = config(
            "a",
            r#"{ "name": "x", "someNewProperty": null }"#,
            Side::Deacon,
        )
        .unwrap();
        let b = config("b", r#"{ "name": "x" }"#, Side::Deacon).unwrap();
        assert_eq!(
            diverging_paths(&a, &b),
            vec!["someNewProperty".to_string()],
            "an unlisted key deacon emits and the reference does not is a finding"
        );
    }

    #[test]
    fn a_populated_enumerated_optional_is_always_compared() {
        // The value guard: the rule elides an ABSENT `appPort`, never a populated one.
        let a = config("a", r#"{ "appPort": [3000] }"#, Side::Deacon).unwrap();
        let b = config("b", r#"{ }"#, Side::Deacon).unwrap();
        assert_eq!(diverging_paths(&a, &b), vec!["appPort".to_string()]);
    }

    #[test]
    fn config_file_path_is_no_longer_dropped() {
        // `prune` removed `configFilePath` unconditionally; it is now a compared value,
        // so the reference emitting it and deacon not is REPORTED (research D3).
        let deacon = config("a", r#"{ "name": "x" }"#, Side::Deacon).unwrap();
        let reference = config(
            "b",
            r#"{ "name": "x", "configFilePath": "/w/.devcontainer/devcontainer.json" }"#,
            Side::Deacon,
        )
        .unwrap();
        assert_eq!(
            diverging_paths(&deacon, &reference),
            vec!["configFilePath".to_string()]
        );
    }

    #[test]
    fn container_state_missing_config_is_normalization_error() {
        let err = container_state("nostate", &json!({ "Mounts": [] })).expect_err("must fail");
        assert!(matches!(err, HarnessError::Normalization { .. }));
    }

    #[test]
    fn container_state_captures_every_env_var_verbatim() {
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
        // 024 US5 (T123): CAPTURE removes NOTHING. `drop_noise_env` used to subtract five
        // variables here, which also removed them from the declarative
        // `chan-container-state` evidence — `PATH` among them, the field FR-050 exists to
        // compare.
        assert!(snap.env.contains("FOO=bar"));
        assert!(snap.env.contains("PATH=/bin"));
        assert!(snap.env.contains("HOME=/root"));
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

    /// The `container_hostname_token` rewrite (024 US5, T123): a container-id-shaped
    /// hostname is TOKENIZED, a configured one is compared.
    #[test]
    fn container_hostname_is_tokenized_only_when_it_looks_like_a_container_id() {
        let state = |host: &str| {
            json!({
                "env": [format!("HOSTNAME={host}"), "FOO=bar".to_string()],
                "envMap": { "HOSTNAME": host, "FOO": "bar" },
            })
        };
        let a = container_hostname_token(&state("0123456789ab"));
        let b = container_hostname_token(&state("fedcba987654"));
        assert_eq!(
            a, b,
            "two container short ids compare equal after the rewrite"
        );
        assert_eq!(a["envMap"]["HOSTNAME"], json!("<CONTAINER_HOSTNAME>"));
        assert_eq!(
            a["env"],
            json!(["HOSTNAME=<CONTAINER_HOSTNAME>", "FOO=bar"]),
            "the array encoding is rewritten too, and no entry is removed"
        );

        let configured = container_hostname_token(&state("my-dev-box"));
        assert_eq!(
            configured["envMap"]["HOSTNAME"],
            json!("my-dev-box"),
            "a hostname the configuration set is not 12 hex characters and is compared"
        );
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

    /// Capture keeps the whole of both sides. The retired state comparison that once read
    /// this snapshot deliberately did NOT compare bind mount SOURCES (a
    /// per-workspace temp path differs by construction); the declarative
    /// `chan-container-state` channel captures the source's leaf and lets a case declare
    /// a tolerance for it, so the elision is visible and stale-checked rather than
    /// hard-coded into the comparison.
    #[test]
    fn container_state_captures_mounts_env_and_the_bind_source_leaf() {
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
        // The mount and the env var present on only one side are both captured, so a
        // comparison can see them.
        assert!(!deacon.mounts.contains_key("/data"));
        assert!(upstream.mounts.contains_key("/data"));
        assert!(!deacon.env.contains("SECRET=1"));
        assert!(upstream.env.contains("SECRET=1"));
        // The bind source is captured as its LEAF, so the two per-workspace temp paths
        // are comparable without being identical.
        assert_eq!(deacon.mounts["/workspace"].source_tail, "ws-a");
        assert_eq!(upstream.mounts["/workspace"].source_tail, "ws-b");
    }

    #[test]
    fn container_state_captures_published_ports() {
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
        assert!(without_port.published_ports.is_empty());
    }
}
