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
//! - [`certify`] — strict certification for the active profile (US2, T019).

pub mod certify;
pub mod coverage;
pub mod load;
pub mod model;
pub mod report;
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
}
