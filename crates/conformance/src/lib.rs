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
