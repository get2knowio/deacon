//! Repository-owned conformance registry — library surface (dev-only crate
//! `deacon-conformance`, `publish = false`).
//!
//! This crate models, loads, validates, and reports on the conformance registry
//! stored as strict JSON under `conformance/registry/`. It is contributor tooling
//! (constitution II — NOT part of the published `deacon` consumer CLI); the
//! `conformance` binary (`validate` / `report` / `certify`) is invoked via
//! `cargo run -p deacon-conformance -- <subcommand>`.
//!
//! Modules land incrementally per the feature plan:
//! - [`model`] — record types, closed enums, and ID rules (T002);
//! - [`load`] — the registry loader with located schema errors (T003);
//! - [`validate`] — the violation-class engine V1–V10 + SCHEMA (US1, T006–T010);
//! - [`coverage`] — derived per-behavior coverage evaluation (US2, T016);
//! - [`report`] — deterministic `report.json` / `report.md` generation (US2, T017–T018);
//! - [`certify`] — strict certification for the active profile (US2, T019);
//! - [`diff`] — deterministic revision diff between two constraint inventories
//!   (US3, T030–T031).

pub mod baseline;
pub mod case_hash;
pub mod certify;
pub mod clause;
pub mod clause_diff;
pub mod conservation;
pub mod coverage;
pub mod coverage_report;
pub mod diff;
pub mod discovery;
pub mod inventory;
pub mod load;
pub mod mapping;
pub mod model;
pub mod obligation;
pub mod parity_corpus;
pub mod prose;
pub mod regression;
pub mod report;
pub mod residual;
pub mod scenario;
pub mod schema;
pub mod snapshot;
pub mod validate;

/// Absolute path to the workspace root, derived from this crate's
/// `CARGO_MANIFEST_DIR` (`<root>/crates/conformance`) so paths are stable
/// regardless of the per-package cargo/nextest working directory. Mirrors
/// `parity-harness::workspace_root`.
pub fn workspace_root() -> std::path::PathBuf {
    let manifest = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest
        .parent() // crates/
        .and_then(|p| p.parent()) // <root>
        .map(std::path::Path::to_path_buf)
        .unwrap_or(manifest)
}

/// The default registry root: `<workspace_root>/conformance/registry`. The CLI's
/// `--registry <dir>` flag overrides it (tests point it at fixtures).
pub fn default_registry_dir() -> std::path::PathBuf {
    workspace_root().join("conformance").join("registry")
}

/// The default schemas root: `<workspace_root>/conformance/schemas`. Contains one
/// `<rev-pin>/` subdirectory per vendored schema revision (each with a
/// `manifest.json` and its byte-exact schema files). The CLI's `--schemas <dir>`
/// flag overrides it (tests point it at fixtures)
/// (020-schema-constraint-inventory).
pub fn default_schemas_dir() -> std::path::PathBuf {
    workspace_root().join("conformance").join("schemas")
}

/// The pin of the currently vendored mandatory schema revision (the subdirectory of
/// [`default_schemas_dir`] holding the manifest + byte-exact schema files). Matches the
/// `pin` of the `rev-schema-<pin>` revision record. Bumped only on a conscious
/// re-vendoring (quickstart.md "Re-vendoring") (020-schema-constraint-inventory).
pub const CURRENT_SCHEMA_PIN: &str = "113500f4";

/// The default manifest directory for `inventory generate`/`check`:
/// `<workspace_root>/conformance/schemas/<CURRENT_SCHEMA_PIN>/`. The CLI's `--schemas
/// <dir>` flag overrides it (tests point it at single-document fixture manifests)
/// (020-schema-constraint-inventory).
pub fn default_pinned_schemas_dir() -> std::path::PathBuf {
    default_schemas_dir().join(CURRENT_SCHEMA_PIN)
}

/// The default committed inventory file:
/// `<workspace_root>/conformance/inventory/constraints.json` — the machine-owned,
/// byte-stable constraint inventory. The CLI's `--inventory <file>` /  `--out
/// <file>` flags override it (020-schema-constraint-inventory).
pub fn default_inventory_file() -> std::path::PathBuf {
    workspace_root()
        .join("conformance")
        .join("inventory")
        .join("constraints.json")
}

/// The default spec-prose root: `<workspace_root>/conformance/spec`. Contains one
/// `<rev-pin>/` subdirectory per vendored spec revision (each with a `manifest.json`
/// and the byte-exact vendored Markdown documents). The CLI's `--spec <dir>` flag
/// overrides it (tests point it at fixtures) (021-normative-clause-inventory).
pub fn default_spec_dir() -> std::path::PathBuf {
    workspace_root().join("conformance").join("spec")
}

/// The pin of the currently vendored mandatory spec revision (the subdirectory of
/// [`default_spec_dir`] holding the manifest + byte-exact prose files). Matches the
/// `pin` of the `rev-spec-<pin>` revision record. Bumped only on a conscious
/// re-vendoring (quickstart.md "Re-vendoring") (021-normative-clause-inventory).
pub const CURRENT_SPEC_PIN: &str = "113500f4";

/// The default pinned-spec directory for `clause generate`/`check`:
/// `<workspace_root>/conformance/spec/<CURRENT_SPEC_PIN>/`. The CLI's `--spec <dir>`
/// flag overrides it (021-normative-clause-inventory).
pub fn default_pinned_spec_dir() -> std::path::PathBuf {
    default_spec_dir().join(CURRENT_SPEC_PIN)
}

/// The default committed clause inventory file:
/// `<workspace_root>/conformance/inventory/clauses.json` — the machine-owned,
/// byte-stable prose-clause inventory (sibling of `constraints.json`). The CLI's
/// `--clauses <file>` flag overrides it (021-normative-clause-inventory).
pub fn default_clauses_file() -> std::path::PathBuf {
    workspace_root()
        .join("conformance")
        .join("inventory")
        .join("clauses.json")
}

/// Resolve the `(spec_dir, clauses_file)` that belong to a registry, as siblings under
/// the same `conformance/` tree: `<registry>/../spec/<CURRENT_SPEC_PIN>` and
/// `<registry>/../inventory/clauses.json`. Mirrors the schema-inventory sibling
/// resolution `inventory_paths_for` uses in the CLI (021-normative-clause-inventory).
pub fn clause_paths_for(
    registry_dir: &std::path::Path,
) -> (std::path::PathBuf, std::path::PathBuf) {
    let base = registry_dir.parent().unwrap_or(registry_dir);
    let spec_dir = base.join("spec").join(CURRENT_SPEC_PIN);
    let clauses_file = base.join("inventory").join("clauses.json");
    (spec_dir, clauses_file)
}

/// The default migration data directory: `<workspace_root>/conformance/migration`.
/// Holds the machine-owned `baseline.json` and the hand-authored `mapping.json`
/// (023-migrate-parity-to-conformance). The CLI's `--registry <dir>` flag resolves it
/// as a sibling of the registry via [`migration_paths_for`] so tests can point at
/// fixtures.
pub fn default_migration_dir() -> std::path::PathBuf {
    workspace_root().join("conformance").join("migration")
}

/// The default committed baseline file:
/// `<workspace_root>/conformance/migration/baseline.json` — the machine-owned,
/// byte-stable, frozen pre-migration inventory.
pub fn default_baseline_file() -> std::path::PathBuf {
    default_migration_dir().join("baseline.json")
}

/// Resolve the `(baseline_file, mapping_file)` that belong to a registry, as siblings
/// under the same `conformance/` tree: `<registry>/../migration/{baseline,mapping}.json`.
/// Mirrors [`clause_paths_for`] / the schema-inventory sibling resolution.
pub fn migration_paths_for(
    registry_dir: &std::path::Path,
) -> (std::path::PathBuf, std::path::PathBuf) {
    let base = registry_dir.parent().unwrap_or(registry_dir);
    let migration = base.join("migration");
    (
        migration.join("baseline.json"),
        migration.join("mapping.json"),
    )
}

/// The default machine-owned obligation inventory:
/// `<workspace_root>/conformance/obligations/obligations.json` — the sole output of
/// `coverage generate` (024-deterministic-conformance-coverage). The CLI's `--out
/// <file>` flag overrides it; `--registry <dir>` resolves it as a sibling via
/// [`obligations_file_for`] so fixture registries get their own.
pub fn default_obligations_file() -> std::path::PathBuf {
    workspace_root()
        .join("conformance")
        .join("obligations")
        .join("obligations.json")
}

/// Resolve the obligation inventory belonging to a registry, as a sibling under the same
/// `conformance/` tree: `<registry>/../obligations/obligations.json`. Mirrors
/// [`clause_paths_for`] / [`migration_paths_for`] / the schema-inventory sibling
/// resolution, so `--registry <fixture>` picks up the fixture's own obligations rather
/// than the workspace's.
pub fn obligations_file_for(registry_dir: &std::path::Path) -> std::path::PathBuf {
    let base = registry_dir.parent().unwrap_or(registry_dir);
    base.join("obligations").join("obligations.json")
}

/// The default discovery data root: `<workspace_root>/conformance/discovery` — the
/// findings queue, the campaign history, and the real-world corpus manifest
/// (025-exploratory-parity-discovery, research D6).
///
/// **A sibling of `registry/`, deliberately not inside it.** [`load::Registry::load`]
/// enumerates *named* subdirectories under `conformance/registry/` and has no wildcard
/// walk at the registry root, so nothing here can be picked up by the registry loader —
/// not by convention, but because there is no code path that would reach it. That is
/// what makes "an unreviewed finding can never influence a release gate" a property of
/// the directory layout rather than a rule someone must remember.
pub fn default_discovery_dir() -> std::path::PathBuf {
    workspace_root().join("conformance").join("discovery")
}

/// Resolve the discovery data root belonging to a registry, as a sibling under the same
/// `conformance/` tree: `<registry>/../discovery`. Mirrors [`clause_paths_for`] /
/// [`migration_paths_for`] / [`obligations_file_for`], so `--registry <fixture>` picks up
/// the fixture's own discovery root rather than the workspace's.
pub fn discovery_dir_for(registry_dir: &std::path::Path) -> std::path::PathBuf {
    let base = registry_dir.parent().unwrap_or(registry_dir);
    base.join("discovery")
}

/// The discovery-side (**D-class**) domain error taxonomy
/// (025-exploratory-parity-discovery, contracts/findings-queue.md).
///
/// Each variant *is* a violation: [`DiscoveryError::class`] names the D-class it belongs
/// to and [`DiscoveryError::record`] names the offending record, so `discovery check`
/// renders `{class} {record}: {message}` without a parallel violation type. Every
/// message names the cause precisely (constitution IV).
///
/// **Numbered separately from the registry's V-series on purpose.** These are emitted by
/// a different command over a different data root; folding them into V-numbering would
/// imply the registry validator can see the queue, which is exactly what the discovery
/// root's placement exists to prevent (research D6/D11).
///
/// **D3** landed with US5 as [`DiscoveryError::PromotionUnresolved`]; **D4** landed with
/// US7 as [`DiscoveryError::CorpusIntegrity`].
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum DiscoveryError {
    /// **D1** — a record that does not parse, or that parses but is structurally
    /// impossible (empty `witnesses`, a derived id that disagrees with its substance).
    /// `record` is the record id when one could be read, else the file path.
    #[error(
        "malformed discovery record `{record}`: {cause}. Remedy: fix the record — the \
         discovery data root is strict JSON and rejects unknown fields at load."
    )]
    MalformedRecord { record: String, cause: String },

    /// **D1** — a reference that names something absent: a `firstObserved` /
    /// `lastObserved` campaign missing from `campaigns.json`, a witness naming an
    /// unresolvable campaign, or a `splitFrom` naming a finding that is gone.
    ///
    /// An unresolvable **`promotedTo`** is deliberately *not* here: it is
    /// [`DiscoveryError::PromotionUnresolved`] (**D3**), because it is a claim about
    /// *coverage* rather than about provenance, and folding it in would report a
    /// finding that reads as covered while nothing executes it under the same code as a
    /// stale campaign pointer.
    #[error(
        "discovery record `{record}` references {kind} `{reference}`, which does not \
         resolve. Remedy: restore the referenced record or correct the reference — a \
         dangling reference lets the queue claim provenance it does not have."
    )]
    UnresolvableReference {
        record: String,
        kind: String,
        reference: String,
    },

    /// **D1** — a signature naming a channel absent from `channels.json`. The channel
    /// set is closed: a signature over an undeclared channel is a signature nothing
    /// observes.
    #[error(
        "discovery record `{record}` names undeclared channel `{channel}`. Remedy: use \
         one of the channels declared in `conformance/registry/channels.json`, or declare \
         the new channel there first (a new channel also needs a `reg-` regression record)."
    )]
    UnknownChannel { record: String, channel: String },

    /// **D2** — a classification that is absent, present too early, or present where it
    /// cannot lead anywhere.
    ///
    /// Three shapes, all one class because all three are the same defect — the queue
    /// asserting a judgement nobody made, or making a judgement nobody can act on:
    ///
    /// - a finding in `triaged` / `promoted` / `no-longer-reproducing` with **no**
    ///   classification (FR-028's "exactly one" reduced to zero);
    /// - a finding in `untriaged` or `split` **carrying** one (an untriaged finding by
    ///   definition has no judgement, and a split parent surrendered its judgement to its
    ///   children — a parent that kept one would assert exactly what the split rejected);
    /// - a `promoted` finding classified `normalizer-defect` or `fixture-defect`, which
    ///   describe a defect in the discovery machinery rather than a behavior of either
    ///   implementation and are therefore non-promotable (FR-035).
    #[error(
        "discovery finding `{record}` has a classification problem: {cause}. Remedy: \
         record exactly one classification with `discovery triage` once the finding is \
         triaged or later, and none while it is untriaged or a split ancestor — a queue \
         that claims a judgement nobody made is worse than one that admits it has none."
    )]
    ClassificationArity { record: String, cause: String },

    /// **D3** — a promotion the registry cannot back: a `promoted` finding with no
    /// `promotedTo`, one naming a case the registry does not declare, or a `promotedTo`
    /// carried in any state other than `promoted`.
    ///
    /// One class because all three are the same defect — **the queue claiming coverage
    /// that does not exist**. That is worse than an uncovered finding: an uncovered
    /// finding is visible in the untriaged bucket and gets reviewed, whereas a promotion
    /// nothing executes reads as done and is never looked at again (FR-042's whole
    /// purpose is that a promoted finding is not rediscovered and re-triaged).
    ///
    /// Deliberately checked against the *loaded registry* on every run rather than only
    /// at the moment of promotion: a case deleted or renamed afterwards produces exactly
    /// this shape, and nothing in the registry can notice, because the registry never
    /// reads the queue (research D6).
    #[error(
        "discovery finding `{record}` has an unresolvable promotion: {cause}. Remedy: \
         author the case with `discovery scaffold`'s skeleton, commit it, and point \
         `promotedTo` at its real id — discovery never writes a case, so a promotion the \
         registry cannot back is a claim nothing executes."
    )]
    PromotionUnresolved { record: String, cause: String },

    /// **D4** — a corpus entry that does not name a retrievable, verifiable snapshot: a
    /// `commit` that is not a 40-hex object name (a branch, a tag, `HEAD`, `latest`, an
    /// abbreviated SHA), a malformed `contentDigest`, an id that does not derive from the
    /// entry's own substance, a duplicate id or name, or a digest that was recorded and
    /// then removed.
    ///
    /// The first four are answerable from the manifest alone and are checked
    /// hermetically on every pull request — which is the entire reason the manifest is
    /// Rust-owned strict JSON rather than a Python tuple (research D8). A validation that
    /// only runs when the network is up is a validation that does not run.
    #[error(
        "corpus entry `{record}`: {cause}. Remedy: pin the entry to a 40-hex commit and \
         leave `contentDigest` to the fetch — the manifest records provenance, and a \
         provenance record that names moving content proves nothing about what was \
         compared."
    )]
    CorpusIntegrity { record: String, cause: String },

    /// **D5** — a pinned-input-set element naming a revision absent from
    /// `revisions.json`. A finding is a claim about a specific pinned pair of
    /// implementations; a pin nothing records is a claim nothing can be checked against.
    #[error(
        "discovery record `{record}`: pinned input `{element}` names revision `{value}`, \
         which is absent from `conformance/registry/revisions.json`. Remedy: record the \
         revision, or re-evaluate the finding under the current pins — a finding is never \
         carried forward across a pin change unverified."
    )]
    StalePin {
        record: String,
        element: String,
        value: String,
    },
}

impl DiscoveryError {
    /// The D-class this error belongs to (`"D1"` … `"D5"`).
    pub fn class(&self) -> &'static str {
        match self {
            DiscoveryError::MalformedRecord { .. }
            | DiscoveryError::UnresolvableReference { .. }
            | DiscoveryError::UnknownChannel { .. } => "D1",
            DiscoveryError::ClassificationArity { .. } => "D2",
            DiscoveryError::PromotionUnresolved { .. } => "D3",
            DiscoveryError::CorpusIntegrity { .. } => "D4",
            DiscoveryError::StalePin { .. } => "D5",
        }
    }

    /// The offending record's id (or the file path, when the record id could not be
    /// read because the file itself did not parse).
    pub fn record(&self) -> &str {
        match self {
            DiscoveryError::MalformedRecord { record, .. }
            | DiscoveryError::UnresolvableReference { record, .. }
            | DiscoveryError::UnknownChannel { record, .. }
            | DiscoveryError::ClassificationArity { record, .. }
            | DiscoveryError::PromotionUnresolved { record, .. }
            | DiscoveryError::CorpusIntegrity { record, .. }
            | DiscoveryError::StalePin { record, .. } => record,
        }
    }
}

/// Atomically write `contents` to `path` (unique temp file + `fs::rename`), creating
/// the parent directory if needed. Never leaves a partial file, and a shorter payload
/// over a longer one can never leave trailing bytes (the failure mode a plain
/// `fs::write` has — see `crates/core/src/cache/disk.rs::save_index`).
///
/// This is the SINGLE atomic-write primitive for every machine-owned artifact this
/// crate emits (`constraints.json`, `clauses.json`, `baseline.json`); the per-artifact
/// `write_*` helpers render their canonical string form and delegate here rather than
/// re-implementing the temp-file dance.
pub fn atomic_write(path: &std::path::Path, contents: &str) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let parent = path.parent().unwrap_or_else(|| std::path::Path::new("."));
    let file_name = path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("artifact.json");

    static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let seq = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let tmp = parent.join(format!("{file_name}.tmp.{}.{}", std::process::id(), seq));

    std::fs::write(&tmp, contents)?;
    std::fs::rename(&tmp, path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn workspace_root_locates_the_crate() {
        let root = workspace_root();
        assert!(
            root.join("crates/conformance/Cargo.toml").is_file(),
            "workspace_root() should locate this crate, got {root:?}"
        );
    }

    #[test]
    fn default_schemas_dir_contains_the_vendored_pin() {
        // The vendored pinned schemas + manifest live under the schemas root, keyed
        // by the `rev-schema-113500f4` pin (020-schema-constraint-inventory).
        let manifest = default_schemas_dir().join("113500f4").join("manifest.json");
        assert!(
            manifest.is_file(),
            "default_schemas_dir() should contain the vendored manifest, got {manifest:?}"
        );
    }

    #[test]
    fn default_inventory_file_path_is_stable() {
        let inv = default_inventory_file();
        assert!(
            inv.ends_with("conformance/inventory/constraints.json"),
            "got {inv:?}"
        );
    }
}
