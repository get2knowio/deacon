//! The `conformance` binary (dev-only): `validate` / `report` / `certify`.
//!
//! Contributor tooling for the repository-owned conformance registry, invoked as
//! `cargo run -p deacon-conformance -- <subcommand>` (contracts/cli.md). NOT part
//! of the `deacon` consumer CLI surface (constitution II).
//!
//! `validate` runs the full violation-class engine (V1–V10 + SCHEMA) via
//! [`validate_path`], emitting one-violation-per-line text or a single `--json`
//! document (contracts/cli.md); `report` writes the deterministic
//! `report.json`/`report.md` pair (running validation first), and `certify`
//! evaluates the strict release gate. `anyhow` is used only here at the binary
//! boundary (constitution V).

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use anyhow::Context;
use clap::{Parser, Subcommand, ValueEnum};

use deacon_conformance::certify::certify;
use deacon_conformance::clause::{generate_clauses, render as render_clauses, write_clauses};
use deacon_conformance::clause_diff::{
    diff as clause_diff, render_json as render_clause_diff_json, render_md as render_clause_diff_md,
};
use deacon_conformance::diff::{
    diff, render_json as render_diff_json, render_md as render_diff_md,
};
use deacon_conformance::inventory::{
    InventoryDrift, compare, generate_inventory, render, write_inventory,
};
use deacon_conformance::load::{
    LoadError, Registry, load_clause_inventory, load_inventory, load_spec_manifest,
};
use deacon_conformance::model::{ClauseInventory, ConstraintInventory, DocumentScope};
use deacon_conformance::report::write_reports;
use deacon_conformance::validate::{
    ClauseInputs, InventoryInputs, Violation, validate_path, validate_path_with_inventory,
};
use deacon_conformance::{
    CURRENT_SCHEMA_PIN, clause_paths_for, default_clauses_file, default_inventory_file,
    default_pinned_schemas_dir, default_pinned_spec_dir, default_registry_dir, workspace_root,
};

/// Structural conformance-registry tooling (dev-only).
#[derive(Debug, Parser)]
#[command(
    name = "conformance",
    about = "Validate, report on, and certify the repository-owned conformance registry",
    version
)]
struct Cli {
    /// Registry root directory. Defaults to `<workspace>/conformance/registry`;
    /// tests point it at fixtures under `fixtures/conformance/`.
    #[arg(long, value_name = "DIR", global = true)]
    registry: Option<PathBuf>,

    /// Injected "today" (`YYYY-MM-DD`) for deterministic waiver-expiry evaluation.
    /// Defaults to the current UTC calendar date.
    #[arg(long, value_name = "YYYY-MM-DD", global = true)]
    today: Option<String>,

    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Structural validation (violation classes V1–V10 + SCHEMA).
    Validate {
        /// Emit a single JSON document (`{ "ok", "violations" }`) on stdout instead
        /// of one violation per line; logs still go to stderr (contracts/cli.md).
        #[arg(long)]
        json: bool,
    },
    /// Generate the deterministic coverage report (`report.json` + `report.md`).
    Report {
        /// Directory to write `report.json` and `report.md` into. Defaults to
        /// `<workspace>/target/conformance/` (research Decision 7).
        #[arg(long, value_name = "DIR")]
        out_dir: Option<PathBuf>,
    },
    /// Strict certification for the active profile (release gate).
    Certify {
        /// Emit a single JSON document
        /// (`{ "certified", "profile", "blocking", "waived" }`) on stdout instead of
        /// the human-readable summary; logs still go to stderr (contracts/cli.md).
        #[arg(long)]
        json: bool,
    },
    /// Schema constraint inventory tooling (020-schema-constraint-inventory).
    Inventory {
        #[command(subcommand)]
        command: InventoryCommand,
    },
    /// Normative clause inventory tooling (021-normative-clause-inventory).
    Clause {
        #[command(subcommand)]
        command: ClauseCommand,
    },
}

/// `clause <generate|check|diff|scaffold>` — machine-owned prose-clause inventory
/// operations (contracts/cli-clause.md). NEVER performs network IO and NEVER invokes
/// an LLM — pure functions of the committed records and the vendored pinned prose.
#[derive(Debug, Subcommand)]
enum ClauseCommand {
    /// Canonicalize the committed clause records against the vendored pinned prose
    /// (recomputes ids/fingerprints, verifies each excerpt is present at its heading).
    Generate {
        /// Pinned-spec directory (holds `manifest.json` + the vendored Markdown files).
        /// Defaults to `<workspace>/conformance/spec/<pin>/`.
        #[arg(long, value_name = "DIR")]
        spec: Option<PathBuf>,
        /// Output clause-inventory file. Defaults to
        /// `<workspace>/conformance/inventory/clauses.json`.
        #[arg(long, value_name = "FILE")]
        clauses: Option<PathBuf>,
    },
    /// Regenerate in memory and byte-compare against the committed clause inventory.
    Check {
        /// Pinned-spec directory (see `generate`).
        #[arg(long, value_name = "DIR")]
        spec: Option<PathBuf>,
        /// Committed clause-inventory file to compare against (see `generate --clauses`).
        #[arg(long, value_name = "FILE")]
        clauses: Option<PathBuf>,
    },
    /// Deterministically diff two clause-inventory files (data-model §4, match key:
    /// substance-anchored id): new / removed / moved / changed / non-material.
    Diff {
        /// The old (left) clause-inventory file.
        #[arg(value_name = "OLD")]
        old: PathBuf,
        /// The new (right) clause-inventory file.
        #[arg(value_name = "NEW")]
        new: PathBuf,
        /// Output format. Defaults to `json`; `md` renders the human review document.
        #[arg(long, value_name = "FORMAT", default_value = "json")]
        format: DiffFormat,
        /// Write the diff to a file instead of stdout.
        #[arg(long, value_name = "FILE")]
        out: Option<PathBuf>,
    },
    /// Emit skeleton `clc-` records (stdout only) for every currently unclassified
    /// clause, with the sentinel `disposition: "UNREVIEWED"` (loader-rejected). Per-clause
    /// skeletons for consumer/ambiguous clauses; one per-document skeleton for authoring
    /// documents. Never writes into the registry.
    Scaffold {
        /// Committed clause-inventory file to scaffold from (see `generate --clauses`).
        #[arg(long, value_name = "FILE")]
        clauses: Option<PathBuf>,
        /// Pinned-spec directory (for per-document `scope`). Defaults to the pinned dir.
        #[arg(long, value_name = "DIR")]
        spec: Option<PathBuf>,
    },
}

/// `inventory <generate|check|diff|scaffold>` — machine-owned constraint inventory
/// operations (contracts/cli-inventory.md). NEVER performs network IO.
#[derive(Debug, Subcommand)]
enum InventoryCommand {
    /// Extract the vendored pinned schemas into the canonical committed inventory.
    Generate {
        /// Manifest directory (holds `manifest.json` + the vendored schema files).
        /// Defaults to `<workspace>/conformance/schemas/<pin>/`.
        #[arg(long, value_name = "DIR")]
        schemas: Option<PathBuf>,
        /// Output inventory file. Defaults to
        /// `<workspace>/conformance/inventory/constraints.json`.
        #[arg(long, value_name = "FILE")]
        out: Option<PathBuf>,
    },
    /// Regenerate in memory and byte-compare against the committed inventory.
    Check {
        /// Manifest directory (see `generate`).
        #[arg(long, value_name = "DIR")]
        schemas: Option<PathBuf>,
        /// Committed inventory file to compare against (see `generate --out`).
        #[arg(long, value_name = "FILE")]
        inventory: Option<PathBuf>,
    },
    /// Deterministically diff two inventory files (data-model §4, match key
    /// `(document, pointer, kind)`): added / removed / materially changed /
    /// non-material (annotation-kind) differences. Reads two arbitrary inventory
    /// files from disk; NEVER performs network IO.
    Diff {
        /// The old (left) inventory file.
        #[arg(value_name = "OLD")]
        old: PathBuf,
        /// The new (right) inventory file.
        #[arg(value_name = "NEW")]
        new: PathBuf,
        /// Output format. Defaults to `json`; `md` renders the human review document.
        #[arg(long, value_name = "FORMAT", default_value = "json")]
        format: DiffFormat,
        /// Write the diff to a file instead of stdout.
        #[arg(long, value_name = "FILE")]
        out: Option<PathBuf>,
    },
    /// Emit skeleton `cls-` records (stdout only) for every currently unclassified
    /// constraint unit. Each carries the sentinel `disposition: "UNREVIEWED"` — a
    /// value the loader REJECTS — so scaffolded output cannot be committed unedited.
    /// Never writes into the registry. The registry root is the global `--registry`.
    Scaffold {
        /// Committed inventory file to scaffold from (see `generate --out`).
        #[arg(long, value_name = "FILE")]
        inventory: Option<PathBuf>,
    },
}

/// The `inventory diff` output format (contracts/cli-inventory.md).
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum DiffFormat {
    /// Canonical machine-readable JSON (default).
    Json,
    /// Human-review Markdown.
    Md,
}

fn main() {
    let cli = Cli::parse();
    std::process::exit(run(cli));
}

/// Dispatch, returning the process exit code (contracts/cli.md: 0 ok, 1 violations,
/// 2 usage/IO error).
fn run(cli: Cli) -> i32 {
    let registry_dir = cli.registry.unwrap_or_else(default_registry_dir);

    // Resolving `today` also validates the `--today` format up front (used by
    // waiver-expiry evaluation in a later phase).
    let today = match resolve_today(cli.today.as_deref()) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("error: {e:#}");
            return 2;
        }
    };

    match cli.command {
        Command::Validate { json } => validate(&registry_dir, &today, json),
        Command::Report { out_dir } => report(&registry_dir, &today, out_dir),
        Command::Certify { json } => certify_cmd(&registry_dir, &today, json),
        Command::Inventory { command } => match command {
            InventoryCommand::Generate { schemas, out } => inventory_generate(schemas, out),
            InventoryCommand::Check { schemas, inventory } => inventory_check(schemas, inventory),
            InventoryCommand::Diff {
                old,
                new,
                format,
                out,
            } => inventory_diff(&old, &new, format, out.as_deref()),
            InventoryCommand::Scaffold { inventory } => {
                inventory_scaffold(&registry_dir, inventory)
            }
        },
        Command::Clause { command } => match command {
            ClauseCommand::Generate { spec, clauses } => clause_generate(spec, clauses),
            ClauseCommand::Check { spec, clauses } => clause_check(spec, clauses),
            ClauseCommand::Diff {
                old,
                new,
                format,
                out,
            } => clause_diff_cmd(&old, &new, format, out.as_deref()),
            ClauseCommand::Scaffold { clauses, spec } => {
                clause_scaffold(&registry_dir, clauses, spec)
            }
        },
    }
}

/// `clause generate` (contracts/cli-clause.md): fingerprint-verify the spec manifest,
/// canonicalize the committed clause records against the vendored prose, and write the
/// byte-stable inventory atomically. Exit `0` on success, `1` on any integrity error
/// (never a partial file), `2` on a write IO failure.
fn clause_generate(spec: Option<PathBuf>, clauses: Option<PathBuf>) -> i32 {
    let spec_dir = spec.unwrap_or_else(default_pinned_spec_dir);
    let out_file = clauses.unwrap_or_else(default_clauses_file);

    let inventory = match generate_clauses(&spec_dir, &out_file) {
        Ok(inv) => inv,
        Err(e) => {
            eprintln!("error: clause generation failed: {e}");
            return 1;
        }
    };
    match write_clauses(&out_file, &inventory) {
        Ok(()) => {
            println!("{}", out_file.display());
            eprintln!(
                "wrote {} clause unit(s) to {}",
                inventory.units.len(),
                out_file.display()
            );
            0
        }
        Err(e) => {
            eprintln!(
                "error: could not write clauses to {}: {e}",
                out_file.display()
            );
            2
        }
    }
}

/// `clause check` (contracts/cli-clause.md): regenerate in memory and byte-compare
/// against the committed clause inventory. Exit `0` if identical, `1` if it differs or
/// on any generate-class error, `2` if the committed file is unreadable.
fn clause_check(spec: Option<PathBuf>, clauses: Option<PathBuf>) -> i32 {
    let spec_dir = spec.unwrap_or_else(default_pinned_spec_dir);
    let clauses_file = clauses.unwrap_or_else(default_clauses_file);

    let regenerated = match generate_clauses(&spec_dir, &clauses_file) {
        Ok(inv) => inv,
        Err(e) => {
            eprintln!("error: clause regeneration failed: {e}");
            return 1;
        }
    };
    let committed_raw = match std::fs::read_to_string(&clauses_file) {
        Ok(raw) => raw,
        Err(e) => {
            eprintln!(
                "error: could not read committed clause inventory {}: {e}",
                clauses_file.display()
            );
            return 2;
        }
    };

    if committed_raw == render_clauses(&regenerated) {
        eprintln!("ok: {} matches regeneration", clauses_file.display());
        return 0;
    }

    // Compute a compact new/removed/moved/changed summary for diagnosis.
    match serde_json::from_str::<ClauseInventory>(&committed_raw) {
        Ok(committed) => {
            let d = clause_diff(&committed, &regenerated);
            eprintln!(
                "error: committed clause inventory {} is out of date (new {}, removed {}, moved {}, \
                 changed {}, non-material {})",
                clauses_file.display(),
                d.new_clauses.len(),
                d.removed.len(),
                d.moved.len(),
                d.changed.len(),
                d.non_material.len(),
            );
            for e in &d.new_clauses {
                eprintln!("  + {}", e.id);
            }
            for e in &d.removed {
                eprintln!("  - {}", e.id);
            }
            for e in &d.changed {
                eprintln!("  ~ {} -> {}", e.old_id, e.new_id);
            }
        }
        Err(e) => eprintln!(
            "error: committed clause inventory is out of date and unparseable: {}: {e}",
            clauses_file.display()
        ),
    }
    1
}

/// `clause diff <old> <new>` (contracts/cli-clause.md): load two clause-inventory files,
/// compute the deterministic revision diff, and write it to stdout or `--out`. Exit `0`
/// on success (incl. an empty diff), `1` on unreadable/malformed input, `2` on `--out`
/// write failure. NEVER performs network IO.
fn clause_diff_cmd(old: &Path, new: &Path, format: DiffFormat, out: Option<&Path>) -> i32 {
    let old_inv = match load_clause_diff_input(old) {
        Ok(inv) => inv,
        Err(e) => {
            eprintln!("error: {e}");
            return 1;
        }
    };
    let new_inv = match load_clause_diff_input(new) {
        Ok(inv) => inv,
        Err(e) => {
            eprintln!("error: {e}");
            return 1;
        }
    };

    let result = clause_diff(&old_inv, &new_inv);
    let rendered = match format {
        DiffFormat::Json => render_clause_diff_json(&result),
        DiffFormat::Md => render_clause_diff_md(&result),
    };
    let summary = format!(
        "new {}, removed {}, moved {}, changed {}, non-material {}",
        result.new_clauses.len(),
        result.removed.len(),
        result.moved.len(),
        result.changed.len(),
        result.non_material.len(),
    );

    match out {
        Some(path) => match std::fs::write(path, &rendered) {
            Ok(()) => {
                println!("{}", path.display());
                eprintln!("wrote clause diff to {} ({summary})", path.display());
                0
            }
            Err(e) => {
                eprintln!(
                    "error: could not write clause diff to {}: {e}",
                    path.display()
                );
                2
            }
        },
        None => {
            print!("{rendered}");
            eprintln!("clause diff: {summary}");
            0
        }
    }
}

/// Read one `clause diff` input file into a [`ClauseInventory`] (a missing file is a hard
/// error — the diff has two required positional inputs).
fn load_clause_diff_input(path: &Path) -> Result<ClauseInventory, String> {
    let raw = std::fs::read_to_string(path)
        .map_err(|e| format!("could not read clause inventory {}: {e}", path.display()))?;
    serde_json::from_str::<ClauseInventory>(&raw)
        .map_err(|e| format!("could not parse clause inventory {}: {e}", path.display()))
}

/// `clause scaffold` (contracts/cli-clause.md): emit skeleton `clc-` records to stdout for
/// every currently unclassified clause. Per-clause skeletons for consumer/ambiguous
/// clauses; ONE per-document skeleton for an authoring document that has no covering
/// document-scope record yet. Each carries the sentinel `disposition: "UNREVIEWED"` — a
/// value the loader rejects. Never writes into the registry.
///
/// Exit `0` on success (possibly zero skeletons); `1` if the inventory/registry/manifest
/// is unreadable.
fn clause_scaffold(registry_dir: &Path, clauses: Option<PathBuf>, spec: Option<PathBuf>) -> i32 {
    let (default_spec, default_clauses) = clause_paths_for(registry_dir);
    let clauses_file = clauses.unwrap_or(default_clauses);
    let spec_dir = spec.unwrap_or(default_spec);

    let committed = match load_clause_inventory(&clauses_file) {
        Ok(Some(inv)) => inv,
        Ok(None) => {
            eprintln!(
                "error: committed clause inventory {} does not exist",
                clauses_file.display()
            );
            return 1;
        }
        Err(e) => {
            eprintln!(
                "error: could not read committed clause inventory {}: {e}",
                clauses_file.display()
            );
            return 1;
        }
    };
    let manifest = match load_spec_manifest(&spec_dir) {
        Ok(m) => m,
        Err(e) => {
            eprintln!(
                "error: could not read spec manifest {}: {e}",
                spec_dir.display()
            );
            return 1;
        }
    };
    let scope: HashSet<&str> = manifest
        .documents
        .iter()
        .filter(|d| d.scope == DocumentScope::Authoring)
        .map(|d| d.key.as_str())
        .collect();
    let registry = match Registry::load(registry_dir) {
        Ok(r) => r,
        Err(e) => {
            eprintln!(
                "error: could not read registry {}: {e}",
                registry_dir.display()
            );
            return 1;
        }
    };
    // Already-covered: per-clause classifications by clause id; document defaults by key.
    let classified_clauses: HashSet<&str> = registry
        .clause_classifications
        .iter()
        .filter_map(|c| c.clause.as_deref())
        .collect();
    let classified_docs: HashSet<&str> = registry
        .clause_classifications
        .iter()
        .filter_map(|c| c.document.as_deref())
        .collect();

    use deacon_conformance::model::Testability;
    let mut skeletons: Vec<ClauseScaffoldRecord> = Vec::new();
    let mut emitted_doc_defaults: HashSet<String> = HashSet::new();
    for unit in &committed.units {
        if classified_clauses.contains(unit.id.as_str()) {
            continue;
        }
        let authoring = scope.contains(unit.document.as_str());
        let ambiguous = unit.testability == Testability::Ambiguous;
        // Authoring, non-ambiguous clauses are covered by a per-document default.
        if authoring && !ambiguous {
            if classified_docs.contains(unit.document.as_str())
                || emitted_doc_defaults.contains(&unit.document)
            {
                continue;
            }
            emitted_doc_defaults.insert(unit.document.clone());
            skeletons.push(ClauseScaffoldRecord::for_document(&unit.document));
        } else {
            skeletons.push(ClauseScaffoldRecord::for_clause(&unit.id));
        }
    }

    match serde_json::to_string_pretty(&skeletons) {
        Ok(doc) => println!("{doc}"),
        Err(e) => {
            eprintln!("error: could not serialize scaffold records: {e}");
            return 1;
        }
    }
    eprintln!(
        "emitted {} skeleton clause-classification record(s) for {} (sentinel disposition \
         \"UNREVIEWED\" — edit before committing)",
        skeletons.len(),
        clauses_file.display()
    );
    0
}

/// A skeleton clause-classification record emitted by `clause scaffold` (sentinel
/// `disposition: "UNREVIEWED"`, loader-rejected). Not the typed `ClauseClassification`.
#[derive(Debug, serde::Serialize)]
struct ClauseScaffoldRecord {
    id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    clause: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    document: Option<String>,
    disposition: &'static str,
    behaviors: Vec<String>,
    rationale: Option<String>,
    notes: Option<String>,
}

impl ClauseScaffoldRecord {
    const SENTINEL: &'static str = "UNREVIEWED";

    fn for_clause(clause_id: &str) -> ClauseScaffoldRecord {
        let tail = clause_id.strip_prefix("clu-").unwrap_or(clause_id);
        ClauseScaffoldRecord {
            id: format!("clc-{tail}"),
            clause: Some(clause_id.to_string()),
            document: None,
            disposition: ClauseScaffoldRecord::SENTINEL,
            behaviors: Vec::new(),
            rationale: None,
            notes: None,
        }
    }

    fn for_document(document: &str) -> ClauseScaffoldRecord {
        ClauseScaffoldRecord {
            id: format!("clc-doc-{document}"),
            clause: None,
            document: Some(document.to_string()),
            disposition: ClauseScaffoldRecord::SENTINEL,
            behaviors: Vec::new(),
            rationale: None,
            notes: None,
        }
    }
}

/// `inventory generate` (contracts/cli-inventory.md): load + fingerprint-verify the
/// manifest, extract, and write the canonical inventory atomically. Exit `0` on
/// success, `1` on any extraction/verification error (never a partial file), `2` on a
/// write IO failure.
fn inventory_generate(schemas: Option<PathBuf>, out: Option<PathBuf>) -> i32 {
    let schemas_dir = schemas.unwrap_or_else(default_pinned_schemas_dir);
    let out_file = out.unwrap_or_else(default_inventory_file);

    let inventory = match generate_inventory(&schemas_dir) {
        Ok(inv) => inv,
        Err(e) => {
            eprintln!("error: inventory generation failed: {e}");
            return 1;
        }
    };
    match write_inventory(&out_file, &inventory) {
        Ok(()) => {
            println!("{}", out_file.display());
            eprintln!(
                "wrote {} constraint unit(s) to {}",
                inventory.units.len(),
                out_file.display()
            );
            0
        }
        Err(e) => {
            eprintln!(
                "error: could not write inventory to {}: {e}",
                out_file.display()
            );
            2
        }
    }
}

/// `inventory check` (contracts/cli-inventory.md): regenerate in memory and byte-compare
/// against the committed inventory. Exit `0` if identical, `1` if it differs
/// (`InventoryOutOfDate`, with a compact added/removed/changed summary) or on any
/// generate-class error, `2` if the committed file is unreadable.
fn inventory_check(schemas: Option<PathBuf>, inventory: Option<PathBuf>) -> i32 {
    let schemas_dir = schemas.unwrap_or_else(default_pinned_schemas_dir);
    let inventory_file = inventory.unwrap_or_else(default_inventory_file);

    let regenerated = match generate_inventory(&schemas_dir) {
        Ok(inv) => inv,
        Err(e) => {
            eprintln!("error: inventory regeneration failed: {e}");
            return 1;
        }
    };

    let committed_raw = match std::fs::read_to_string(&inventory_file) {
        Ok(raw) => raw,
        Err(e) => {
            eprintln!(
                "error: could not read committed inventory {}: {e}",
                inventory_file.display()
            );
            return 2;
        }
    };

    // Byte comparison is the contract; the unit-level summary is diagnostic only.
    if committed_raw == render(&regenerated) {
        eprintln!("ok: {} matches regeneration", inventory_file.display());
        return 0;
    }

    let committed = match serde_json::from_str::<deacon_conformance::model::ConstraintInventory>(
        &committed_raw,
    ) {
        Ok(inv) => inv,
        Err(e) => {
            // The committed file differs AND does not parse — still out of date; report
            // the parse cause so the mismatch is diagnosable.
            eprintln!(
                "error: committed inventory is out of date and unparseable: {}: {e}",
                inventory_file.display()
            );
            return 1;
        }
    };
    let drift = compare(&committed, &regenerated);
    report_drift(&inventory_file, &drift);
    1
}

/// `inventory diff <old> <new>` (contracts/cli-inventory.md): load two arbitrary
/// inventory files from disk, compute the deterministic revision diff (match key
/// `(document, pointer, kind)`, data-model §4), and write it to stdout or `--out`.
///
/// Exit `0` on success — including an empty diff (two identical inventories is a valid,
/// boring diff). Exit `1` if either input is unreadable or fails to parse as a
/// `ConstraintInventory`. Exit `2` on a `--out` write IO failure. NEVER performs
/// network IO.
fn inventory_diff(old: &Path, new: &Path, format: DiffFormat, out: Option<&Path>) -> i32 {
    let old_inv = match load_diff_input(old) {
        Ok(inv) => inv,
        Err(e) => {
            eprintln!("error: {e}");
            return 1;
        }
    };
    let new_inv = match load_diff_input(new) {
        Ok(inv) => inv,
        Err(e) => {
            eprintln!("error: {e}");
            return 1;
        }
    };

    let result = diff(&old_inv, &new_inv);
    let rendered = match format {
        DiffFormat::Json => render_diff_json(&result),
        DiffFormat::Md => render_diff_md(&result),
    };

    match out {
        Some(path) => match std::fs::write(path, &rendered) {
            Ok(()) => {
                println!("{}", path.display());
                eprintln!(
                    "wrote diff to {} (added {}, removed {}, changed {}, non-material {})",
                    path.display(),
                    result.added.len(),
                    result.removed.len(),
                    result.changed.len(),
                    result.non_material.len(),
                );
                0
            }
            Err(e) => {
                eprintln!("error: could not write diff to {}: {e}", path.display());
                2
            }
        },
        None => {
            print!("{rendered}");
            eprintln!(
                "diff: added {}, removed {}, changed {}, non-material {}",
                result.added.len(),
                result.removed.len(),
                result.changed.len(),
                result.non_material.len(),
            );
            0
        }
    }
}

/// Read one `inventory diff` input file into a [`ConstraintInventory`]. Unlike
/// `load_inventory`, a missing file is a hard error (the diff has two required
/// positional inputs, not the registry-relative default). Returns a human-readable
/// error string on any unreadable / malformed input (mapped to exit 1 by the caller).
fn load_diff_input(path: &Path) -> Result<ConstraintInventory, String> {
    let raw = std::fs::read_to_string(path)
        .map_err(|e| format!("could not read inventory {}: {e}", path.display()))?;
    serde_json::from_str::<ConstraintInventory>(&raw)
        .map_err(|e| format!("could not parse inventory {}: {e}", path.display()))
}

/// Print the compact `inventory check` drift summary on stderr (added/removed/changed
/// unit IDs).
fn report_drift(inventory_file: &Path, drift: &InventoryDrift) {
    eprintln!(
        "error: committed inventory {} is out of date (added {}, removed {}, changed {})",
        inventory_file.display(),
        drift.added.len(),
        drift.removed.len(),
        drift.changed.len()
    );
    for id in &drift.added {
        eprintln!("  + {id}");
    }
    for id in &drift.removed {
        eprintln!("  - {id}");
    }
    for (old, new) in &drift.changed {
        eprintln!("  ~ {old} -> {new}");
    }
}

/// `inventory scaffold` (contracts/cli-inventory.md): emit a skeleton `cls-` record to
/// stdout for every constraint unit that currently has NO classification record
/// pointing at it. Each skeleton carries the sentinel `disposition: "UNREVIEWED"` — a
/// value the loader REJECTS — so scaffolded output cannot be committed unedited. NEVER
/// writes into the registry.
///
/// Exit `0` on success (possibly emitting zero skeletons when everything is already
/// classified); exit `1` if the inventory or registry is unreadable.
fn inventory_scaffold(registry_dir: &Path, inventory: Option<PathBuf>) -> i32 {
    // Resolve the inventory as a sibling of the registry being scaffolded, exactly as
    // `validate` / `report` / `certify` do. Defaulting to the workspace inventory here
    // would scaffold the REAL 600+ units against a `--registry <fixture>`'s
    // classifications.
    let inventory_file = inventory.unwrap_or_else(|| inventory_paths_for(registry_dir).1);

    // Load the committed inventory (the set of units to scaffold against).
    let committed = match load_inventory(&inventory_file) {
        Ok(Some(inv)) => inv,
        Ok(None) => {
            eprintln!(
                "error: committed inventory {} does not exist",
                inventory_file.display()
            );
            return 1;
        }
        Err(e) => {
            eprintln!(
                "error: could not read committed inventory {}: {e}",
                inventory_file.display()
            );
            return 1;
        }
    };

    // Load the registry's existing classifications (the already-covered constraints).
    let registry = match Registry::load(registry_dir) {
        Ok(registry) => registry,
        Err(e) => {
            eprintln!(
                "error: could not read registry {}: {e}",
                registry_dir.display()
            );
            return 1;
        }
    };
    let classified: HashSet<&str> = registry
        .classifications
        .iter()
        .map(|c| c.constraint.as_str())
        .collect();

    // One skeleton per unclassified unit, in the inventory's committed (id-sorted) order.
    let skeletons: Vec<ScaffoldRecord> = committed
        .units
        .iter()
        .filter(|u| !classified.contains(u.id.as_str()))
        .map(ScaffoldRecord::for_unit)
        .collect();

    // A single JSON array on stdout (deterministic, byte-stable); diagnostics on stderr.
    match serde_json::to_string_pretty(&skeletons) {
        Ok(doc) => println!("{doc}"),
        Err(e) => {
            eprintln!("error: could not serialize scaffold records: {e}");
            return 1;
        }
    }
    eprintln!(
        "emitted {} skeleton classification record(s) for {} (sentinel disposition \"UNREVIEWED\" — \
         edit before committing)",
        skeletons.len(),
        inventory_file.display()
    );
    0
}

/// A skeleton classification record emitted by `inventory scaffold`. It is NOT the
/// typed [`deacon_conformance::model::Classification`] because its `disposition` is the
/// sentinel string `"UNREVIEWED"`, which that closed enum deliberately rejects at load.
/// `rationale`/`notes` are emitted as explicit `null` placeholders for the human to fill.
#[derive(Debug, serde::Serialize)]
struct ScaffoldRecord {
    id: String,
    constraint: String,
    disposition: &'static str,
    behaviors: Vec<String>,
    rationale: Option<String>,
    notes: Option<String>,
}

impl ScaffoldRecord {
    /// The scaffold sentinel disposition the loader rejects (contracts/cli-inventory.md).
    const SENTINEL: &'static str = "UNREVIEWED";

    fn for_unit(unit: &deacon_conformance::model::ConstraintUnit) -> ScaffoldRecord {
        // `id` mirrors the constraint tail: `cls-` + the tail of the `cst-` id.
        let tail = unit.id.strip_prefix("cst-").unwrap_or(unit.id.as_str());
        ScaffoldRecord {
            id: format!("cls-{tail}"),
            constraint: unit.id.clone(),
            disposition: ScaffoldRecord::SENTINEL,
            behaviors: Vec::new(),
            rationale: None,
            notes: None,
        }
    }
}

/// Structural validation (V1–V14 + SCHEMA), per contracts/cli.md and
/// contracts/classification-schema.md:
///
/// - text mode: one violation per line on stdout, nothing on success;
/// - `--json` mode: a single `{ "ok", "violations" }` document on stdout;
///
/// with logs/diagnostics always on stderr. Exit codes: `0` valid, `1` one or more
/// violations (all reported, not first-failure), `2` unreadable registry root.
///
/// The `validate` command enforces the full class set, including the schema-constraint
/// inventory join (V11–V14) against the workspace's committed inventory + pinned
/// schemas. `report` / `certify` gate on the registry-only [`validate_path`] (V1–V10)
/// first; `certify` then evaluates V11–V14 itself as blocking items (see `certify_cmd`),
/// while `report` only summarizes the join without gating on it.
fn validate(registry_dir: &Path, today: &str, json: bool) -> i32 {
    let repo_root = workspace_root();
    // The committed inventory + vendored schemas are siblings of the registry dir under
    // the same `conformance/` tree, so `--registry <fixture>` (which ships no inventory)
    // naturally validates V1–V10 only, while the real `conformance/registry` picks up its
    // `../inventory` + `../schemas` and enforces the full V1–V14 set.
    let (schemas_dir, inventory_file) = inventory_paths_for(registry_dir);
    let inputs = InventoryInputs {
        schemas_dir: &schemas_dir,
        inventory_file: &inventory_file,
    };
    let (spec_dir, clauses_file) = clause_paths_for(registry_dir);
    let clause_inputs = ClauseInputs {
        spec_dir: &spec_dir,
        clauses_file: &clauses_file,
    };
    let violations = match validate_path_with_inventory(
        registry_dir,
        today,
        &repo_root,
        &inputs,
        &clause_inputs,
    ) {
        Ok(violations) => violations,
        Err(LoadError::Root { path, cause }) => {
            eprintln!("error: cannot read registry root {path:?}: {cause}");
            return 2;
        }
        // Schema failures fold into SCHEMA-class violations, so the only `Err` returned
        // is `Root`; treat anything else defensively as usage.
        Err(other) => {
            eprintln!("error: {other}");
            return 2;
        }
    };

    if json {
        emit_json(&violations, registry_dir);
    } else {
        emit_text(&violations, registry_dir);
    }

    if violations.is_empty() { 0 } else { 1 }
}

/// Text mode: one `"<code> <record>: <message>"` line per violation on stdout;
/// nothing on stdout on success. A short summary goes to stderr either way.
fn emit_text(violations: &[Violation], registry_dir: &Path) {
    for v in violations {
        println!("{} {}: {}", v.code, v.record, v.message);
    }
    if violations.is_empty() {
        eprintln!("ok: {} is valid", registry_dir.display());
    } else {
        eprintln!(
            "error: {} has {} violation(s)",
            registry_dir.display(),
            violations.len()
        );
    }
}

/// JSON mode: a single `{ "ok": bool, "violations": [...] }` document on stdout.
fn emit_json(violations: &[Violation], registry_dir: &Path) {
    #[derive(serde::Serialize)]
    struct Report<'a> {
        ok: bool,
        violations: &'a [Violation],
    }
    let report = Report {
        ok: violations.is_empty(),
        violations,
    };
    match serde_json::to_string_pretty(&report) {
        Ok(doc) => println!("{doc}"),
        Err(e) => eprintln!("error: could not serialize report: {e}"),
    }
    eprintln!(
        "validated {} ({} violation(s))",
        registry_dir.display(),
        violations.len()
    );
}

/// `report` (contracts/cli.md): validate first (violations → exit 1, no report),
/// then write the deterministic `report.json` + `report.md` into `--out-dir`
/// (default `<workspace>/target/conformance/`). Exit `0` on success, `2` on IO error.
fn report(registry_dir: &Path, today: &str, out_dir: Option<PathBuf>) -> i32 {
    let registry = match load_and_validate(registry_dir, today) {
        Ok(registry) => registry,
        Err(code) => return code,
    };

    // The committed inventory is a sibling of the registry dir under the same
    // `conformance/` tree (mirrors `validate`'s V11–V14 pathing): the real
    // `conformance/registry` picks up its `../inventory/constraints.json`, while a
    // `--registry <fixture>` (which ships no sibling inventory) yields `None` and a
    // present-but-zeroed inventory section.
    let (_schemas_dir, inventory_file) = inventory_paths_for(registry_dir);
    let inventory = match load_inventory(&inventory_file) {
        Ok(inventory) => inventory,
        Err(e) => {
            eprintln!(
                "error: could not load inventory {}: {e}",
                inventory_file.display()
            );
            return 2;
        }
    };

    // The committed clause inventory + spec manifest are siblings of the registry dir.
    let (spec_dir, clauses_file) = clause_paths_for(registry_dir);
    let clause_inventory = match load_clause_inventory(&clauses_file) {
        Ok(inv) => inv,
        Err(e) => {
            eprintln!(
                "error: could not load clause inventory {}: {e}",
                clauses_file.display()
            );
            return 2;
        }
    };
    // Document-scope sets for the clause join (empty when the manifest is absent).
    let (authoring_docs, covered_docs) = match load_spec_manifest(&spec_dir) {
        Ok(manifest) => {
            let authoring: std::collections::HashSet<String> = manifest
                .documents
                .iter()
                .filter(|d| d.scope == DocumentScope::Authoring)
                .map(|d| d.key.clone())
                .collect();
            let covered: std::collections::HashSet<String> = registry
                .clause_classifications
                .iter()
                .filter_map(|c| c.document.clone())
                .filter(|d| authoring.contains(d))
                .collect();
            (authoring, covered)
        }
        Err(_) => (
            std::collections::HashSet::new(),
            std::collections::HashSet::new(),
        ),
    };

    let out_dir = out_dir.unwrap_or_else(default_report_dir);
    match write_reports(
        &registry,
        inventory.as_ref(),
        clause_inventory.as_ref(),
        &authoring_docs,
        &covered_docs,
        &out_dir,
    ) {
        Ok((json_path, md_path)) => {
            // Human-readable result on stdout; diagnostics on stderr.
            println!("{}", json_path.display());
            println!("{}", md_path.display());
            eprintln!("wrote conformance report to {}", out_dir.display());
            0
        }
        Err(e) => {
            eprintln!(
                "error: could not write reports to {}: {e}",
                out_dir.display()
            );
            2
        }
    }
}

/// `certify` (contracts/cli.md + contracts/cli-inventory.md): validate first (invalid
/// → exit 1), then evaluate strict certification — including the schema-constraint
/// inventory join (V11–V14), which blocks exactly as gaps/uncovered behaviors do. Exit
/// `0` certified, `1` not certified (blocking items listed) or registry invalid, `2`
/// usage/IO. The committed inventory + vendored schemas are resolved as siblings of the
/// registry dir (mirroring `validate`); a fixture registry that ships neither scopes the
/// V11–V14 join out, so certification reduces to the gap/uncovered gate.
fn certify_cmd(registry_dir: &Path, today: &str, json: bool) -> i32 {
    let registry = match load_and_validate(registry_dir, today) {
        Ok(registry) => registry,
        Err(code) => return code,
    };

    let (schemas_dir, inventory_file) = inventory_paths_for(registry_dir);
    let inputs = InventoryInputs {
        schemas_dir: &schemas_dir,
        inventory_file: &inventory_file,
    };
    let (spec_dir, clauses_file) = clause_paths_for(registry_dir);
    let clause_inputs = ClauseInputs {
        spec_dir: &spec_dir,
        clauses_file: &clauses_file,
    };
    let result = certify(&registry, &inputs, &clause_inputs);

    if json {
        match serde_json::to_string_pretty(&result) {
            Ok(doc) => println!("{doc}"),
            Err(e) => {
                eprintln!("error: could not serialize certification: {e}");
                return 2;
            }
        }
    } else {
        for item in &result.blocking {
            use deacon_conformance::certify::BlockingKind;
            match item.kind {
                BlockingKind::Gap => println!("blocking gap: {}", item.id),
                BlockingKind::Uncovered => println!("blocking uncovered: {}", item.id),
                BlockingKind::Constraint => println!(
                    "blocking constraint ({}): {}",
                    item.code.as_deref().unwrap_or("?"),
                    item.id
                ),
                BlockingKind::Clause => println!(
                    "blocking clause ({}): {}",
                    item.code.as_deref().unwrap_or("?"),
                    item.id
                ),
            }
        }
        if result.certified {
            println!("certified: {}", result.profile);
        } else {
            println!("NOT certified: {}", result.profile);
        }
    }
    eprintln!(
        "certification for {}: {} ({} blocking, {} waived)",
        registry_dir.display(),
        if result.certified {
            "certified"
        } else {
            "not certified"
        },
        result.blocking.len(),
        result.waived.len(),
    );

    if result.certified { 0 } else { 1 }
}

/// Load the registry at `registry_dir`, running the full validation engine first.
/// Returns the loaded [`Registry`] when valid, or the process exit code to return
/// (`1` for any violation / schema error, `2` for an unreadable root) with the
/// cause already reported on stderr. `report`/`certify` share this gate so both
/// "run validation first" per contracts/cli.md.
fn load_and_validate(registry_dir: &Path, today: &str) -> Result<Registry, i32> {
    let repo_root = workspace_root();
    // `validate_path` folds schema-load failures into SCHEMA-class violations and
    // only returns `Err` for an unreadable root.
    let violations = match validate_path(registry_dir, today, &repo_root) {
        Ok(violations) => violations,
        Err(LoadError::Root { path, cause }) => {
            eprintln!("error: cannot read registry root {path:?}: {cause}");
            return Err(2);
        }
        Err(other) => {
            eprintln!("error: {other}");
            return Err(2);
        }
    };

    if !violations.is_empty() {
        eprintln!(
            "error: {} is not valid ({} violation(s)); no action taken:",
            registry_dir.display(),
            violations.len()
        );
        for v in &violations {
            eprintln!("  {} {}: {}", v.code, v.record, v.message);
        }
        return Err(1);
    }

    // Valid — re-load the parsed registry for report/certify. Schema parsing cannot
    // fail here (it just succeeded inside `validate_path`); a Root error is likewise
    // impossible, but map any residual error to a usage exit defensively.
    Registry::load(registry_dir).map_err(|e| {
        eprintln!("error: {e}");
        2
    })
}

/// The default report output directory: `<workspace>/target/conformance/`
/// (research Decision 7). Overridable via `report --out-dir`.
fn default_report_dir() -> PathBuf {
    workspace_root().join("target").join("conformance")
}

/// Resolve the `(schemas_dir, inventory_file)` that belong to a registry, as siblings
/// under the same `conformance/` tree: `<registry>/../schemas/<pin>` and
/// `<registry>/../inventory/constraints.json`. For the real
/// `<workspace>/conformance/registry` this yields the committed inventory + vendored
/// schemas; for a fixture registry that ships neither, both paths are absent and the
/// V11–V14 inventory join scopes itself out (see `validate::check_inventory`).
fn inventory_paths_for(registry_dir: &Path) -> (PathBuf, PathBuf) {
    let base = registry_dir.parent().unwrap_or(registry_dir);
    let schemas_dir = base.join("schemas").join(CURRENT_SCHEMA_PIN);
    let inventory_file = base.join("inventory").join("constraints.json");
    (schemas_dir, inventory_file)
}

/// Resolve the effective "today": the validated `--today` flag, else the current
/// UTC calendar date (via `jiff`). Returns the canonical `YYYY-MM-DD` string.
fn resolve_today(flag: Option<&str>) -> anyhow::Result<String> {
    match flag {
        Some(raw) => {
            let date: jiff::civil::Date = raw
                .parse()
                .with_context(|| format!("invalid --today {raw:?}: expected YYYY-MM-DD"))?;
            Ok(date.to_string())
        }
        None => {
            let date = jiff::Timestamp::now()
                .to_zoned(jiff::tz::TimeZone::UTC)
                .date();
            Ok(date.to_string())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_today_uses_valid_flag_verbatim() {
        assert_eq!(resolve_today(Some("2026-07-19")).unwrap(), "2026-07-19");
    }

    #[test]
    fn resolve_today_rejects_malformed_flag() {
        assert!(resolve_today(Some("2026-13-40")).is_err());
        assert!(resolve_today(Some("not-a-date")).is_err());
    }

    #[test]
    fn resolve_today_defaults_to_current_utc_date() {
        // Shape check only (YYYY-MM-DD), value depends on the wall clock.
        let today = resolve_today(None).unwrap();
        assert_eq!(today.len(), 10, "expected YYYY-MM-DD, got {today:?}");
        assert_eq!(today.matches('-').count(), 2);
    }

    #[test]
    fn cli_parses_subcommands_and_global_flags() {
        // Global flags accepted before and after the subcommand.
        let cli = Cli::try_parse_from([
            "conformance",
            "--registry",
            "fixtures/conformance/valid",
            "validate",
            "--today",
            "2026-07-19",
        ])
        .expect("valid invocation parses");
        assert!(matches!(cli.command, Command::Validate { json: false }));
        assert_eq!(cli.today.as_deref(), Some("2026-07-19"));
        assert_eq!(
            cli.registry.as_deref(),
            Some(Path::new("fixtures/conformance/valid"))
        );

        assert!(Cli::try_parse_from(["conformance", "report"]).is_ok());
        assert!(Cli::try_parse_from(["conformance", "certify"]).is_ok());
        assert!(
            Cli::try_parse_from(["conformance", "bogus"]).is_err(),
            "unknown subcommand must be rejected"
        );
    }
}
