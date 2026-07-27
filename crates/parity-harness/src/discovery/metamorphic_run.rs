//! Deacon-only metamorphic relation evaluation
//! (025-exploratory-parity-discovery, US6, T095/T127).
//!
//! This is the only discovery tier that needs **neither** the oracle **nor** Docker **nor**
//! the network (research D12), which makes it the cheapest complete vertical slice through
//! generation → comparison → signature → candidate. It also catches what the differential
//! structurally cannot: if deacon and the reference are *consistently* wrong, the
//! differential is clean and the defect is invisible, whereas a sensitivity relation
//! asserts the result **must** change and so fails on consistent wrongness.
//!
//! ## What is data and what is code
//!
//! The **catalogue** is registry data — `conformance/registry/metamorphic.json`, validated
//! by V31/V32. The **transformations** are code, because "reindent this document" is not
//! expressible as data without inventing a rewriting language, and a rewriting language is
//! a second thing to get wrong. [`TRANSFORMATIONS`] is therefore a closed table keyed by
//! relation id, and [`evaluate_catalogue`] fails loudly with
//! [`HarnessError::RelationUnevaluable`] for a declared relation the table does not cover.
//!
//! That failure mode is the whole point. A relation the harness cannot apply reports
//! nothing, and *reporting nothing is byte-identical to holding*. SC-011 requires zero
//! inert relations, so an unimplemented relation must turn the run red naming itself,
//! never contribute a silent pass.
//!
//! ## The evidence document
//!
//! Each side is observed as one document:
//!
//! ```json
//! { "exitCode": 0, "structuredOutput": { "configuration": { … }, "workspace": { … } } }
//! ```
//!
//! Both declared channel families are then paths within it — `chan-exit-code` at
//! `exitCode`, `chan-structured-output` under `structuredOutput` — so one comparison
//! covers both, and a run that *failed* is still comparable (its `structuredOutput` is
//! `null`, which is a value, not an absence of evidence). Normalization is
//! [`crate::normalize`] and nothing else: FR-015 permits exactly one normalization
//! definition, and a second one here could disagree with the one every other comparison
//! uses.
//!
//! ## Both sides are deacon
//!
//! [`crate::normalize::ConfigDivergence`]'s two sides are named `deacon` and `reference`
//! because it was built for the differential. Here both sides *are* deacon, so the mapping is fixed once
//! and stated: **`reference` is the original run, `deacon` is the transformed run**. That
//! makes the derived [`Signature`]'s `kind` read the way it should — `deacon-only` means
//! the transformation *introduced* something, `ref-only` means it *lost* something.
//!
//! ## The accounting, and why a residual is the interesting output
//!
//! An invariance relation holds when the two normalized documents are equal *after* the
//! relation's declared [`Accounting`] absorbs the differences it legitimately explains —
//! the workspace-path tokenization for relocation (FR-046), or the exact site the
//! transformation rewrote. Everything the accounting does not absorb is a **residual**,
//! and a residual is reported rather than tolerated.
//!
//! For `mrl-path-relocation` that residual is simultaneously a check on deacon and a check
//! on the normalizer: an absolute path the tokenizer missed surfaces here. Triage it
//! carefully — it is as likely to be a `normalizer-defect` as a `deacon-regression`, and
//! misfiling it as the latter sends someone to fix code that is correct.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use deacon_conformance::discovery::metamorphic::{MetamorphicRelation, RelationEffect};
use deacon_conformance::discovery::signature::{Divergence, DivergenceKind, Signature};

use crate::HarnessError;
use crate::exec::{ExecKind, Invocation, Side, run_and_capture};
use crate::normalize::{self, DiffKind, DocumentBlock, TokenMap};

/// Where the configuration document lives inside a [`Fixture`].
///
/// One of the two spec discovery locations. A plain `devcontainer.json` at the workspace
/// root is NOT one, and a fixture that put it there would fail to resolve for a reason
/// that has nothing to do with the relation under test.
pub const CONFIG_PATH: &str = ".devcontainer/devcontainer.json";

/// The workspace directory name every materialized fixture uses.
///
/// Fixed, and deliberately shared by both sides of a relocation: deacon derives
/// container-side paths (and `${localWorkspaceFolderBasename}`) from the workspace
/// directory's basename, so two differently-*named* directories would differ for a reason
/// that is not relocation. Holding the basename constant makes the relocation a pure
/// change of absolute path, which is what the relation asserts about.
pub const WORKSPACE_DIR_NAME: &str = "metamorphic-workspace";

// ---------------------------------------------------------------------------
// Fixture
// ---------------------------------------------------------------------------

/// A workspace tree, held as authored **text** rather than parsed values.
///
/// Text, because three of the seven relations transform things a parsed value cannot
/// represent — indentation, comments, and member order are all lost the moment a document
/// becomes a `serde_json::Value`. A fixture that round-tripped through a value could not
/// express the transformation whose invariance it is meant to test.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Fixture {
    /// Workspace-relative path → file contents, in sorted order for deterministic
    /// materialization and byte-stable candidate rendering.
    files: BTreeMap<String, String>,
}

impl Fixture {
    /// An empty fixture.
    pub fn new() -> Fixture {
        Fixture::default()
    }

    /// A fixture whose only file is the configuration document.
    pub fn with_config(config: &str) -> Fixture {
        Fixture::new().with_file(CONFIG_PATH, config)
    }

    /// Add (or replace) one file.
    pub fn with_file(mut self, rel: &str, contents: &str) -> Fixture {
        self.files.insert(rel.to_string(), contents.to_string());
        self
    }

    /// Remove one file, if present.
    pub fn without_file(mut self, rel: &str) -> Fixture {
        self.files.remove(rel);
        self
    }

    /// The files, workspace-relative path → contents.
    pub fn files(&self) -> &BTreeMap<String, String> {
        &self.files
    }

    /// The authored configuration document.
    ///
    /// A fixture with no configuration at [`CONFIG_PATH`] is a fixture defect, not a
    /// relation failure, so it is an error rather than an empty string.
    pub fn config(&self, relation: &str) -> Result<&str, HarnessError> {
        self.files
            .get(CONFIG_PATH)
            .map(String::as_str)
            .ok_or_else(|| HarnessError::RelationUnevaluable {
                relation: relation.to_string(),
                cause: format!("the base fixture has no `{CONFIG_PATH}`"),
            })
    }

    /// Write the fixture into `dir`, removing anything already there.
    ///
    /// The wipe matters: a non-relocating relation materializes both sides into the SAME
    /// directory, and `mrl-extends-flattening`'s transformed side *removes* a file. Writing
    /// over the previous tree without clearing it would leave the parent document behind,
    /// and the relation would compare a chain against a chain.
    pub fn materialize(&self, dir: &Path) -> Result<(), HarnessError> {
        if dir.exists() {
            std::fs::remove_dir_all(dir).map_err(|e| HarnessError::Report {
                cause: format!("could not clear workspace {dir:?}: {e}"),
            })?;
        }
        for (rel, contents) in &self.files {
            let path = dir.join(rel);
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent).map_err(|e| HarnessError::Report {
                    cause: format!("could not create {parent:?}: {e}"),
                })?;
            }
            std::fs::write(&path, contents).map_err(|e| HarnessError::Report {
                cause: format!("could not write {path:?}: {e}"),
            })?;
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// The closed transformation table
// ---------------------------------------------------------------------------

/// How a relation reconciles the two normalized documents before deciding whether it holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Accounting {
    /// Nothing is explained away: the documents must be equal outright.
    Identity,
    /// The workspace moved, so compare modulo the declared `<WORKSPACE>` tokenization and
    /// report whatever the tokenization does not account for (FR-046).
    PathTokenization,
    /// The transformation rewrote exactly these observable paths (and their descendants).
    /// A difference confined to them is the transformation itself; anything else is
    /// residual.
    ///
    /// This is a **scoped** accounting, not an ignore list: the paths are named, finite,
    /// and specific to one relation, in the same spirit as the scoped allowed-differences
    /// of 022 (a bare channel would be a global ignore and is exactly what FR-032 forbids
    /// there).
    TransformedSites(&'static [&'static str]),
}

/// What a transformation does to the fixture.
pub enum TransformKind {
    /// Rewrite the authored configuration text.
    RewriteConfigText(fn(&str) -> Result<String, HarnessError>),
    /// Rewrite the whole fixture (adding or removing files).
    RewriteFixture(fn(&Fixture) -> Result<Fixture, HarnessError>),
    /// Leave the fixture alone and materialize it at a different absolute path.
    Relocate,
}

impl std::fmt::Debug for TransformKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TransformKind::RewriteConfigText(_) => f.write_str("RewriteConfigText"),
            TransformKind::RewriteFixture(_) => f.write_str("RewriteFixture"),
            TransformKind::Relocate => f.write_str("Relocate"),
        }
    }
}

/// Which part of deacon's output a relation is asserted over.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ObservedScope {
    /// The whole CLI document.
    WholeDocument,
    /// One named top-level block of it (e.g. `mergedConfiguration`).
    ///
    /// `mrl-extends-flattening` needs this: the `configuration` block is an **echo** of the
    /// authored document, and a chain and its flattening are authored differently by
    /// construction. The claim the relation makes is about the *resolved* configuration, so
    /// that is the block it compares.
    Block(&'static str),
}

/// Where an invariance relation's deliberate break is applied (see [`Sabotage`]).
///
/// The break must land on **exactly one** side and must survive to the output, and which
/// order achieves that depends on the transformation — so it is declared per relation
/// rather than guessed. Guessing would mean a silent fallback in the one place a silent
/// fallback is least affordable: a break that quietly failed to land would leave the
/// relation holding, and a relation that holds under its own break reads as a healthy
/// relation rather than an inert one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BreakPoint {
    /// Perturb the base first, then transform it.
    ///
    /// Required when the transformation *emits* something the perturbation could not then
    /// be applied to — `mrl-comment-invariance` produces JSONC, and re-parsing that as
    /// strict JSON to insert a change would fail.
    BeforeTransform,
    /// Transform first, then perturb the result.
    ///
    /// The default, and required when the transformation does not carry its input's
    /// content through — `mrl-extends-flattening` emits the hand-written flattening, so a
    /// perturbation applied to its input would simply be discarded and the two sides would
    /// still agree.
    AfterTransform,
}

/// One relation's executable half.
pub struct Transformation {
    /// The `mrl-` id this implements.
    pub relation: &'static str,
    /// The base workspace the relation is evaluated over.
    pub base: fn() -> Fixture,
    /// Extra argv appended after `read-configuration --workspace-folder <ws>`.
    pub argv: &'static [&'static str],
    /// Which part of the output is compared.
    pub scope: ObservedScope,
    /// What is applied to the input.
    pub kind: TransformKind,
    /// How the two results are reconciled.
    pub accounting: Accounting,
    /// Where an invariance break is applied.
    pub break_point: BreakPoint,
}

impl Transformation {
    /// Whether this transformation materializes its two sides at different absolute paths.
    pub fn relocates(&self) -> bool {
        matches!(self.kind, TransformKind::Relocate)
    }

    /// Apply the transformation to `base`.
    pub fn apply(&self, base: &Fixture) -> Result<Fixture, HarnessError> {
        match &self.kind {
            TransformKind::RewriteConfigText(f) => {
                let rewritten = f(base.config(self.relation)?)?;
                Ok(base.clone().with_file(CONFIG_PATH, &rewritten))
            }
            TransformKind::RewriteFixture(f) => f(base),
            TransformKind::Relocate => Ok(base.clone()),
        }
    }
}

/// The closed transformation table — one entry per mandated relation family (FR-044).
///
/// Closed on purpose. Adding a relation to `metamorphic.json` without adding its
/// transformation here is caught by [`evaluate_catalogue`] as
/// [`HarnessError::RelationUnevaluable`], because the alternative — skipping it — would
/// make a declared relation contribute a silent pass.
pub const TRANSFORMATIONS: &[Transformation] = &[
    Transformation {
        relation: "mrl-formatting-invariance",
        base: rich_base_fixture,
        argv: &[],
        scope: ObservedScope::WholeDocument,
        kind: TransformKind::RewriteConfigText(reindent),
        accounting: Accounting::Identity,
        break_point: BreakPoint::AfterTransform,
    },
    Transformation {
        relation: "mrl-comment-invariance",
        base: rich_base_fixture,
        argv: &[],
        scope: ObservedScope::WholeDocument,
        kind: TransformKind::RewriteConfigText(insert_comments_and_trailing_commas),
        accounting: Accounting::Identity,
        break_point: BreakPoint::BeforeTransform,
    },
    Transformation {
        relation: "mrl-key-order-invariance",
        base: rich_base_fixture,
        argv: &[],
        scope: ObservedScope::WholeDocument,
        kind: TransformKind::RewriteConfigText(reverse_object_members),
        accounting: Accounting::Identity,
        break_point: BreakPoint::AfterTransform,
    },
    Transformation {
        relation: "mrl-path-relocation",
        base: rich_base_fixture,
        argv: &[],
        scope: ObservedScope::WholeDocument,
        kind: TransformKind::Relocate,
        accounting: Accounting::PathTokenization,
        break_point: BreakPoint::AfterTransform,
    },
    Transformation {
        relation: "mrl-lifecycle-equivalence",
        base: rich_base_fixture,
        argv: &[],
        scope: ObservedScope::WholeDocument,
        kind: TransformKind::RewriteConfigText(lifecycle_to_named_object),
        accounting: Accounting::TransformedSites(&[
            "structuredOutput.configuration.postCreateCommand",
        ]),
        break_point: BreakPoint::AfterTransform,
    },
    Transformation {
        relation: "mrl-extends-flattening",
        base: extends_chain_fixture,
        argv: &["--include-merged-configuration"],
        scope: ObservedScope::Block("mergedConfiguration"),
        kind: TransformKind::RewriteFixture(flatten_extends_chain),
        accounting: Accounting::Identity,
        break_point: BreakPoint::AfterTransform,
    },
    Transformation {
        relation: "mrl-declaration-order-sensitivity",
        base: compose_overlay_fixture,
        argv: &[],
        scope: ObservedScope::WholeDocument,
        kind: TransformKind::RewriteConfigText(reverse_compose_overlay_list),
        accounting: Accounting::Identity,
        break_point: BreakPoint::AfterTransform,
    },
];

/// The transformation implementing `relation`, or `None`.
pub fn transformation_for(relation: &str) -> Option<&'static Transformation> {
    TRANSFORMATIONS.iter().find(|t| t.relation == relation)
}

// ---------------------------------------------------------------------------
// Base fixtures
// ---------------------------------------------------------------------------

/// The configuration the four document-level relations are evaluated over.
///
/// Deliberately rich in the shapes that make the relations non-vacuous: nested objects
/// (`customizations`), ordered arrays (`forwardPorts`, `runArgs`,
/// `overrideFeatureInstallOrder`), an unordered map with more than one member (`features`,
/// `containerEnv`), a lifecycle command in its bare string form, and a
/// `${localWorkspaceFolder}` substitution that puts a host absolute path into the result —
/// which is what gives `mrl-path-relocation` something to be invariant *about*.
///
/// The Feature ids are echoed, never resolved: `read-configuration` without
/// `--include-features-configuration` performs no fetch, so this tier stays hermetic.
fn rich_base_fixture() -> Fixture {
    Fixture::with_config(
        r#"{
  "name": "metamorphic-base",
  "image": "alpine:3.18",
  "workspaceFolder": "/workspace",
  "features": {
    "ghcr.io/devcontainers/features/common-utils:2": {},
    "ghcr.io/devcontainers/features/git:1": { "version": "latest" }
  },
  "overrideFeatureInstallOrder": [
    "ghcr.io/devcontainers/features/common-utils",
    "ghcr.io/devcontainers/features/git"
  ],
  "containerEnv": { "ZULU": "z", "ALPHA": "${localWorkspaceFolder}" },
  "forwardPorts": [3000, 3001],
  "runArgs": ["--cap-add", "SYS_PTRACE"],
  "postCreateCommand": "echo hi",
  "customizations": { "vscode": { "settings": { "editor.tabSize": 2 }, "extensions": ["a.b"] } }
}
"#,
    )
}

/// A two-link `extends` chain: a parent contributing an image, an environment variable and
/// a port, and a child contributing a name and a second environment variable.
fn extends_chain_fixture() -> Fixture {
    Fixture::new()
        .with_file(
            ".devcontainer/base.json",
            r#"{
  "image": "alpine:3.18",
  "containerEnv": { "FROM_BASE": "1" },
  "forwardPorts": [3000]
}
"#,
        )
        .with_file(
            CONFIG_PATH,
            r#"{
  "extends": "./base.json",
  "name": "child",
  "containerEnv": { "FROM_CHILD": "2" }
}
"#,
        )
}

/// The hand-flattened equal of [`extends_chain_fixture`] — one document, no parent.
fn flatten_extends_chain(base: &Fixture) -> Result<Fixture, HarnessError> {
    // Guard the correspondence rather than assume it: a flattening that silently stopped
    // matching its chain would make the relation compare two unrelated documents and
    // report a difference that says nothing about deacon.
    if !base.files().contains_key(".devcontainer/base.json") {
        return Err(HarnessError::RelationUnevaluable {
            relation: "mrl-extends-flattening".to_string(),
            cause: "the base fixture declares no parent document to flatten".to_string(),
        });
    }
    Ok(base
        .clone()
        .without_file(".devcontainer/base.json")
        .with_file(
            CONFIG_PATH,
            r#"{
  "name": "child",
  "image": "alpine:3.18",
  "containerEnv": { "FROM_BASE": "1", "FROM_CHILD": "2" },
  "forwardPorts": [3000]
}
"#,
        ))
}

/// A Compose configuration whose `dockerComposeFile` is an ordered overlay list — the
/// collection the cited clause says order matters for.
fn compose_overlay_fixture() -> Fixture {
    Fixture::with_config(
        r#"{
  "name": "compose-overlay",
  "dockerComposeFile": ["docker-compose.yml", "docker-compose.override.yml"],
  "service": "app",
  "workspaceFolder": "/workspace"
}
"#,
    )
}

// ---------------------------------------------------------------------------
// Text transformations
// ---------------------------------------------------------------------------

/// **`mrl-formatting-invariance`** — re-emit the document with different indentation and
/// line wrapping, changing no token.
///
/// Every inter-token space is dropped and replaced with our own, which is token-preserving
/// **by construction** rather than by care: in JSON, whitespace appears only *between*
/// tokens, and two value tokens are never adjacent without a structural character
/// (`{}[]:,`) between them, so no re-spacing can fuse or split a token. The test asserts
/// the consequence non-circularly — both texts parse to the same value.
pub fn reindent(text: &str) -> Result<String, HarnessError> {
    let mut out = String::with_capacity(text.len() * 2);
    let mut depth: usize = 0;
    let mut in_string = false;
    let mut escaped = false;

    // A deliberately unusual layout: tab indentation, a blank line after every `{`, and a
    // space on both sides of `:`. Nothing about it is a token.
    let indent = |out: &mut String, depth: usize| {
        out.push('\n');
        for _ in 0..depth {
            out.push('\t');
        }
    };

    for c in text.chars() {
        if in_string {
            out.push(c);
            if escaped {
                escaped = false;
            } else if c == '\\' {
                escaped = true;
            } else if c == '"' {
                in_string = false;
            }
            continue;
        }
        match c {
            '"' => {
                in_string = true;
                out.push(c);
            }
            w if w.is_whitespace() => {}
            '{' | '[' => {
                out.push(c);
                depth += 1;
                indent(&mut out, depth);
            }
            '}' | ']' => {
                depth = depth.saturating_sub(1);
                indent(&mut out, depth);
                out.push(c);
            }
            ':' => out.push_str(" :  "),
            ',' => {
                out.push(c);
                indent(&mut out, depth);
            }
            other => out.push(other),
        }
    }
    out.push('\n');
    Ok(out)
}

/// **`mrl-comment-invariance`** — insert JSONC line and block comments, and a trailing
/// comma in every non-empty object and array.
pub fn insert_comments_and_trailing_commas(text: &str) -> Result<String, HarnessError> {
    let mut out = String::with_capacity(text.len() * 2);
    out.push_str("// injected: a leading line comment\n");
    let mut in_string = false;
    let mut escaped = false;

    for c in text.chars() {
        if in_string {
            out.push(c);
            if escaped {
                escaped = false;
            } else if c == '\\' {
                escaped = true;
            } else if c == '"' {
                in_string = false;
            }
            continue;
        }
        match c {
            '"' => {
                in_string = true;
                out.push(c);
            }
            '{' | '[' => {
                out.push(c);
                out.push_str(" /* injected: an opening block comment */ ");
            }
            '}' | ']' => {
                // A trailing comma only where the container has content: `{,}` and `[,]`
                // are not JSONC, and emitting one would test the parser's error handling
                // rather than its comment tolerance.
                if last_meaningful(&out).is_some_and(|p| !matches!(p, '{' | '[' | ',')) {
                    out.push(',');
                }
                out.push_str("\n// injected: a trailing line comment\n");
                out.push(c);
            }
            other => out.push(other),
        }
    }
    out.push_str("\n// injected: a closing line comment\n");
    Ok(out)
}

/// The last emitted character that is neither whitespace nor part of an injected comment.
///
/// Scans backwards over whitespace and over any `//`-comment line we just wrote, which is
/// enough because this rewriter only ever appends comments in those two shapes.
fn last_meaningful(out: &str) -> Option<char> {
    for line in out.lines().rev() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with("//") {
            continue;
        }
        // Drop a trailing block comment we injected on the same line.
        let head = match trimmed.rfind("*/") {
            Some(idx) => trimmed[..trimmed[..idx].rfind("/*").unwrap_or(0)].trim_end(),
            None => trimmed,
        };
        if let Some(c) = head.chars().next_back() {
            return Some(c);
        }
    }
    None
}

/// **`mrl-key-order-invariance`** — reverse the member order of every JSON object.
pub fn reverse_object_members(text: &str) -> Result<String, HarnessError> {
    let value = parse_config(text, "mrl-key-order-invariance")?;
    Ok(render_config(&reverse_members(&value)))
}

fn reverse_members(value: &Value) -> Value {
    match value {
        Value::Object(map) => {
            // `preserve_order` makes a `Map` insertion-ordered, so rebuilding it in reverse
            // genuinely changes the document's member order — which is exactly the
            // transformation. It does NOT change the value: `Map`'s equality is map
            // equality.
            let mut out = Map::new();
            for (k, v) in map.iter().rev() {
                out.insert(k.clone(), reverse_members(v));
            }
            Value::Object(out)
        }
        // Arrays keep their order: reversing one would be the *sensitivity* relation, and
        // conflating the two would make this relation assert something it does not mean.
        Value::Array(items) => Value::Array(items.iter().map(reverse_members).collect()),
        other => other.clone(),
    }
}

/// **`mrl-lifecycle-equivalence`** — rewrite `postCreateCommand` from its bare form into the
/// single-entry named-object form denoting the same command.
pub fn lifecycle_to_named_object(text: &str) -> Result<String, HarnessError> {
    const RELATION: &str = "mrl-lifecycle-equivalence";
    let mut value = parse_config(text, RELATION)?;
    let object = value
        .as_object_mut()
        .ok_or_else(|| HarnessError::RelationUnevaluable {
            relation: RELATION.to_string(),
            cause: "the configuration is not a JSON object".to_string(),
        })?;
    let existing = object
        .get("postCreateCommand")
        .cloned()
        .filter(|v| !v.is_null() && !v.is_object())
        .ok_or_else(|| HarnessError::RelationUnevaluable {
            relation: RELATION.to_string(),
            cause: "the base fixture declares no bare-form `postCreateCommand` to rewrite; \
                    with nothing to transform the relation would compare a document with \
                    itself and hold vacuously"
                .to_string(),
        })?;
    let mut named = Map::new();
    named.insert("solo".to_string(), existing);
    object.insert("postCreateCommand".to_string(), Value::Object(named));
    Ok(render_config(&value))
}

/// **`mrl-declaration-order-sensitivity`** — reverse the `dockerComposeFile` overlay list.
pub fn reverse_compose_overlay_list(text: &str) -> Result<String, HarnessError> {
    const RELATION: &str = "mrl-declaration-order-sensitivity";
    let mut value = parse_config(text, RELATION)?;
    let object = value
        .as_object_mut()
        .ok_or_else(|| HarnessError::RelationUnevaluable {
            relation: RELATION.to_string(),
            cause: "the configuration is not a JSON object".to_string(),
        })?;
    let list = object
        .get_mut("dockerComposeFile")
        .and_then(Value::as_array_mut)
        .ok_or_else(|| HarnessError::RelationUnevaluable {
            relation: RELATION.to_string(),
            cause: "the base fixture declares no `dockerComposeFile` array".to_string(),
        })?;
    if list.len() < 2 {
        // A one-element list has exactly one order, so reversing it changes nothing. A
        // sensitivity relation over it could never be satisfied, and reporting that as a
        // *failed relation* would blame deacon for a defective fixture.
        return Err(HarnessError::RelationUnevaluable {
            relation: RELATION.to_string(),
            cause: format!(
                "`dockerComposeFile` has {} entr(y/ies); a declaration-ordered collection \
                 needs at least two for a permutation to exist",
                list.len()
            ),
        });
    }
    list.reverse();
    Ok(render_config(&value))
}

/// Parse an authored configuration document, tolerating nothing.
///
/// The base fixtures are plain JSON by construction (the comment-bearing document is
/// *produced* by a transformation, never consumed by one), so a parse failure here is a
/// fixture defect and is reported as one rather than as a relation failure.
fn parse_config(text: &str, relation: &str) -> Result<Value, HarnessError> {
    serde_json::from_str(text).map_err(|e| HarnessError::RelationUnevaluable {
        relation: relation.to_string(),
        cause: format!("the base configuration is not parseable JSON: {e}"),
    })
}

fn render_config(value: &Value) -> String {
    let mut out = serde_json::to_string_pretty(value)
        .unwrap_or_else(|e| unreachable!("re-rendering a parsed document is infallible: {e}"));
    out.push('\n');
    out
}

// ---------------------------------------------------------------------------
// Sabotage — the SC-011 anti-inert probe
// ---------------------------------------------------------------------------

/// Whether to evaluate a relation honestly, or to deliberately break it.
///
/// The break is applied to the **input**, never to an observation: perturbing what the
/// comparison *returns* would make a comparison that ignores its arguments look alive, the
/// same defect 024's regression harness forbids by sealing its injection boundary. Here the
/// two breaks are the two ways a relation can genuinely be violated, so a relation that
/// still "holds" under one is not observing its own transformation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Sabotage {
    /// Evaluate the relation as declared.
    None,
    /// Break it: give an invariance relation a genuinely different input, and take a
    /// sensitivity relation's transformation away.
    Break,
}

/// The saboteur for an invariance relation: change something the resolved configuration
/// must reflect, so the two sides genuinely differ.
fn perturb_invariant_input(fixture: &Fixture, relation: &str) -> Result<Fixture, HarnessError> {
    let mut value = parse_config(fixture.config(relation)?, relation)?;
    let object = value
        .as_object_mut()
        .ok_or_else(|| HarnessError::RelationUnevaluable {
            relation: relation.to_string(),
            cause: "the configuration is not a JSON object".to_string(),
        })?;
    object.insert(
        "name".to_string(),
        Value::String("sabotaged-metamorphic-input".to_string()),
    );
    Ok(fixture
        .clone()
        .with_file(CONFIG_PATH, &render_config(&value)))
}

// ---------------------------------------------------------------------------
// Evaluation
// ---------------------------------------------------------------------------

/// One difference between the two runs that the relation's accounting did not absorb.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Residual {
    /// The observable path within the evidence document.
    pub path: String,
    /// The difference kind, in the wire spelling the signature uses.
    pub kind: String,
    /// What the original run produced.
    pub original: Option<Value>,
    /// What the transformed run produced.
    pub transformed: Option<Value>,
}

/// One side's evidence: the fixture that produced it, and what deacon did with it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SideEvidence {
    /// The materialized workspace tree.
    pub input: Fixture,
    /// The absolute workspace path it was materialized at.
    pub workspace: String,
    /// The RAW evidence document, before normalization.
    pub raw: Value,
    /// The NORMALIZED evidence document. Held separately from [`raw`](Self::raw), never
    /// derived on read: raw and normalized evidence must never be conflated (FR-014, the
    /// FR-016 precedent from 022).
    pub normalized: Value,
}

/// The result of evaluating one relation over one base fixture.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RelationOutcome {
    /// The `mrl-` id.
    pub relation: String,
    /// The transformation as the catalogue words it — what a reviewer reproduces.
    pub transformation: String,
    /// What the relation asserts.
    pub effect: RelationEffect,
    /// Whether the assertion held.
    pub holds: bool,
    /// The original run.
    pub original: SideEvidence,
    /// The transformed run.
    pub transformed: SideEvidence,
    /// Differences the accounting did not absorb.
    pub residual: Vec<Residual>,
    /// Differences the accounting DID absorb, retained rather than discarded: for
    /// `mrl-path-relocation` this is the evidence that the tokenization did its job, and
    /// for a scoped-site relation it is the transformation showing up where it should.
    pub accounted: Vec<Residual>,
}

impl RelationOutcome {
    /// The reviewable candidate for a failed relation (FR-047), or `None` when it held.
    pub fn candidate(&self) -> Option<MetamorphicCandidate> {
        if self.holds {
            return None;
        }
        Some(MetamorphicCandidate {
            relation: self.relation.clone(),
            transformation: self.transformation.clone(),
            effect: self.effect,
            original_input: self.original.input.clone(),
            transformed_input: self.transformed.input.clone(),
            original_normalized: self.original.normalized.clone(),
            transformed_normalized: self.transformed.normalized.clone(),
            residual: self.residual.clone(),
        })
    }
}

/// A metamorphic failure, in the same reviewable shape a differential finding takes so both
/// enter one triage pipeline (FR-047, contracts/metamorphic-catalogue.md § Failure output).
///
/// Names the relation, the transformation applied, **both** inputs, and **both** normalized
/// outputs — every one of them required, because a candidate a reviewer cannot reproduce
/// from its own contents is a bug report with the evidence left behind.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MetamorphicCandidate {
    /// The relation that failed.
    pub relation: String,
    /// The transformation that was applied.
    pub transformation: String,
    /// What the relation asserted.
    pub effect: RelationEffect,
    /// The input the original run saw.
    pub original_input: Fixture,
    /// The input the transformed run saw.
    pub transformed_input: Fixture,
    /// The original run's normalized evidence.
    pub original_normalized: Value,
    /// The transformed run's normalized evidence.
    pub transformed_normalized: Value,
    /// The differences the accounting did not absorb (empty for a sensitivity failure,
    /// where the absence of a difference IS the failure).
    pub residual: Vec<Residual>,
}

impl MetamorphicCandidate {
    /// The deduplication keys for this candidate.
    ///
    /// One [`Signature`] per residual difference, derived by the SAME function the
    /// differential uses, so a metamorphic finding and a differential finding at the same
    /// path deduplicate against each other — which is correct: they are the same defect
    /// observed two ways.
    ///
    /// A **sensitivity** failure yields none, and that is not an oversight: its failure is
    /// the *absence* of a difference, and there is nothing to key on. Keying it on the site
    /// the transformation touched would collide with a genuine value difference at that
    /// path and merge two unrelated defects. Such a candidate is identified by its relation.
    pub fn signatures(&self, channel: &str) -> Vec<Signature> {
        self.residual
            .iter()
            .filter_map(|r| {
                let kind = DivergenceKind::parse(&r.kind)?;
                Some(Signature::derive(
                    channel,
                    // `reference` is the original run and `deacon` is the transformed one,
                    // fixed once here (see the module docs) so `deacon-only` reads as "the
                    // transformation introduced this".
                    &Divergence {
                        kind,
                        path: &r.path,
                        deacon: r.transformed.as_ref(),
                        reference: r.original.as_ref(),
                    },
                ))
            })
            .collect()
    }
}

/// Evaluate every relation in `catalogue`, in catalogue order.
///
/// Fails loudly — never skips — for a declared relation with no transformation: see
/// [`HarnessError::RelationUnevaluable`].
pub async fn evaluate_catalogue(
    deacon: &Path,
    root: &Path,
    catalogue: &[MetamorphicRelation],
    sabotage: Sabotage,
) -> Result<Vec<RelationOutcome>, HarnessError> {
    let mut out = Vec::with_capacity(catalogue.len());
    for (index, relation) in catalogue.iter().enumerate() {
        // A per-relation subdirectory so one relation's wipe cannot reach another's tree,
        // and an index prefix so the layout is stable and readable after a failure.
        let relation_root = root.join(format!("{index:02}-{}", relation.id));
        out.push(evaluate(deacon, &relation_root, relation, sabotage).await?);
    }
    Ok(out)
}

/// Evaluate one relation against deacon alone.
pub async fn evaluate(
    deacon: &Path,
    root: &Path,
    relation: &MetamorphicRelation,
    sabotage: Sabotage,
) -> Result<RelationOutcome, HarnessError> {
    let transformation =
        transformation_for(&relation.id).ok_or_else(|| HarnessError::RelationUnevaluable {
            relation: relation.id.clone(),
            cause: "no transformation is registered for it in `TRANSFORMATIONS`".to_string(),
        })?;

    let base = (transformation.base)();
    let breaking_invariance =
        sabotage == Sabotage::Break && relation.effect == RelationEffect::Invariance;

    // Breaking an INVARIANCE relation means giving the transformed side a genuinely
    // different input; breaking a SENSITIVITY relation means taking its transformation
    // away, so the two sides become identical and a relation that still reports "changed"
    // is not observing its own transformation. Which side of the transformation the
    // invariance perturbation goes on is declared per relation (see [`BreakPoint`]) rather
    // than guessed, because a break that quietly failed to land would leave the relation
    // holding — and a relation holding under its own break is indistinguishable from a
    // healthy one.
    let source = if breaking_invariance && transformation.break_point == BreakPoint::BeforeTransform
    {
        perturb_invariant_input(&base, &relation.id)?
    } else {
        base.clone()
    };
    let mut transformed_input =
        if sabotage == Sabotage::Break && relation.effect == RelationEffect::Sensitivity {
            source.clone()
        } else {
            transformation.apply(&source)?
        };
    if breaking_invariance && transformation.break_point == BreakPoint::AfterTransform {
        transformed_input = perturb_invariant_input(&transformed_input, &relation.id)?;
    }

    let (ws_original, ws_transformed) = workspace_paths(root, transformation.relocates());

    let original = run_side(
        deacon,
        relation,
        transformation,
        &base,
        &ws_original,
        "original",
        sabotage,
    )
    .await?;
    let transformed = run_side(
        deacon,
        relation,
        transformation,
        &transformed_input,
        &ws_transformed,
        "transformed",
        sabotage,
    )
    .await?;

    let (residual, accounted) = reconcile(
        &original.normalized,
        &transformed.normalized,
        transformation.accounting,
    );

    let holds = match relation.effect {
        RelationEffect::Invariance => residual.is_empty(),
        // Sensitivity asks whether ANY difference appeared, including one the accounting
        // would have explained away for an invariance relation — the assertion is "the
        // result changed", not "the result changed somewhere unaccounted".
        RelationEffect::Sensitivity => !residual.is_empty() || !accounted.is_empty(),
    };

    Ok(RelationOutcome {
        relation: relation.id.clone(),
        transformation: relation.transformation.clone(),
        effect: relation.effect,
        holds,
        original,
        transformed,
        residual,
        accounted,
    })
}

/// Where each side is materialized.
///
/// A non-relocating relation uses **one** directory for both runs, so the only thing that
/// differs between the two sides is the transformation. Giving them separate directories
/// would put a different absolute path into every result and force every relation to
/// tokenize — which would quietly turn `mrl-path-relocation` into the same relation as the
/// others and delete the FR-046 residual check.
fn workspace_paths(root: &Path, relocates: bool) -> (PathBuf, PathBuf) {
    if relocates {
        (
            root.join("origin").join(WORKSPACE_DIR_NAME),
            root.join("relocated").join(WORKSPACE_DIR_NAME),
        )
    } else {
        let single = root.join(WORKSPACE_DIR_NAME);
        (single.clone(), single)
    }
}

/// Materialize one side, run deacon over it, and capture raw + normalized evidence.
#[allow(clippy::too_many_arguments)]
async fn run_side(
    deacon: &Path,
    relation: &MetamorphicRelation,
    transformation: &Transformation,
    fixture: &Fixture,
    workspace: &Path,
    side_name: &str,
    sabotage: Sabotage,
) -> Result<SideEvidence, HarnessError> {
    fixture.materialize(workspace)?;
    let workspace_arg = workspace.to_string_lossy().into_owned();
    let mut args: Vec<&str> = vec!["read-configuration", "--workspace-folder", &workspace_arg];
    args.extend_from_slice(transformation.argv);

    // The sabotage state is part of the artifact name, not only the outcome: the SC-011
    // probe evaluates each relation twice — once clean, once broken — and a shared name
    // would have the second run silently overwrite the first's raw stdout/stderr. The one
    // pair a reviewer most needs side by side is exactly the pair that would be lost.
    let case = match sabotage {
        Sabotage::None => format!("{}-{side_name}", relation.id),
        Sabotage::Break => format!("{}-broken-{side_name}", relation.id),
    };
    let invocation = run_and_capture(
        Side::Deacon,
        "discovery_metamorphic",
        &case,
        deacon,
        &args,
        workspace,
        ExecKind::Config.bound(),
        &crate::discovery_report_root(),
    )
    .await?;

    let raw = raw_evidence(&invocation, transformation.scope);
    let normalized = normalize_evidence(&raw, workspace, transformation.accounting);
    Ok(SideEvidence {
        input: fixture.clone(),
        workspace: workspace_arg,
        raw,
        normalized,
    })
}

/// Build the raw evidence document from one invocation.
///
/// A run that failed, or whose stdout is not JSON, yields `structuredOutput: null` rather
/// than an error: the exit code is itself a declared channel, and a relation whose two
/// sides disagree about whether the configuration resolves is exactly the failure the
/// relation exists to catch. Turning that into a harness error would report deacon's
/// divergence as the harness being broken.
fn raw_evidence(invocation: &Invocation, scope: ObservedScope) -> Value {
    let document = invocation
        .stdout_json()
        .ok()
        .map(|doc| match scope {
            ObservedScope::WholeDocument => doc,
            ObservedScope::Block(name) => doc.get(name).cloned().unwrap_or(Value::Null),
        })
        .unwrap_or(Value::Null);
    let mut out = Map::new();
    out.insert(
        "exitCode".to_string(),
        match invocation.exit_code {
            Some(code) => Value::from(code),
            None => Value::Null,
        },
    );
    out.insert("structuredOutput".to_string(), document);
    Value::Object(out)
}

/// Normalize one side's raw evidence: the single [`crate::normalize`] rule chain, plus the
/// workspace-path tokenization when — and only when — the relation's accounting declares it.
///
/// The chain is reused whole rather than composed from a metamorphic-specific subset.
/// FR-015 permits exactly one normalization definition, and a bespoke chain here would be a
/// second one — free to disagree with the one every other comparison uses, which is the
/// defect that rule exists to prevent.
///
/// The cost is worth naming. `drop_absent_optional` was written to reconcile a *serializer*
/// difference between deacon and the reference, and here both sides are deacon, so it can
/// only ever hide — specifically, a transformation that turned an enumerated key from
/// `[]` into absent would compare equal. It applies identically to both sides, so it cannot
/// manufacture a difference, only mask one; and the relation that most needs the strictness,
/// `mrl-extends-flattening`, observes the `mergedConfiguration` block on its own, where the
/// rule's scope check finds no wrapper key and elides nothing at all.
fn normalize_evidence(raw: &Value, workspace: &Path, accounting: Accounting) -> Value {
    let mut out = Map::new();
    out.insert(
        "exitCode".to_string(),
        raw.get("exitCode").cloned().unwrap_or(Value::Null),
    );
    let document = raw.get("structuredOutput").cloned().unwrap_or(Value::Null);
    let ruled = normalize::config_document_rules(&document, Side::Deacon, DocumentBlock::Wrapper);
    let document = if accounting == Accounting::PathTokenization {
        normalize::path_token(&ruled, &workspace_tokens(workspace))
    } else {
        ruled
    };
    out.insert("structuredOutput".to_string(), document);
    Value::Object(out)
}

/// The `<WORKSPACE>` token map for one side of a relocation.
///
/// Carries BOTH the path as given and its canonicalized form. deacon canonicalizes the
/// workspace folder before substituting it, so on a host where the temp root is a symlink
/// the emitted path is the resolved one and a map built from the given path alone would
/// substitute nothing — producing a residual that looks like a leaked path and is really a
/// gap in the token map.
fn workspace_tokens(workspace: &Path) -> TokenMap {
    let mut tokens = TokenMap::workspace(workspace);
    if let Ok(canonical) = std::fs::canonicalize(workspace)
        && canonical != workspace
    {
        tokens.insert(canonical.to_string_lossy(), "<WORKSPACE>");
    }
    tokens
}

/// Split the differences between two normalized documents into those the accounting
/// absorbs and those it does not.
fn reconcile(
    original: &Value,
    transformed: &Value,
    accounting: Accounting,
) -> (Vec<Residual>, Vec<Residual>) {
    let mut residual = Vec::new();
    let mut accounted = Vec::new();
    // `diff`'s two sides are named for the differential: `reference` is the original run,
    // `deacon` the transformed one (see the module docs).
    for divergence in normalize::diff(transformed, original) {
        let entry = Residual {
            path: divergence.path.clone(),
            kind: kind_str(divergence.kind).to_string(),
            original: divergence.reference.clone(),
            transformed: divergence.deacon.clone(),
        };
        if accounting_absorbs(accounting, &divergence.path) {
            accounted.push(entry);
        } else {
            residual.push(entry);
        }
    }
    (residual, accounted)
}

/// Whether `accounting` explains a difference at `path`.
///
/// `PathTokenization` absorbs **nothing** here on purpose. The tokenization has already run
/// on both sides during normalization, so anything still differing is precisely the
/// residual FR-046 requires be reported — a path the tokenizer did not account for. Making
/// this arm absorb path-shaped differences would delete the check the relation exists for.
fn accounting_absorbs(accounting: Accounting, path: &str) -> bool {
    match accounting {
        Accounting::Identity | Accounting::PathTokenization => false,
        Accounting::TransformedSites(sites) => sites
            .iter()
            .any(|site| path == *site || path.starts_with(&format!("{site}."))),
    }
}

fn kind_str(kind: DiffKind) -> &'static str {
    match kind {
        DiffKind::RefOnly => "ref-only",
        DiffKind::DeaconOnly => "deacon-only",
        DiffKind::Value => "value",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn relation(id: &str, effect: RelationEffect) -> MetamorphicRelation {
        MetamorphicRelation {
            id: id.to_string(),
            transformation: "t".to_string(),
            effect,
            ground: "bhv-x".to_string(),
            channels: vec!["chan-structured-output".to_string()],
            rationale: "r".to_string(),
        }
    }

    #[test]
    fn every_mandated_family_has_a_transformation() {
        // The anti-inert guarantee at the table level: a family declared in the registry
        // with no implementation here would be skipped, and a skipped relation reports
        // nothing — which is indistinguishable from holding (SC-011).
        for family in deacon_conformance::discovery::metamorphic::MANDATED_RELATIONS {
            assert!(
                transformation_for(family).is_some(),
                "no transformation implements mandated relation `{family}`"
            );
        }
        assert_eq!(
            TRANSFORMATIONS.len(),
            deacon_conformance::discovery::metamorphic::MANDATED_RELATIONS.len()
        );
    }

    #[tokio::test]
    async fn an_unimplemented_relation_fails_loudly_rather_than_skipping() {
        let dir = tempfile::tempdir().expect("tempdir");
        let err = evaluate(
            Path::new("/nonexistent/deacon"),
            dir.path(),
            &relation("mrl-invented", RelationEffect::Invariance),
            Sabotage::None,
        )
        .await
        .expect_err("an unimplemented relation must not be skipped");
        match err {
            HarnessError::RelationUnevaluable { relation, .. } => {
                assert_eq!(relation, "mrl-invented")
            }
            other => panic!("expected RelationUnevaluable, got {other:?}"),
        }
    }

    #[test]
    fn reindent_changes_layout_and_no_token() {
        let original = (rich_base_fixture)()
            .config("t")
            .expect("config")
            .to_string();
        let rewritten = reindent(&original).expect("reindents");
        assert_ne!(rewritten, original, "the layout must actually change");
        assert!(rewritten.contains('\t'), "the rewrite uses tab indentation");
        // Token preservation, checked the only way that is not circular: both texts parse
        // to the same value.
        let a: Value = serde_json::from_str(&original).expect("original parses");
        let b: Value = serde_json::from_str(&rewritten).expect("rewritten parses");
        assert_eq!(a, b, "reindentation must preserve every token");
    }

    #[test]
    fn comment_injection_produces_valid_jsonc_with_no_empty_container_comma() {
        let original = (rich_base_fixture)()
            .config("t")
            .expect("config")
            .to_string();
        let rewritten = insert_comments_and_trailing_commas(&original).expect("rewrites");
        assert!(rewritten.contains("// injected"));
        assert!(rewritten.contains("/* injected"));
        // `{}` must not become `{,}`: the fixture's `common-utils` options object is empty,
        // and a comma there would test the parser's error handling rather than its comment
        // tolerance.
        assert!(
            !rewritten.replace(char::is_whitespace, "").contains("{,"),
            "an empty object must not receive a trailing comma:\n{rewritten}"
        );
        assert!(!rewritten.replace(char::is_whitespace, "").contains("[,"));
        // A non-empty container DOES get one.
        assert!(
            rewritten.contains(','),
            "a non-empty container must receive a trailing comma"
        );
    }

    #[test]
    fn member_reversal_changes_order_but_not_the_value() {
        let original = (rich_base_fixture)()
            .config("t")
            .expect("config")
            .to_string();
        let rewritten = reverse_object_members(&original).expect("rewrites");
        assert_ne!(rewritten, original);
        let a: Value = serde_json::from_str(&original).expect("parses");
        let b: Value = serde_json::from_str(&rewritten).expect("parses");
        assert_eq!(a, b, "objects compare as maps, so the VALUE is unchanged");
        // The rendered order really did change: `name` led the original document.
        assert!(original.trim_start().starts_with('{'));
        let first_key = |s: &str| s.split('"').nth(1).unwrap_or_default().to_string();
        assert_ne!(first_key(&original), first_key(&rewritten));
        // Arrays keep their order — reversing one would be the sensitivity relation.
        assert_eq!(
            b.get("forwardPorts"),
            a.get("forwardPorts"),
            "array order is not a member order"
        );
    }

    #[test]
    fn lifecycle_rewrite_wraps_the_bare_form_and_refuses_a_missing_one() {
        let rewritten =
            lifecycle_to_named_object((rich_base_fixture)().config("t").expect("config"))
                .expect("rewrites");
        let value: Value = serde_json::from_str(&rewritten).expect("parses");
        assert_eq!(
            value.get("postCreateCommand"),
            Some(&serde_json::json!({ "solo": "echo hi" }))
        );

        // Nothing to transform is a fixture defect, not a vacuously holding relation.
        let err = lifecycle_to_named_object(r#"{ "image": "alpine:3.18" }"#)
            .expect_err("a fixture with no bare lifecycle command must fail loudly");
        assert!(matches!(err, HarnessError::RelationUnevaluable { .. }));
    }

    #[test]
    fn compose_reversal_needs_at_least_two_entries() {
        let rewritten =
            reverse_compose_overlay_list((compose_overlay_fixture)().config("t").expect("config"))
                .expect("rewrites");
        let value: Value = serde_json::from_str(&rewritten).expect("parses");
        assert_eq!(
            value.get("dockerComposeFile"),
            Some(&serde_json::json!([
                "docker-compose.override.yml",
                "docker-compose.yml"
            ]))
        );

        // A one-element list has one order, so a sensitivity relation over it could never
        // be satisfied. Reporting that as a failed relation would blame deacon for a
        // defective fixture.
        let err = reverse_compose_overlay_list(r#"{ "dockerComposeFile": ["only.yml"] }"#)
            .expect_err("a single-entry list must fail loudly");
        assert!(matches!(err, HarnessError::RelationUnevaluable { .. }));
    }

    #[test]
    fn flattening_matches_the_chain_it_replaces() {
        let chain = (extends_chain_fixture)();
        let flat = flatten_extends_chain(&chain).expect("flattens");
        assert!(
            !flat.files().contains_key(".devcontainer/base.json"),
            "the flattened fixture has no parent"
        );
        let flat_config: Value =
            serde_json::from_str(flat.config("t").expect("config")).expect("parses");
        assert!(flat_config.get("extends").is_none());
        // A fixture with nothing to flatten fails loudly rather than comparing a document
        // with itself.
        let err = flatten_extends_chain(&Fixture::with_config("{}"))
            .expect_err("a chain-less fixture must fail loudly");
        assert!(matches!(err, HarnessError::RelationUnevaluable { .. }));
    }

    #[test]
    fn the_break_point_matches_what_each_transformation_can_carry() {
        // `mrl-comment-invariance` emits JSONC, so a perturbation applied AFTER it would
        // have to re-parse comments as strict JSON and would fail — the exact failure this
        // declaration exists to prevent.
        assert_eq!(
            transformation_for("mrl-comment-invariance")
                .expect("declared")
                .break_point,
            BreakPoint::BeforeTransform
        );
        // `mrl-extends-flattening` emits the hand-written flattening and discards its
        // input's content, so a perturbation applied BEFORE it would vanish and both sides
        // would still agree — a break that never landed, reported as a healthy relation.
        assert_eq!(
            transformation_for("mrl-extends-flattening")
                .expect("declared")
                .break_point,
            BreakPoint::AfterTransform
        );
        // Every transformation whose output is strict JSON perturbs afterwards.
        for transformation in TRANSFORMATIONS {
            if transformation.relation != "mrl-comment-invariance" {
                assert_eq!(
                    transformation.break_point,
                    BreakPoint::AfterTransform,
                    "{}",
                    transformation.relation
                );
            }
        }
    }

    #[test]
    fn a_non_relocating_relation_reuses_one_workspace() {
        let root = Path::new("/tmp/x");
        let (a, b) = workspace_paths(root, false);
        assert_eq!(a, b, "one directory, so only the transformation differs");
        let (a, b) = workspace_paths(root, true);
        assert_ne!(a, b);
        assert_eq!(
            a.file_name(),
            b.file_name(),
            "the basename is held constant so relocation is a pure change of absolute path"
        );
    }

    #[test]
    fn the_scoped_accounting_absorbs_only_its_named_sites() {
        let sites =
            Accounting::TransformedSites(&["structuredOutput.configuration.postCreateCommand"]);
        assert!(accounting_absorbs(
            sites,
            "structuredOutput.configuration.postCreateCommand"
        ));
        assert!(accounting_absorbs(
            sites,
            "structuredOutput.configuration.postCreateCommand.solo"
        ));
        // A sibling whose name merely starts the same way is NOT absorbed — the site is a
        // path, not a prefix match.
        assert!(!accounting_absorbs(
            sites,
            "structuredOutput.configuration.postCreateCommandExtra"
        ));
        assert!(!accounting_absorbs(
            sites,
            "structuredOutput.configuration.name"
        ));
        // The path-tokenization accounting absorbs nothing: tokenization already ran, so
        // whatever still differs IS the FR-046 residual.
        assert!(!accounting_absorbs(
            Accounting::PathTokenization,
            "anything"
        ));
        assert!(!accounting_absorbs(Accounting::Identity, "anything"));
    }

    #[test]
    fn a_candidate_names_the_relation_the_transformation_both_inputs_and_both_outputs() {
        let outcome = RelationOutcome {
            relation: "mrl-formatting-invariance".to_string(),
            transformation: "reindent the configuration document".to_string(),
            effect: RelationEffect::Invariance,
            holds: false,
            original: SideEvidence {
                input: Fixture::with_config("{\"name\":\"a\"}"),
                workspace: "/ws".to_string(),
                raw: serde_json::json!({ "exitCode": 0 }),
                normalized: serde_json::json!({ "structuredOutput": { "name": "a" } }),
            },
            transformed: SideEvidence {
                input: Fixture::with_config("{\n  \"name\": \"b\"\n}"),
                workspace: "/ws".to_string(),
                raw: serde_json::json!({ "exitCode": 0 }),
                normalized: serde_json::json!({ "structuredOutput": { "name": "b" } }),
            },
            residual: vec![Residual {
                path: "structuredOutput.name".to_string(),
                kind: "value".to_string(),
                original: Some(Value::String("a".to_string())),
                transformed: Some(Value::String("b".to_string())),
            }],
            accounted: vec![],
        };
        let candidate = outcome.candidate().expect("a failed relation yields one");
        assert_eq!(candidate.relation, "mrl-formatting-invariance");
        assert!(candidate.transformation.contains("reindent"));
        assert_ne!(candidate.original_input, candidate.transformed_input);
        assert_ne!(
            candidate.original_normalized,
            candidate.transformed_normalized
        );

        // The signature is derived by the same function the differential uses, so the two
        // deduplicate against each other.
        let signatures = candidate.signatures("chan-structured-output");
        assert_eq!(signatures.len(), 1);
        assert_eq!(signatures[0].path, "structuredOutput.name");
        assert_eq!(signatures[0].derived_id(), signatures[0].id);

        // A relation that held yields no candidate at all.
        let mut held = outcome.clone();
        held.holds = true;
        assert!(held.candidate().is_none());
    }
}
