//! Shared fixture builder for the User Story 5 conservation tests.
//!
//! Every conservation test asks the same question — "what does the report say when ONE
//! thing about the real registry changes?" — so they all need a mutable copy of the real
//! registry plus its `migration/` sibling. Building that copy once here keeps each test
//! about its own mutation instead of about file plumbing, and keeps the mutations
//! honest: they modify the REAL committed data, so a test cannot pass against a
//! convenient synthetic registry that no longer resembles what ships.

#![allow(dead_code)]

use std::path::{Path, PathBuf};

use deacon_conformance::baseline::BaselineUnit;
use deacon_conformance::conservation::{
    EquivalenceFacts, MigrationReport, ReportError, migration_report,
};
use deacon_conformance::load::Registry;
use deacon_conformance::{default_registry_dir, workspace_root};

/// A writable copy of the real `conformance/` tree, rooted in a tempdir.
pub struct Fixture {
    dir: tempfile::TempDir,
}

impl Fixture {
    /// Copy the real registry, its `migration/` sibling and its `fixtures/` sibling.
    pub fn real() -> Fixture {
        let dir = tempfile::tempdir().expect("tempdir");
        let src = workspace_root().join("conformance");
        let dst = dir.path().join("conformance");
        for sub in ["registry", "migration", "fixtures"] {
            let from = src.join(sub);
            if from.is_dir() {
                copy_dir(&from, &dst.join(sub));
            }
        }
        assert!(
            dst.join("registry").join("cases").is_dir(),
            "the fixture must contain the real registry"
        );
        Fixture { dir }
    }

    /// The copied registry directory.
    pub fn registry_dir(&self) -> PathBuf {
        self.dir.path().join("conformance").join("registry")
    }

    fn migration_dir(&self) -> PathBuf {
        self.dir.path().join("conformance").join("migration")
    }

    fn fixtures_dir(&self) -> PathBuf {
        self.dir.path().join("conformance").join("fixtures")
    }

    /// Load the copied registry.
    pub fn registry(&self) -> Registry {
        Registry::load(&self.registry_dir()).expect("the fixture registry loads")
    }

    /// Compute the report over the copied registry, with no equivalence ledger.
    pub fn report(&self) -> Result<MigrationReport, ReportError> {
        migration_report(&self.registry(), &self.fixtures_dir(), None)
    }

    /// Compute the report with an equivalence ledger.
    pub fn report_with(&self, facts: &EquivalenceFacts) -> Result<MigrationReport, ReportError> {
        migration_report(&self.registry(), &self.fixtures_dir(), Some(facts))
    }

    /// A unit from the copied baseline.
    pub fn baseline_unit(&self, id: &str) -> Option<BaselineUnit> {
        self.registry()
            .baseline
            .and_then(|b| b.records.into_iter().find(|u| u.id == id))
    }

    /// Drop one mapping entry, leaving its baseline unit orphaned.
    pub fn without_mapping_entry(self, unit: &str) -> Fixture {
        self.edit_json(&self.migration_dir().join("mapping.json"), |doc| {
            if let Some(records) = doc.get_mut("records").and_then(|v| v.as_array_mut()) {
                records.retain(|r| r.get("unit").and_then(|v| v.as_str()) != Some(unit));
            }
        });
        self
    }

    /// Mutate one mapping entry in place.
    pub fn edit_mapping_entry(self, unit: &str, edit: impl Fn(&mut serde_json::Value)) -> Fixture {
        self.edit_json(&self.migration_dir().join("mapping.json"), |doc| {
            if let Some(records) = doc.get_mut("records").and_then(|v| v.as_array_mut()) {
                for record in records.iter_mut() {
                    if record.get("unit").and_then(|v| v.as_str()) == Some(unit) {
                        edit(record);
                    }
                }
            }
        });
        self
    }

    /// Every per-area case file, sorted (024 T007: cases live in `cases/<area>.json`).
    fn case_files(&self) -> Vec<PathBuf> {
        let dir = self.registry_dir().join("cases");
        let mut files: Vec<PathBuf> = std::fs::read_dir(&dir)
            .unwrap_or_else(|e| panic!("read {}: {e}", dir.display()))
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.is_file() && p.extension().and_then(|s| s.to_str()) == Some("json"))
            .collect();
        files.sort();
        files
    }

    /// Drop one case from whichever `cases/<area>.json` holds it.
    pub fn without_case(self, case_id: &str) -> Fixture {
        for file in self.case_files() {
            self.edit_json(&file, |doc| {
                if let Some(records) = doc.get_mut("records").and_then(|v| v.as_array_mut()) {
                    records.retain(|r| r.get("id").and_then(|v| v.as_str()) != Some(case_id));
                }
            });
        }
        self
    }

    /// Mutate one case in place, in whichever `cases/<area>.json` holds it.
    pub fn edit_case(self, case_id: &str, edit: impl Fn(&mut serde_json::Value)) -> Fixture {
        for file in self.case_files() {
            self.edit_json(&file, |doc| {
                if let Some(records) = doc.get_mut("records").and_then(|v| v.as_array_mut()) {
                    for record in records.iter_mut() {
                        if record.get("id").and_then(|v| v.as_str()) == Some(case_id) {
                            edit(record);
                        }
                    }
                }
            });
        }
        self
    }

    /// Mutate the committed baseline in place — the anti-gaming lever (T069).
    pub fn edit_baseline(self, edit: impl Fn(&mut serde_json::Value)) -> Fixture {
        self.edit_json(&self.migration_dir().join("baseline.json"), edit);
        self
    }

    /// Remove the committed baseline entirely.
    pub fn without_baseline(self) -> Fixture {
        let _ = std::fs::remove_file(self.migration_dir().join("baseline.json"));
        self
    }

    fn edit_json(&self, path: &Path, edit: impl FnOnce(&mut serde_json::Value)) {
        let raw = std::fs::read_to_string(path)
            .unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
        let mut doc: serde_json::Value =
            serde_json::from_str(&raw).unwrap_or_else(|e| panic!("parse {}: {e}", path.display()));
        edit(&mut doc);
        let rendered = serde_json::to_string_pretty(&doc).expect("render") + "\n";
        std::fs::write(path, rendered).unwrap_or_else(|e| panic!("write {}: {e}", path.display()));
    }
}

/// The real registry directory, for tests that only read.
pub fn real_registry_dir() -> PathBuf {
    default_registry_dir()
}

fn copy_dir(src: &Path, dst: &Path) {
    std::fs::create_dir_all(dst).expect("create dir");
    for entry in std::fs::read_dir(src).expect("read dir").flatten() {
        let path = entry.path();
        let target = dst.join(entry.file_name());
        if path.is_dir() {
            copy_dir(&path, &target);
        } else {
            std::fs::copy(&path, &target).expect("copy file");
        }
    }
}
