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

use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};

use anyhow::Context;
use clap::{Parser, Subcommand, ValueEnum};

use deacon_conformance::baseline::{
    self, BaselineDrift, BaselineFile, generate_baseline, load_baseline, write_baseline,
};
use deacon_conformance::case_hash::hashes_for_case;
use deacon_conformance::certification::{
    Environment as ReportEnvironment, ReportInputs, build_report,
    render_json as render_certification_json, render_md as render_certification_md,
};
use deacon_conformance::certify::{EvidenceInputs, certify_with_evidence};
use deacon_conformance::clause::{generate_clauses, render as render_clauses, write_clauses};
use deacon_conformance::clause_diff::{
    diff as clause_diff, render_json as render_clause_diff_json, render_md as render_clause_diff_md,
};
use deacon_conformance::conservation;
use deacon_conformance::coverage_report::{build_coverage_reports, write_coverage_reports};
use deacon_conformance::diff::{
    diff, render_json as render_diff_json, render_md as render_diff_md,
};
use deacon_conformance::discovery::queue::{self, DiscoveryData};
use deacon_conformance::discovery::report as discovery_report_mod;
use deacon_conformance::drift::check::{check_drift, check_proposal, drift_state_is_known};
use deacon_conformance::drift::{load_drift, load_proposal};
use deacon_conformance::inventory::{
    InventoryDrift, compare, generate_inventory, render, write_inventory,
};
use deacon_conformance::lane::{
    CaseMembership, ExecutionUnit, build_lane_report, case_memberships, check_lanes,
    derive_execution_units, load_lanes, render_lane_report_md, scaffold_lane,
};
use deacon_conformance::load::{
    LoadError, Registry, load_clause_inventory, load_inventory, load_spec_manifest,
};
use deacon_conformance::manifest::RequiredCase;
use deacon_conformance::model::{ClauseInventory, ConstraintInventory, DocumentScope};
use deacon_conformance::obligation::{
    ObligationInventory, ObligationKind, compare as compare_obligations, generate_obligations,
    render as render_obligations, write_obligations,
};
use deacon_conformance::report::write_reports;
use deacon_conformance::residual::UNREVIEWED_SENTINEL as SCAFFOLD_SENTINEL;
use deacon_conformance::snapshot;
use deacon_conformance::validate::{
    ClauseInputs, InventoryInputs, Violation, validate_path, validate_path_with_inventory,
};
use deacon_conformance::{
    CURRENT_SCHEMA_PIN, CURRENT_SPEC_PIN, atomic_write, clause_paths_for, default_clauses_file,
    default_inventory_file, default_pinned_schemas_dir, default_pinned_spec_dir,
    default_registry_dir, drift_dir_for, lanes_dir_for, migration_paths_for, workspace_root,
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
    /// Generate the human-facing parity page (`docs/PARITY.md`) from the registry.
    ///
    /// Deterministic: the same registry renders the same bytes. Prints to stdout
    /// unless `--write` is given, so a check can diff it without a temp file.
    ParityPage {
        /// Write to `docs/PARITY.md` instead of printing.
        #[arg(long)]
        write: bool,
        /// Exit non-zero if the committed page differs from a fresh render, naming
        /// the drift. This is the gate that keeps the page honest.
        #[arg(long, conflicts_with = "write")]
        check: bool,
    },
    /// Strict certification for the active profile (release gate).
    Certify {
        /// Emit a single JSON document
        /// (`{ "certified", "profile", "blocking", "waived" }`) on stdout instead of
        /// the human-readable summary; logs still go to stderr (contracts/cli.md).
        #[arg(long)]
        json: bool,
        /// Directory to write `certification.json` + `certification.md` into
        /// (026-continuous-conformance-certification, FR-034). Written on BOTH a
        /// certified and a blocked verdict, so a blocked release still ships the
        /// artifact explaining why.
        #[arg(long, value_name = "DIR")]
        report_dir: Option<PathBuf>,
        /// Execution manifest to verify (026, FR-033d). Defaults to
        /// `target/conformance/execution-manifest.json`. An absent manifest is the
        /// *missing required execution* blocker, never a skip.
        #[arg(long, value_name = "FILE")]
        manifest: Option<PathBuf>,
    },
    /// Continuous-integration lane tooling (026-continuous-conformance-certification).
    Lane {
        #[command(subcommand)]
        command: LaneCommand,
    },
    /// Upstream source-drift tooling (026-continuous-conformance-certification).
    Drift {
        #[command(subcommand)]
        command: DriftCommand,
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
    /// Committed snapshot staleness/diff tooling (022-conformance-runner). Hermetic,
    /// read-only (never writes; the reviewed refresh is the `parity-harness`
    /// `conformance-snapshot` bin). Handlers land in User Story 2 (T031–T033).
    Snapshot {
        #[command(subcommand)]
        command: SnapshotCommand,
    },
    /// Migration mapping tooling (023-migrate-parity-to-conformance). Hermetic.
    Migration {
        #[command(subcommand)]
        command: MigrationCommand,
    },
    /// Pre-migration coverage baseline tooling (023-migrate-parity-to-conformance).
    /// Hermetic: reads the repository tree, the parity registry, and the conformance
    /// registry. No Docker, no network, no oracle.
    Baseline {
        #[command(subcommand)]
        command: BaselineCommand,
    },
    /// Deterministic coverage obligation tooling (024-deterministic-conformance-coverage,
    /// contracts/coverage-cli.md). Hermetic: no network, no Docker, no reference oracle.
    /// Dev-only — NEVER part of the shipped `deacon` consumer CLI.
    Coverage {
        #[command(subcommand)]
        command: CoverageCommand,
    },
    /// Exploratory parity discovery tooling (025-exploratory-parity-discovery,
    /// contracts/discovery-cli.md). Hermetic: no network, no Docker, no reference oracle.
    /// Dev-only — NEVER part of the shipped `deacon` consumer CLI.
    ///
    /// Operates on `conformance/discovery/`, a SIBLING of the registry that no registry
    /// loader path reaches, so nothing here can influence `certify`.
    Discovery {
        #[command(subcommand)]
        command: DiscoveryCommand,
    },
}

/// `discovery <check|report|triage|split|scaffold>` (contracts/discovery-cli.md).
///
/// **The exit-status rule for every command here**: the status reflects *whether the
/// command ran*, never *what it found*. A queue holding fifty untriaged findings still
/// exits `0` from `report`. Any command whose status depended on its findings would
/// become a gate the moment someone wired it into CI — and a stochastic gate makes green
/// non-reproducible. `check` is the one exception, and it is a *structural* verdict on
/// the records, not a verdict on what was discovered.
#[derive(Debug, Subcommand)]
enum DiscoveryCommand {
    /// Validate the discovery data root (violation classes D1–D5). Read-only by
    /// construction. Reports ALL violations in one pass, matching `validate`.
    Check {
        /// Emit a single JSON document (`{ "ok", "violations" }`) on stdout instead of
        /// one violation per line; logs still go to stderr.
        #[arg(long)]
        json: bool,
    },
    /// Render the findings queue and campaign history to `target/discovery/queue.{json,md}`
    /// (git-ignored, byte-stable). **Never gates**: the exit status reflects only whether
    /// the artifacts were produced.
    Report {
        /// Directory to write `queue.json` and `queue.md` into. Defaults to
        /// `<workspace>/target/discovery/`.
        #[arg(long, value_name = "DIR")]
        out_dir: Option<PathBuf>,
    },
    /// Record a reviewer's classification. The ONLY writer of `classification`.
    Triage {
        /// The finding to classify (`fnd-<hash8>`).
        #[arg(value_name = "FND-ID")]
        finding: String,
        /// One of the six closed classifications.
        #[arg(long, value_name = "CLASSIFICATION")]
        classification: String,
        /// Reviewer prose recorded alongside the classification.
        #[arg(long, value_name = "TEXT")]
        notes: Option<String>,
    },
    /// Split a signature-merged finding whose witnesses turn out to have different
    /// causes (FR-032). The parent becomes an inert ancestor.
    Split {
        /// The finding to split (`fnd-<hash8>`).
        #[arg(value_name = "FND-ID")]
        finding: String,
    },
    /// Emit a skeleton behavior + case + fixture layout for promoting a finding, to
    /// **stdout only**, with `UNREVIEWED` sentinels the registry loader rejects.
    ///
    /// Stdout-only is the same discipline as `inventory scaffold` / `clause scaffold`:
    /// generation never writes a hand-authored file. Promotion is a human editing the
    /// registry with this output as a starting point, which is what makes FR-036 hold —
    /// there is no code path from a finding to a registry write.
    Scaffold {
        /// The finding to scaffold a promotion for (`fnd-<hash8>`).
        #[arg(value_name = "FND-ID")]
        finding: String,
        /// Scaffold a **tolerance** instead of a promotion: a scoped `wvr-` waiver
        /// (rationale + `expires`) plus the `allowedDifferences` entry that references
        /// it (FR-041). Still stdout only.
        ///
        /// A blanket or unscoped tolerance is refused rather than emitted — a bare-channel
        /// `observablePath` is a global ignore list wearing a waiver id.
        #[arg(long)]
        tolerate: bool,
    },
}

/// `coverage <generate|check|report|scaffold>` (contracts/coverage-cli.md). `generate`
/// is the sole writer of `conformance/obligations/obligations.json`; `check` byte-compares
/// without writing; `report` writes the four read-only report families to
/// `target/conformance/`; `scaffold` emits skeleton dispositions to stdout only.
#[derive(Debug, Subcommand)]
enum CoverageCommand {
    /// Regenerate `conformance/obligations/obligations.json` from the scenario model,
    /// applicability rules, high-risk triples, and behavior records.
    Generate {
        /// Redirect the written file for inspection without touching the committed one.
        #[arg(long, value_name = "FILE")]
        out: Option<PathBuf>,
    },
    /// Regenerate in memory and byte-compare against the committed obligations file.
    Check {},
    /// Write the four coverage report families to `target/conformance/` (git-ignored).
    /// Read-only with respect to the registry; exit code never reflects coverage.
    Report {
        /// Directory to write the report artifacts into. Defaults to
        /// `<workspace>/target/conformance/`.
        #[arg(long, value_name = "DIR")]
        out_dir: Option<PathBuf>,
    },
    /// Emit skeleton `odp-` disposition records to stdout for every undispositioned
    /// applicable obligation, each carrying an `"UNREVIEWED"` sentinel the loader rejects.
    /// Never writes the registry.
    Scaffold {},
}

/// `migration <scaffold>` — hand-authored mapping/residual tooling
/// (contracts/cli-commands.md). Generation NEVER writes a hand-authored file: `scaffold`
/// emits skeletons to **stdout** only. `migration report` / `migration check` (the
/// conservation accounting) land with User Story 5.
#[derive(Debug, Subcommand)]
enum MigrationCommand {
    /// Produce the deterministic before-and-after conservation accounting and write
    /// `migration-report.{json,md}`.
    Report {
        /// Output format for the stdout document. Defaults to `json`.
        #[arg(long, value_name = "FORMAT", default_value = "json")]
        format: DiffFormat,
        /// Directory to write `migration-report.json` + `.md` into. Defaults to
        /// `<workspace>/target/conformance/`.
        #[arg(long, value_name = "DIR")]
        out_dir: Option<PathBuf>,
        /// Fold in an equivalence ledger (`target/parity/equivalence.json`), enabling the
        /// strictness-improvement and deletable-carrier sections. OPT-IN by design: the
        /// ledger is a git-ignored artifact of a live parity run, and a hermetic command
        /// whose exit code depends on whether someone happened to produce one locally
        /// would not be deterministic (FR-043).
        #[arg(long, value_name = "FILE")]
        ledger: Option<PathBuf>,
    },
    /// The gating form of `report`: the same computation, NO file output, failing while
    /// naming each unaccounted item, missing counterpart, weakened error path, or
    /// inflated behavior denominator.
    Check {
        /// Fold in an equivalence ledger (see `report --ledger`).
        #[arg(long, value_name = "FILE")]
        ledger: Option<PathBuf>,
    },
    /// Emit skeleton `mapping.json` / `residuals.json` records to **stdout** for every
    /// unmapped baseline unit and every unmapped characterized exception, each carrying
    /// the `UNREVIEWED` sentinel the loader REJECTS. Never writes the registry.
    Scaffold {
        /// Committed baseline file to scaffold from. Defaults to
        /// `<registry>/../migration/baseline.json`.
        #[arg(long, value_name = "FILE")]
        baseline: Option<PathBuf>,
    },
}

/// `baseline <generate|check>` — the machine-owned frozen pre-migration inventory
/// (contracts/cli-commands.md). `generate` is its ONLY writer; `check` never writes.
#[derive(Debug, Subcommand)]
enum BaselineCommand {
    /// Enumerate the pre-migration inventory and write `conformance/migration/baseline.json`.
    Generate {
        /// Record this commit as the freeze `revision`. Defaults to the committed
        /// baseline's existing revision, or `unfrozen` when there is none.
        #[arg(long, value_name = "SHA")]
        freeze: Option<String>,
        /// Overwrite an already-frozen baseline. Without this, re-running can never
        /// silently relax the bar the conservation claim is measured against (FR-045).
        #[arg(long)]
        force: bool,
        /// Output baseline file. Defaults to
        /// `<registry>/../migration/baseline.json`.
        #[arg(long, value_name = "FILE")]
        out: Option<PathBuf>,
        /// Repository root to enumerate. Defaults to the workspace root.
        #[arg(long, value_name = "DIR")]
        repo_root: Option<PathBuf>,
    },
    /// Recompute the inventory in memory and byte-compare it against the committed
    /// file. NEVER writes. Exit `0` on match; `1` naming each added, removed, or
    /// changed unit (FR-004).
    Check {
        /// Committed baseline file to compare against (see `generate --out`).
        #[arg(long, value_name = "FILE")]
        baseline: Option<PathBuf>,
        /// Assert the committed freeze `revision` is exactly this commit. Without it the
        /// regeneration adopts whatever revision the committed file records, so only the
        /// unit records are compared; with it, a tampered freeze label is reported too.
        #[arg(long, value_name = "SHA")]
        freeze: Option<String>,
        /// Repository root to enumerate. Defaults to the workspace root.
        #[arg(long, value_name = "DIR")]
        repo_root: Option<PathBuf>,
    },
}

/// `snapshot <check|diff>` — hermetic, read-only staleness + drift operations over the
/// committed `conformance/snapshots/<os>-<arch>/<case-id>/` trees (contract
/// runner-cli.md §1). NEVER writes evidence — ordinary runs only read/compare (FR-021).
#[derive(Debug, Subcommand)]
enum SnapshotCommand {
    /// Recompute case/fixture hashes + probe the environment and compare to the
    /// committed provenance; report `stale` (naming the mismatched field) or
    /// `no-reference-for-platform`. Exit `0` pass, `1` stale/violation, `2` malformed.
    Check {
        /// Restrict the check to a single case id (default: all cases).
        #[arg(long, value_name = "ID")]
        case: Option<String>,
        /// Restrict the check to a single `os-arch` platform (default: the current one).
        #[arg(long, value_name = "OS-ARCH")]
        platform: Option<String>,
    },
    /// Deterministic drift between two snapshot trees.
    Diff {
        /// The old (left) snapshot directory.
        #[arg(value_name = "OLD-DIR")]
        old: PathBuf,
        /// The new (right) snapshot directory.
        #[arg(value_name = "NEW-DIR")]
        new: PathBuf,
        /// Output format. Defaults to `json`; `md` renders the human review document.
        #[arg(long, value_name = "FORMAT", default_value = "json")]
        format: DiffFormat,
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
/// `lane` subcommands (026-continuous-conformance-certification, contracts/cli-lane.md).
#[derive(Debug, Subcommand)]
enum LaneCommand {
    /// Report V34 lane-integrity violations. Read-only by construction.
    Check {
        /// Emit `{ "ok", "violations" }` on stdout instead of one violation per line.
        #[arg(long)]
        json: bool,
    },
    /// Write the per-lane ran/excluded breakdown. NEVER gates — the exit code reflects
    /// whether the report could be written, never what it says.
    Report {
        #[arg(long, value_name = "DIR")]
        out_dir: Option<PathBuf>,
    },
    /// Emit a skeleton lane record to stdout. Writes nothing.
    Scaffold {
        /// The unassigned unit the skeleton should claim.
        #[arg(long, value_name = "UNIT")]
        for_unit: String,
    },
}

/// `drift` subcommands (026, contracts/cli-drift.md).
#[derive(Debug, Subcommand)]
enum DriftCommand {
    /// Report V36 drift-record integrity violations.
    Check {
        #[arg(long)]
        json: bool,
    },
    /// Render the committed observations. NEVER gates.
    Report {
        #[arg(long, value_name = "DIR")]
        out_dir: Option<PathBuf>,
    },
    /// Emit a skeleton observation to stdout. Writes nothing.
    Scaffold,
    /// Upgrade-proposal tooling.
    Proposal {
        #[command(subcommand)]
        command: ProposalCommand,
    },
}

/// `drift proposal` subcommands.
#[derive(Debug, Subcommand)]
enum ProposalCommand {
    /// Verify a bundle carries all seven sections and is deterministic.
    Check {
        /// The bundle to check.
        #[arg(value_name = "FILE")]
        proposal: PathBuf,
    },
}

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
        Command::ParityPage { write, check } => parity_page_cmd(&registry_dir, write, check),
        Command::Certify {
            json,
            report_dir,
            manifest,
        } => certify_cmd(&registry_dir, &today, json, report_dir, manifest),
        Command::Lane { command } => match command {
            LaneCommand::Check { json } => lane_check(&registry_dir, json),
            LaneCommand::Report { out_dir } => lane_report(&registry_dir, out_dir),
            LaneCommand::Scaffold { for_unit } => lane_scaffold(&for_unit),
        },
        Command::Drift { command } => match command {
            DriftCommand::Check { json } => drift_check(&registry_dir, json),
            DriftCommand::Report { out_dir } => drift_report(&registry_dir, out_dir),
            DriftCommand::Scaffold => drift_scaffold(),
            DriftCommand::Proposal { command } => match command {
                ProposalCommand::Check { proposal } => drift_proposal_check(&proposal),
            },
        },
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
        Command::Migration { command } => match command {
            MigrationCommand::Report {
                format,
                out_dir,
                ledger,
            } => migration_report_cmd(&registry_dir, format, out_dir, ledger),
            MigrationCommand::Check { ledger } => migration_check(&registry_dir, ledger),
            MigrationCommand::Scaffold { baseline } => migration_scaffold(&registry_dir, baseline),
        },
        Command::Baseline { command } => match command {
            BaselineCommand::Generate {
                freeze,
                force,
                out,
                repo_root,
            } => baseline_generate(&registry_dir, freeze, force, out, repo_root),
            BaselineCommand::Check {
                baseline,
                freeze,
                repo_root,
            } => baseline_check(&registry_dir, baseline, freeze, repo_root),
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
        Command::Snapshot { command } => match command {
            SnapshotCommand::Check { case, platform } => {
                snapshot_check(&registry_dir, case.as_deref(), platform.as_deref())
            }
            SnapshotCommand::Diff { old, new, format } => snapshot_diff(&old, &new, format),
        },
        Command::Coverage { command } => match command {
            CoverageCommand::Generate { out } => coverage_generate(&registry_dir, out),
            CoverageCommand::Check {} => coverage_check(&registry_dir),
            CoverageCommand::Report { out_dir } => {
                coverage_report_cmd(&registry_dir, &today, out_dir)
            }
            CoverageCommand::Scaffold {} => coverage_scaffold(&registry_dir, &today),
        },
        Command::Discovery { command } => match command {
            DiscoveryCommand::Check { json } => discovery_check(&registry_dir, json),
            DiscoveryCommand::Report { out_dir } => discovery_report(&registry_dir, out_dir),
            DiscoveryCommand::Triage {
                finding,
                classification,
                notes,
            } => discovery_triage(&registry_dir, &finding, &classification, notes.as_deref()),
            DiscoveryCommand::Split { finding } => discovery_split(&registry_dir, &finding),
            DiscoveryCommand::Scaffold { finding, tolerate } => {
                discovery_scaffold(&registry_dir, &finding, tolerate)
            }
        },
    }
}

// ---------------------------------------------------------------------------
// discovery (025-exploratory-parity-discovery)
// ---------------------------------------------------------------------------

/// Load the registry a `discovery` subcommand needs to resolve the queue's outward
/// references (declared channels, recorded revisions, and — from US5 — cases).
///
/// The direction is one-way and load-bearing: discovery reads the registry, the registry
/// never reads discovery. That asymmetry is what makes the queue unreachable from
/// `certify` (research D6).
fn load_for_discovery(registry_dir: &Path) -> Result<Registry, i32> {
    // Deliberately NOT `load_for_coverage`: its diagnostic ends "…before generating
    // obligations", which names the wrong subject entirely when a `discovery` subcommand
    // is what failed. A message that misidentifies what the operator was doing sends them
    // to the wrong file.
    match Registry::load(registry_dir) {
        Ok(registry) => Ok(registry),
        Err(LoadError::Schema(errors)) => {
            for violation in deacon_conformance::validate::schema_violations(&errors) {
                eprintln!(
                    "{} {}: {}",
                    violation.code, violation.record, violation.message
                );
            }
            eprintln!(
                "error: {} is schema-invalid; discovery resolves its channels and pins \
                 against the registry, so fix the registry records first",
                registry_dir.display()
            );
            Err(1)
        }
        Err(other) => {
            eprintln!(
                "error: cannot read registry {}: {other}",
                registry_dir.display()
            );
            Err(2)
        }
    }
}

/// A discovery data-root load failure, rendered per output mode by the caller.
///
/// Carrying the failure rather than printing it is what lets `--json` keep its contract:
/// the violation report belongs on **stdout** as a single JSON document, and a helper that
/// printed to stderr would leave `… --json | jq .ok` with a parse error instead of `false`
/// exactly when something is wrong.
struct DiscoveryLoadFailure {
    /// One `(class, record, message)` triple per failing file.
    violations: Vec<(&'static str, String, String)>,
    /// The process exit code this failure maps to.
    code: i32,
}

/// Load the discovery data root, reporting located schema failures as **D1**.
///
/// A malformed file is D1 rather than a bare load error because that is what it is: the
/// records are the queue, and a record that does not parse is a malformed record.
fn load_discovery_data(discovery_dir: &Path) -> Result<DiscoveryData, DiscoveryLoadFailure> {
    match DiscoveryData::load(discovery_dir) {
        Ok(data) => Ok(data),
        Err(LoadError::Schema(errors)) => Err(DiscoveryLoadFailure {
            violations: errors
                .iter()
                .map(|e| ("D1", e.file.display().to_string(), e.message.clone()))
                .collect(),
            code: 1,
        }),
        Err(other) => Err(DiscoveryLoadFailure {
            violations: vec![(
                "D1",
                discovery_dir.display().to_string(),
                format!("cannot read discovery data root: {other}"),
            )],
            code: 2,
        }),
    }
}

/// Print a load failure as plain text on stderr and return its exit code. Used by every
/// discovery subcommand except `check --json`, which renders it as JSON on stdout.
fn report_discovery_load_failure(failure: &DiscoveryLoadFailure, discovery_dir: &Path) -> i32 {
    for (class, record, message) in &failure.violations {
        eprintln!("{class} {record}: {message}");
    }
    eprintln!(
        "error: {} could not be loaded; the discovery data root is strict JSON and rejects \
         unknown fields, a missing `records` array, and an unsupported `schemaVersion`",
        discovery_dir.display()
    );
    failure.code
}

/// `discovery check` (contracts/discovery-cli.md): validate the discovery data root.
///
/// Read-only by construction — it never writes. Exit `0` when there are no D-class
/// violations, `1` when there are (all reported in one pass), `2` on an unreadable root
/// (the usage/IO code every other command in this binary uses).
///
/// Under `--json` the violation report is a single JSON document on **stdout** in every
/// failing case, including the case where the data root itself would not load — a
/// consumer piping to `jq` must get `{"ok": false, …}`, never an empty stream.
fn discovery_check(registry_dir: &Path, json: bool) -> i32 {
    let render = |violations: &[(&str, String, String)], ok: bool| -> Result<(), i32> {
        if json {
            let document = serde_json::json!({
                "ok": ok,
                "violations": violations
                    .iter()
                    .map(|(class, record, message)| serde_json::json!({
                        "class": class,
                        "record": record,
                        "message": message,
                    }))
                    .collect::<Vec<_>>(),
            });
            match serde_json::to_string_pretty(&document) {
                Ok(text) => println!("{text}"),
                Err(e) => {
                    eprintln!("error: could not render the check result: {e}");
                    return Err(2);
                }
            }
        } else {
            for (class, record, message) in violations {
                println!("{class} {record}: {message}");
            }
        }
        Ok(())
    };

    let registry = match load_for_discovery(registry_dir) {
        Ok(registry) => registry,
        Err(code) => {
            // The registry failure is already reported as located `V…`/`SCHEMA` lines on
            // stderr by `load_for_discovery`; emit a well-formed `ok: false` document so
            // the JSON contract holds even here.
            if let Err(render_code) = render(&[], false) {
                return render_code;
            }
            return code;
        }
    };
    let discovery_dir = deacon_conformance::discovery_dir_for(registry_dir);
    let data = match load_discovery_data(&discovery_dir) {
        Ok(data) => data,
        Err(failure) => {
            let violations: Vec<(&str, String, String)> = failure
                .violations
                .iter()
                .map(|(c, r, m)| (*c, r.clone(), m.clone()))
                .collect();
            if let Err(render_code) = render(&violations, false) {
                return render_code;
            }
            if !json {
                // Text mode already printed the violations above; add the remedy line.
                eprintln!(
                    "error: {} could not be loaded; the discovery data root is strict JSON \
                     and rejects unknown fields, a missing `records` array, and an \
                     unsupported `schemaVersion`",
                    discovery_dir.display()
                );
            }
            return failure.code;
        }
    };

    let checked = queue::check(&data, &queue::RegistryView::from_registry(&registry));
    let violations: Vec<(&str, String, String)> = checked
        .iter()
        .map(|v| (v.class(), v.record().to_string(), v.to_string()))
        .collect();
    if let Err(code) = render(&violations, violations.is_empty()) {
        return code;
    }
    let violations = checked;

    if violations.is_empty() {
        eprintln!(
            // The corpus count is named alongside the queue's: a run that validated 33
            // pinned entries and reported only "0 findings, 0 campaigns" would understate
            // what it checked, and the D4 immutable-reference clause is the one thing here
            // that must visibly run on every pull request.
            "ok: {} validates clean ({} finding(s), {} campaign(s), {} corpus entr(ies))",
            discovery_dir.display(),
            data.findings.len(),
            data.campaigns.len(),
            data.corpus.len()
        );
        0
    } else {
        eprintln!(
            "error: {} has {} discovery violation(s)",
            discovery_dir.display(),
            violations.len()
        );
        1
    }
}

/// `discovery report` (contracts/discovery-cli.md): render the queue and the campaign
/// history to `target/discovery/queue.{json,md}`.
///
/// **Never gates.** The exit status reflects only whether the artifacts were produced:
/// `0` when written, `1` when they could not be (including when the inputs could not be
/// read, since without them there is nothing to write). A queue holding fifty untriaged
/// findings still exits `0`.
fn discovery_report(registry_dir: &Path, out_dir: Option<PathBuf>) -> i32 {
    let registry = match load_for_discovery(registry_dir) {
        Ok(registry) => registry,
        // `report` has only two statuses in the contract, so an unreadable input is a
        // "could not write" — there is nothing to write from.
        Err(_) => return 1,
    };
    let discovery_dir = deacon_conformance::discovery_dir_for(registry_dir);
    let data = match load_discovery_data(&discovery_dir) {
        Ok(data) => data,
        Err(failure) => {
            report_discovery_load_failure(&failure, &discovery_dir);
            // The contract gives `report` exactly two statuses, so every failure — an
            // unreadable root included — collapses to "could not write". Passing the
            // loader's `2` through would invent a third status the contract does not have.
            return 1;
        }
    };

    let pins = discovery_report_mod::CurrentPins::from_registry(&registry);
    // The behavior index is what lets promoted findings be *reported* grouped under the
    // behavior their case names (FR-031) — a reviewed mapping, never one the report
    // invents, because a finding itself never names a behavior (FR-025).
    let behaviors = discovery_report_mod::BehaviorIndex::from_registry(&registry);
    let report = discovery_report_mod::build_queue_report_with_behaviors(&data, &pins, &behaviors);
    let dir = out_dir.unwrap_or_else(|| workspace_root().join("target").join("discovery"));

    match discovery_report_mod::write_queue_report(&dir, &report) {
        Ok(written) => {
            for path in &written {
                println!("{}", path.display());
            }
            eprintln!(
                "wrote {} discovery artifact(s) to {} ({} finding(s): {} untriaged, {} triaged, \
                 {} split, {} promoted, {} no-longer-reproducing, {} pin-stale; {} campaign(s), \
                 {} signature(s) suppressed)",
                written.len(),
                dir.display(),
                report.total,
                report.untriaged.len(),
                report.triaged.len(),
                report.split.len(),
                report.promoted.len(),
                report.no_longer_reproducing.len(),
                report.pin_stale.len(),
                report.campaigns.len(),
                report.signatures_suppressed,
            );
            0
        }
        Err(e) => {
            eprintln!(
                "error: could not write the discovery report to {}: {e}",
                dir.display()
            );
            1
        }
    }
}

/// `discovery triage` (contracts/discovery-cli.md): record a reviewer's classification.
///
/// The **only** writer of `classification`. Interactive-equivalent, never automated: no
/// campaign invokes it, which is what keeps promotion a human act.
///
/// Exit `0` when recorded; `1` on an unknown finding, an invalid classification, or a
/// finding already in a terminal state.
fn discovery_triage(
    registry_dir: &Path,
    finding_id: &str,
    classification: &str,
    notes: Option<&str>,
) -> i32 {
    let Some(classification) = queue::Classification::parse(classification) else {
        let names: Vec<&str> = queue::Classification::all()
            .iter()
            .map(|c| c.as_str())
            .collect();
        eprintln!(
            "error: `{classification}` is not a classification; expected one of: {}",
            names.join(", ")
        );
        return 1;
    };

    let discovery_dir = deacon_conformance::discovery_dir_for(registry_dir);
    let mut data = match load_discovery_data(&discovery_dir) {
        Ok(data) => data,
        Err(failure) => return report_discovery_load_failure(&failure, &discovery_dir),
    };

    let Some(finding) = data.finding_mut(finding_id) else {
        eprintln!(
            "error: no finding `{finding_id}` in {}",
            discovery_dir.display()
        );
        return 1;
    };

    // The state machine owns which states may be classified and which advance
    // (`crates/conformance/src/discovery/queue.rs`, T069). Re-deciding that here would put
    // the lifecycle in two places, and the copy the checker does not read is the one that
    // drifts.
    let previous_state = finding.state;
    let new_state = match finding.triage(classification, notes) {
        Ok(state) => state,
        Err(e) => {
            eprintln!("error: {e}");
            return 1;
        }
    };

    match queue::write_findings(&discovery_dir, &data.findings) {
        Ok(()) => {
            eprintln!(
                "recorded `{}` on {finding_id} in {}",
                classification.as_str(),
                queue::findings_path(&discovery_dir).display()
            );
            if previous_state == new_state {
                eprintln!(
                    "note: {finding_id} stays in state `{}` — only a campaign that \
                     reproduces it may change that",
                    new_state.as_str()
                );
            }
            0
        }
        Err(e) => {
            eprintln!("error: could not write the findings queue: {e}");
            1
        }
    }
}

/// `discovery split` (contracts/discovery-cli.md): split a signature-merged finding whose
/// witnesses turn out to have different causes (FR-032).
///
/// The parent becomes an inert ancestor (invariant Q10): it keeps its witnesses as
/// historical record, stops accepting new ones (enforced by [`queue::upsert_finding`], which
/// refuses the whole lineage, not just this record), and **surrenders its classification to
/// its children** — a split exists precisely because one classification could not describe
/// them all, so a parent that kept one would assert the judgement the split rejected.
///
/// One child per witness, each keyed to `parent ‖ its own witness` and starting untriaged.
/// See [`queue::split_finding`] for why the finest partition is the only one derivable from
/// what the reviewer actually said.
///
/// Exit `0` when the split was written; `1` on an unknown finding, a finding with fewer
/// than two witnesses (there is nothing to split), or a finding in a state the lifecycle
/// does not let a split leave.
fn discovery_split(registry_dir: &Path, finding_id: &str) -> i32 {
    let discovery_dir = deacon_conformance::discovery_dir_for(registry_dir);
    let mut data = match load_discovery_data(&discovery_dir) {
        Ok(data) => data,
        Err(failure) => return report_discovery_load_failure(&failure, &discovery_dir),
    };

    let split = match queue::split_finding(&mut data.findings, finding_id) {
        Ok(split) => split,
        Err(e) => {
            eprintln!("error: {e}");
            if matches!(e, queue::TransitionError::UnknownFinding { .. }) {
                eprintln!("       (searched {})", discovery_dir.display());
            }
            return 1;
        }
    };

    match queue::write_findings(&discovery_dir, &data.findings) {
        Ok(()) => {
            eprintln!(
                "split {finding_id} into {} child finding(s); it is now an inert ancestor \
                 that keeps its witnesses as historical record, accepts no new ones, and \
                 carries no classification of its own.",
                split.children.len()
            );
            for child in &split.children {
                eprintln!("  {child}");
            }
            eprintln!(
                "Each child is untriaged: classify them separately with `discovery triage`. \
                 The split is permanent — the deduplication rule never re-merges this \
                 lineage, so your judgement is not reverted by the next campaign."
            );
            0
        }
        Err(e) => {
            eprintln!("error: could not write the findings queue: {e}");
            1
        }
    }
}

/// `discovery scaffold` (contracts/discovery-cli.md): emit a skeleton for promoting — or,
/// with `--tolerate`, for *tolerating* — a finding, to **stdout only**.
///
/// Writes **nothing**. That is the same discipline as `inventory scaffold` /
/// `clause scaffold`, and here it is what makes FR-036 hold: there is no code path from a
/// finding to a registry write, so a stochastic process cannot author the record it is
/// tested against. Every field a human must decide carries the `UNREVIEWED` sentinel the
/// registry loader rejects, so scaffolded output cannot be committed unedited.
///
/// The skeletons themselves live in `deacon_conformance::discovery::promote`, not here:
/// they are pure `Finding → JSON` transformations, and keeping them in the library is what
/// lets the hermetic guards assert the FR-041 scoping rule without spawning a process. This
/// function is the process boundary and nothing else — load, dispatch, render, explain.
///
/// Exit `0` when emitted; `1` on an unknown finding, a non-promotable classification
/// (`normalizer-defect` / `fixture-defect`, FR-035), or a tolerance that would be blanket
/// or unscoped (FR-041).
fn discovery_scaffold(registry_dir: &Path, finding_id: &str, tolerate: bool) -> i32 {
    use deacon_conformance::discovery::promote;

    let discovery_dir = deacon_conformance::discovery_dir_for(registry_dir);
    let data = match load_discovery_data(&discovery_dir) {
        Ok(data) => data,
        Err(failure) => return report_discovery_load_failure(&failure, &discovery_dir),
    };

    let Some(finding) = data.finding(finding_id) else {
        eprintln!(
            "error: no finding `{finding_id}` in {}",
            discovery_dir.display()
        );
        return 1;
    };

    let document = if tolerate {
        promote::tolerance_skeleton(finding)
    } else {
        promote::promotion_skeleton(finding)
    };
    let document = match document {
        Ok(document) => document,
        Err(e) => {
            eprintln!("error: {e}");
            return 1;
        }
    };

    match serde_json::to_string_pretty(&document) {
        Ok(text) => println!("{text}"),
        Err(e) => {
            eprintln!("error: could not render the scaffold: {e}");
            return 2;
        }
    }

    if tolerate {
        eprintln!(
            "scaffolded a TOLERANCE skeleton for {finding_id}; stdout only — nothing was \
             written. The waiver is scoped to the observable path the difference occurs at, \
             never to the whole channel (FR-041): a bare-channel scope is a global ignore \
             list wearing a waiver id, and it is refused rather than emitted. Fill in \
             `rationale`, `expires`, the behavior, and the binary/fixture the scope names, \
             then add the `allowedDifferences` entry to the case that observes this path. \
             Every `{SCAFFOLD_SENTINEL}` field is rejected by the registry loader, so this \
             cannot be committed unedited. Tolerating records that the difference is \
             ACCEPTABLE — the waiver self-invalidates as stale the moment it stops \
             reproducing, so this is not a way to make a difference go away."
        );
    } else {
        eprintln!(
            "scaffolded a promotion skeleton for {finding_id}; stdout only — nothing was written. \
             Every `{SCAFFOLD_SENTINEL}` field carries the sentinel the registry loader rejects. \
             Author the behavior's three axes by hand: a finding tells you what DIFFERS, not \
             whether deacon is wrong, the reference is wrong, or the spec is silent — that is the \
             review. Then copy the fixture, author the case with a FULL `scenarioContext` (V26), \
             and flip the `odp-cmb-*` dispositions it covers off `gap` in the same commit."
        );
    }
    0
}

// ---------------------------------------------------------------------------
// coverage (024-deterministic-conformance-coverage)
// ---------------------------------------------------------------------------

/// Load a registry for a `coverage` subcommand, reporting schema failures as located
/// lines on stderr. `Err(code)` is the process exit code: `1` for a schema-invalid
/// registry, `2` for an unreadable root (the same split every other command uses).
fn load_for_coverage(registry_dir: &Path) -> Result<Registry, i32> {
    match Registry::load(registry_dir) {
        Ok(registry) => Ok(registry),
        Err(LoadError::Schema(errors)) => {
            for violation in deacon_conformance::validate::schema_violations(&errors) {
                eprintln!(
                    "{} {}: {}",
                    violation.code, violation.record, violation.message
                );
            }
            eprintln!(
                "error: {} is schema-invalid; fix the records before generating obligations",
                registry_dir.display()
            );
            Err(1)
        }
        Err(other) => {
            eprintln!(
                "error: cannot read registry {}: {other}",
                registry_dir.display()
            );
            Err(2)
        }
    }
}

/// `coverage generate` (contracts/coverage-cli.md): regenerate the machine-owned
/// obligation inventory from the scenario model, the applicability rules, the high-risk
/// triples, and the behavior records, then write it atomically.
///
/// Exit `0` on success, `1` on a model-integrity failure (V26) — **reported before any
/// write**, so a broken model never produces a plausible-looking file — or on a
/// regeneration failure, `2` on a write IO error.
///
/// Writes **exactly one** file and never a disposition, case, behavior, waiver, gap, or
/// report (FR-018). That boundary is the 020/021 one, restated because it is the
/// invariant most easily lost: a generator that could edit a disposition would convert
/// human review into a build artifact.
fn coverage_generate(registry_dir: &Path, out: Option<PathBuf>) -> i32 {
    let registry = match load_for_coverage(registry_dir) {
        Ok(registry) => registry,
        Err(code) => return code,
    };

    let model_violations = deacon_conformance::validate::check_scenario_model(&registry);
    if !model_violations.is_empty() {
        for violation in &model_violations {
            eprintln!(
                "{} {}: {}",
                violation.code, violation.record, violation.message
            );
        }
        eprintln!(
            "error: the scenario model has {} integrity violation(s); nothing was written",
            model_violations.len()
        );
        return 1;
    }

    let inventory = match generate_obligations(&registry) {
        Ok(inventory) => inventory,
        Err(e) => {
            eprintln!("error: obligation generation failed: {e}");
            return 1;
        }
    };

    let out_file = out.unwrap_or_else(|| deacon_conformance::obligations_file_for(registry_dir));
    match write_obligations(&out_file, &inventory) {
        Ok(()) => {
            println!("{}", out_file.display());
            let (combinations, behaviors) = obligation_tally(&inventory);
            eprintln!(
                "wrote {} obligation(s) to {} ({combinations} combination, {behaviors} behavior)",
                inventory.units.len(),
                out_file.display()
            );
            0
        }
        Err(e) => {
            eprintln!(
                "error: could not write obligations to {}: {e}",
                out_file.display()
            );
            2
        }
    }
}

/// `(combination, behavior)` unit counts, for the generate/check diagnostics.
fn obligation_tally(inventory: &ObligationInventory) -> (usize, usize) {
    let combinations = inventory
        .units
        .iter()
        .filter(|u| u.kind == ObligationKind::Combination)
        .count();
    (combinations, inventory.units.len() - combinations)
}

/// `coverage check` (contracts/coverage-cli.md): regenerate **in memory** and
/// byte-compare against the committed inventory — the CLI face of the hermetic
/// determinism test.
///
/// Exit `0` when they match, `1` on drift (naming the first differing unit id and whether
/// it was added, removed, or changed) or on a regeneration failure, `2` when the
/// committed file is unreadable.
fn coverage_check(registry_dir: &Path) -> i32 {
    let registry = match load_for_coverage(registry_dir) {
        Ok(registry) => registry,
        Err(code) => return code,
    };

    let regenerated = match generate_obligations(&registry) {
        Ok(inventory) => inventory,
        Err(e) => {
            eprintln!("error: obligation regeneration failed: {e}");
            return 1;
        }
    };

    let obligations_file = deacon_conformance::obligations_file_for(registry_dir);
    let committed_raw = match std::fs::read_to_string(&obligations_file) {
        Ok(raw) => raw,
        Err(e) => {
            eprintln!(
                "error: could not read committed obligations {}: {e}",
                obligations_file.display()
            );
            return 2;
        }
    };

    if committed_raw == render_obligations(&regenerated) {
        eprintln!(
            "ok: {} matches regeneration ({} obligation(s))",
            obligations_file.display(),
            regenerated.units.len()
        );
        return 0;
    }

    match serde_json::from_str::<ObligationInventory>(&committed_raw) {
        Ok(committed) => {
            let drift = compare_obligations(&committed, &regenerated);
            match drift.first_difference() {
                Some((id, how)) => eprintln!(
                    "error: committed obligations are out of date — `{id}` was {how} \
                     (+{} added, -{} removed, ~{} changed); run `coverage generate`",
                    drift.added.len(),
                    drift.removed.len(),
                    drift.changed.len()
                ),
                None => eprintln!(
                    "error: committed obligations differ from a fresh regeneration in formatting \
                     or the revision pin, not in their unit set; run `coverage generate`"
                ),
            }
        }
        Err(e) => eprintln!(
            "error: committed obligations are out of date and unparseable: {}: {e}",
            obligations_file.display()
        ),
    }
    1
}

/// `coverage report` (contracts/coverage-cli.md): write the coverage report families to
/// `target/conformance/` (git-ignored).
///
/// **Read-only** with respect to the record — it never records, refreshes, or repairs
/// evidence (FR-063) — and its exit code never reflects what the report says. Reporting
/// never gates and gating never reports: a command that both measured coverage and
/// decided the build's fate would make widening the report the cheapest way to go green.
///
/// Exit `0` when the reports were written, `1` when the registry or model could not be
/// loaded, `2` on a write IO error.
fn coverage_report_cmd(registry_dir: &Path, _today: &str, out_dir: Option<PathBuf>) -> i32 {
    let registry = match load_for_coverage(registry_dir) {
        Ok(registry) => registry,
        Err(code) => return code,
    };

    let inventory = match generate_obligations(&registry) {
        Ok(inventory) => inventory,
        Err(e) => {
            eprintln!("error: obligation generation failed: {e}");
            return 1;
        }
    };

    let dir = out_dir.unwrap_or_else(|| workspace_root().join("target").join("conformance"));
    let reports = build_coverage_reports(&registry, &inventory);
    match write_coverage_reports(&dir, &reports) {
        Ok(written) => {
            for path in &written {
                println!("{}", path.display());
            }
            eprintln!(
                "wrote {} coverage artifact(s) to {} ({} obligation(s), {} undispositioned)",
                written.len(),
                dir.display(),
                reports.pairwise.summary.valid,
                reports.pairwise.summary.undispositioned
            );
            0
        }
        Err(e) => {
            eprintln!(
                "error: could not write coverage reports to {}: {e}",
                dir.display()
            );
            2
        }
    }
}

/// `coverage scaffold` (contracts/coverage-cli.md, T069): emit skeleton `odp-` records to
/// **stdout** for every undispositioned applicable obligation.
///
/// Mirrors `inventory scaffold` / `clause scaffold` / `migration scaffold`: generation
/// never writes a hand-authored file, and every skeleton carries the `UNREVIEWED`
/// sentinel the loader rejects — twice over. `disposition` carries it so the closed
/// [`DispositionKind`](deacon_conformance::obligation::DispositionKind) enum refuses the
/// record, which forces the author to pick one of the four words rather than accept a
/// default; `rationale` carries it because contracts/coverage-cli.md names that field, and
/// because a record still holding it after the disposition was chosen is the exact shape
/// V29's filler test rejects.
///
/// Exit `0` even when nothing is undispositioned — an empty answer is a valid answer, and
/// making "nothing to do" an error would put the workflow's terminal state in the failure
/// channel.
fn coverage_scaffold(registry_dir: &Path, _today: &str) -> i32 {
    let registry = match load_for_coverage(registry_dir) {
        Ok(registry) => registry,
        Err(code) => return code,
    };
    let inventory = match generate_obligations(&registry) {
        Ok(inventory) => inventory,
        Err(e) => {
            eprintln!("error: obligation generation failed: {e}");
            return 1;
        }
    };

    let audit = deacon_conformance::obligation::audit_dispositions(&registry, &inventory);
    let records: Vec<serde_json::Value> = audit
        .undispositioned
        .iter()
        .map(|unit| {
            serde_json::json!({
                "id": deacon_conformance::obligation::scaffold_disposition_id(&unit.id),
                "obligation": unit.id,
                "disposition": SCAFFOLD_SENTINEL,
                "rationale": SCAFFOLD_SENTINEL,
            })
        })
        .collect();

    let document = serde_json::json!({ "schemaVersion": 1, "records": records });
    match serde_json::to_string_pretty(&document) {
        Ok(text) => println!("{text}"),
        Err(e) => {
            eprintln!("error: could not render the scaffold: {e}");
            return 2;
        }
    }
    eprintln!(
        "scaffolded {} undispositioned obligation(s) ({} inactive-environment obligation(s) \
         excluded — nobody owes them a decision); stdout only — nothing was written. Replace \
         `disposition` with one of `case` / `non-testable` / `waived` / `gap`, and replace \
         `rationale` with the payload that word requires (`cases` / `rationale` / `waiver` / \
         `gap`). Both fields carry the `{SCAFFOLD_SENTINEL}` sentinel the loader rejects.",
        audit.undispositioned.len(),
        audit.inactive.len()
    );
    0
}

/// `snapshot check` (contract runner-cli.md §1): recompute the case/fixture hashes +
/// probe the host environment, compare to the committed provenance, and report `stale`
/// (naming the first drifted field) or `no-reference-for-platform`. Hermetic (never
/// writes). Exit `0` all fresh / no-reference (informational), `1` any stale, `2`
/// malformed input / dangling reference.
///
/// The evidence-determining inputs (case/fixture hashes, oracle + source pins, normalizer
/// version) are recomputed and compared. Host tool versions (Node/Docker/Compose) are
/// informational, NOT staleness signals (see `snapshot::compare_staleness`), so they are
/// neither probed nor compared — a snapshot stays fresh across machines regardless of the
/// host toolchain (SC-003). `imageDigests` is not recomputed hermetically (it needs image
/// inspection) — carried from the recorded provenance.
fn snapshot_check(registry_dir: &Path, case_filter: Option<&str>, platform: Option<&str>) -> i32 {
    let registry = match Registry::load(registry_dir) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("error: cannot load registry: {e}");
            return 2;
        }
    };
    let os_arch = platform
        .map(str::to_string)
        .unwrap_or_else(snapshot::current_os_arch);
    // Snapshots are a sibling of the registry dir (mirrors the certify/validate
    // sibling-resolution), so `snapshot check --registry <dir>` inspects that tree's
    // snapshots rather than always the real committed ones. Fixtures stay
    // workspace-rooted, matching how the runner/refresh/validate resolve them.
    let snapshots_root = registry_dir
        .parent()
        .map(|p| p.join("snapshots"))
        .unwrap_or_else(snapshot::default_snapshots_dir);
    let fixtures_root = workspace_root().join("conformance").join("fixtures");

    // Which declarative cases to check: the named one, or every declarative case.
    let cases: Vec<&deacon_conformance::model::TestCase> = registry
        .cases
        .iter()
        .filter(|c| {
            matches!(
                c.classify(),
                Ok(deacon_conformance::model::CaseKind::Declarative)
            )
        })
        // A default sweep (no --case) checks only `snapshot`-oracle cases — the ones a
        // committed snapshot is expected for. An explicit --case is checked regardless of
        // its oracle type (the user asked for it by id).
        .filter(|c| {
            case_filter.map_or(
                c.oracle_type == Some(deacon_conformance::model::OracleType::Snapshot),
                |id| c.id == id,
            )
        })
        .collect();

    if let Some(id) = case_filter {
        if cases.is_empty() {
            eprintln!("error: no declarative case with id {id:?}");
            return 2;
        }
    }

    let oracle_pin = snapshot::current_oracle_version_pin();
    let mut stale = Vec::new();
    let mut no_reference = Vec::new();
    let mut fresh = 0usize;

    for case in cases {
        let resolution = match snapshot::resolve(&snapshots_root, &os_arch, &case.id) {
            Ok(r) => r,
            Err(e) => {
                eprintln!("error: {e}");
                return 2;
            }
        };
        let recorded = match resolution {
            snapshot::Resolution::NoReferenceForPlatform { os_arch } => {
                no_reference.push((case.id.clone(), os_arch));
                continue;
            }
            snapshot::Resolution::Found(s) => s.provenance,
        };
        let (case_hash, fixture_hash) = match hashes_for_case(case, &fixtures_root) {
            Ok(h) => h,
            Err(e) => {
                eprintln!("error: cannot recompute hashes for {}: {e}", case.id);
                return 2;
            }
        };
        // Build the `current` provenance: recompute the evidence-determining inputs and
        // carry recorded values for the rest (host tool versions are not staleness signals,
        // so they are not probed here; never fabricated).
        let mut current = recorded.clone();
        current.case_hash = case_hash;
        current.fixture_hash = fixture_hash;
        current.source_revision = CURRENT_SPEC_PIN.to_string();
        current.normalizer_version = snapshot::NORMALIZER_VERSION.to_string();
        if let Some(v) = &oracle_pin {
            current.oracle_version = v.clone();
        }

        match snapshot::compare_staleness(&recorded, &current) {
            snapshot::Staleness::Fresh => {
                println!("fresh {}", case.id);
                fresh += 1;
            }
            snapshot::Staleness::Stale {
                field,
                recorded,
                current,
            } => {
                println!(
                    "stale {} {field}: recorded {recorded:?} != current {current:?}",
                    case.id
                );
                stale.push(case.id.clone());
            }
        }
    }

    for (id, os_arch) in &no_reference {
        println!("no-reference-for-platform {id} ({os_arch})");
    }
    eprintln!(
        "snapshot check ({os_arch}): {fresh} fresh, {} stale, {} no-reference",
        stale.len(),
        no_reference.len()
    );
    if stale.is_empty() { 0 } else { 1 }
}

/// `snapshot diff <old-dir> <new-dir>` (contract runner-cli.md §1): deterministic drift
/// between two committed snapshot trees (a single case directory each). Exit `0` on
/// success (whether or not differences exist — the diff IS the output), `2` on IO error.
fn snapshot_diff(old: &Path, new: &Path, format: DiffFormat) -> i32 {
    let old_snap = match snapshot::load_snapshot(old) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("error: cannot load old snapshot: {e}");
            return 2;
        }
    };
    let new_snap = match snapshot::load_snapshot(new) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("error: cannot load new snapshot: {e}");
            return 2;
        }
    };
    let entries = snapshot::diff(&old_snap, &new_snap);
    match format {
        DiffFormat::Json => match serde_json::to_string_pretty(&entries) {
            Ok(doc) => println!("{doc}"),
            Err(e) => {
                eprintln!("error: could not serialize diff: {e}");
                return 2;
            }
        },
        DiffFormat::Md => {
            if entries.is_empty() {
                println!("No snapshot differences.");
            } else {
                println!("| Artifact | Path | Old | New |");
                println!("|----------|------|-----|-----|");
                for e in &entries {
                    println!(
                        "| {} | `{}` | `{}` | `{}` |",
                        e.artifact, e.path, e.old, e.new
                    );
                }
            }
        }
    }
    0
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

// ---------------------------------------------------------------------------
// baseline (023-migrate-parity-to-conformance)
// ---------------------------------------------------------------------------

/// Compute the conservation report over the registry at `registry_dir`.
///
/// The equivalence ledger is a LIVE artifact produced only under the parity profile; it
/// is read when present and its ABSENCE is reported as such, never treated as "every
/// unit is equivalent" (that conflation is how a more-permissive replacement gets
/// deleted into).
fn compute_migration_report(
    registry_dir: &Path,
    ledger_path: Option<PathBuf>,
) -> Result<deacon_conformance::conservation::MigrationReport, i32> {
    let registry = match Registry::load(registry_dir) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("error: could not load {}: {e}", registry_dir.display());
            return Err(2);
        }
    };
    let fixtures_root = registry_dir
        .parent()
        .unwrap_or(registry_dir)
        .join("fixtures");
    let ledger = ledger_path.and_then(|path| load_equivalence_facts(&path));

    match conservation::migration_report(&registry, &fixtures_root, ledger.as_ref()) {
        Ok(report) => Ok(report),
        Err(e) => {
            eprintln!("error: {e}");
            Err(2)
        }
    }
}

/// Read `target/parity/equivalence.json` when it exists. A missing ledger yields `None`
/// (nothing has been proven yet); a malformed one is reported and also yields `None`,
/// because a ledger we cannot read has proven nothing either.
fn load_equivalence_facts(path: &Path) -> Option<conservation::EquivalenceFacts> {
    let raw = match std::fs::read_to_string(path) {
        Ok(raw) => raw,
        Err(e) => {
            eprintln!(
                "warning: could not read the equivalence ledger {} ({e}); nothing is \
                 proven equivalent",
                path.display()
            );
            return None;
        }
    };
    let doc: serde_json::Value = match serde_json::from_str(&raw) {
        Ok(v) => v,
        Err(e) => {
            eprintln!(
                "warning: {} is malformed ({e}); treating the ledger as absent — nothing \
                 is proven equivalent",
                path.display()
            );
            return None;
        }
    };
    let mut facts = conservation::EquivalenceFacts::default();
    for entry in doc.get("entries").and_then(|v| v.as_array())? {
        let Some(unit) = entry.get("unit").and_then(|v| v.as_str()) else {
            continue;
        };
        match entry.get("relation").and_then(|v| v.as_str()) {
            Some("equivalent") => {
                facts.cleared.insert(unit.to_string());
            }
            Some("stricter") => {
                // A `stricter` relation permits deletion ONLY once its newly detected
                // difference is characterized (FR-036). An uncharacterized one is
                // suppression, so it does not clear the unit — the report then reports it
                // under failure condition 7 AND leaves the carrier blocked, rather than
                // the two disagreeing.
                if entry
                    .get("characterizedAs")
                    .and_then(|v| v.as_str())
                    .is_some_and(|c| !c.trim().is_empty())
                {
                    facts.cleared.insert(unit.to_string());
                }
                facts.stricter.push((
                    unit.to_string(),
                    entry
                        .get("detail")
                        .and_then(|v| v.as_str())
                        .unwrap_or_default()
                        .to_string(),
                    entry
                        .get("characterizedAs")
                        .and_then(|v| v.as_str())
                        .map(str::to_string),
                ));
            }
            Some("more-permissive") => {
                facts.more_permissive.insert(unit.to_string());
            }
            _ => {}
        }
    }
    Some(facts)
}

/// `migration report` (contracts/cli-commands.md): write the deterministic accounting to
/// `target/conformance/` and emit the document on stdout.
///
/// Exit `0` only when every baseline item is accounted for; `1` with the violations
/// otherwise; `2` on an IO/load failure. Diagnostics go to stderr, the document to
/// stdout (constitution VI).
fn migration_report_cmd(
    registry_dir: &Path,
    format: DiffFormat,
    out_dir: Option<PathBuf>,
    ledger: Option<PathBuf>,
) -> i32 {
    let report = match compute_migration_report(registry_dir, ledger) {
        Ok(r) => r,
        Err(code) => return code,
    };

    let dir = out_dir.unwrap_or_else(|| workspace_root().join("target").join("conformance"));
    if let Err(e) = std::fs::create_dir_all(&dir) {
        eprintln!("error: could not create {}: {e}", dir.display());
        return 2;
    }
    let (json_path, md_path) = match conservation::write_reports(&dir, &report) {
        Ok(paths) => paths,
        Err(e) => {
            eprintln!(
                "error: could not write the report into {}: {e}",
                dir.display()
            );
            return 2;
        }
    };

    match format {
        DiffFormat::Json => print!("{}", conservation::render_report_json(&report)),
        DiffFormat::Md => print!("{}", conservation::render_report_md(&report)),
    }

    eprintln!("wrote {} and {}", json_path.display(), md_path.display());
    report_accounting_summary(&report);
    if report.is_clean() { 0 } else { 1 }
}

/// `migration check` (contracts/cli-commands.md): the gating form — the same computation,
/// NO file output, failing while naming each item.
fn migration_check(registry_dir: &Path, ledger: Option<PathBuf>) -> i32 {
    let report = match compute_migration_report(registry_dir, ledger) {
        Ok(r) => r,
        Err(code) => return code,
    };
    for violation in &report.violations {
        println!(
            "condition {} {}: {}",
            violation.condition, violation.item, violation.message
        );
    }
    report_accounting_summary(&report);
    if report.is_clean() { 0 } else { 1 }
}

/// The one-line accounting summary both commands emit on stderr.
fn report_accounting_summary(report: &deacon_conformance::conservation::MigrationReport) {
    let acc = &report.accounting;
    eprintln!(
        "conservation: {} unit(s) — migrated {}, deduplicated {}, residual {}, retired {}, \
         unaccounted {}; error paths {}/{} preserved; {} violation(s)",
        report.totals.before.units,
        acc.migrated,
        acc.deduplicated,
        acc.residual,
        acc.retired,
        acc.unaccounted.len(),
        report.error_paths.preserved,
        report.error_paths.before,
        report.violations.len()
    );
}

/// `migration scaffold` (contracts/cli-commands.md): emit skeleton mapping/residual/
/// exception records to **stdout** for everything not yet mapped. Mirrors `inventory
/// scaffold` / `clause scaffold` — it NEVER writes into the registry, and every emitted
/// record carries the `UNREVIEWED` sentinel the loader rejects, so scaffolded output
/// cannot be committed unedited.
///
/// Exit `0` when something was emitted or nothing needs scaffolding, `2` when the
/// baseline or registry cannot be read.
fn migration_scaffold(registry_dir: &Path, baseline_file: Option<PathBuf>) -> i32 {
    let committed_file = resolve_baseline_file(registry_dir, baseline_file);
    let baseline = match load_baseline(&committed_file) {
        Ok(Some(b)) => b,
        Ok(None) => {
            eprintln!(
                "error: no committed baseline at {} — run `baseline generate --freeze \
                 <sha>` first; there is nothing to scaffold against.",
                committed_file.display()
            );
            return 2;
        }
        Err(e) => {
            eprintln!("error: {e}");
            return 2;
        }
    };

    let registry = match Registry::load(registry_dir) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("error: could not load {}: {e}", registry_dir.display());
            return 2;
        }
    };

    let mapped: HashSet<&str> = registry.mapping.iter().map(|m| m.unit.as_str()).collect();
    let mut unmapped: Vec<&deacon_conformance::baseline::BaselineUnit> = baseline
        .records
        .iter()
        .filter(|u| !mapped.contains(u.id.as_str()))
        .collect();
    unmapped.sort_by(|a, b| a.id.cmp(&b.id));

    // One skeleton mapping record per unmapped unit. `disposition` carries the sentinel:
    // the closed `Disposition` enum rejects it at load, so this can never be committed.
    let records: Vec<serde_json::Value> = unmapped
        .iter()
        .map(|unit| {
            serde_json::json!({
                "unit": unit.id,
                "disposition": SCAFFOLD_SENTINEL,
                "caseIds": [SCAFFOLD_SENTINEL],
                "rationale": SCAFFOLD_SENTINEL,
                "fixtureMapping": unit
                    .fixtures
                    .iter()
                    .map(|from| serde_json::json!({ "from": from, "to": SCAFFOLD_SENTINEL }))
                    .collect::<Vec<_>>(),
            })
        })
        .collect();

    // One skeleton residual per program that still has unmapped units — the shape a
    // reviewer fills in when the unit cannot be expressed as data.
    let mut by_program: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for unit in &unmapped {
        by_program
            .entry(unit.program.as_str())
            .or_default()
            .push(unit.id.as_str());
    }
    let residuals: Vec<serde_json::Value> = by_program
        .iter()
        .map(|(program, units)| {
            serde_json::json!({
                "id": format!("res-{program}-UNREVIEWED"),
                "units": units,
                "blockedCarrier": program,
                "missingCapability": SCAFFOLD_SENTINEL,
                "followUp": SCAFFOLD_SENTINEL,
                "behaviors": [],
            })
        })
        .collect();

    // Skeleton exception correspondences for every unmapped characterized exception.
    let mapped_exceptions: HashSet<&str> = registry
        .mapping_exceptions
        .iter()
        .map(|e| e.exception.as_str())
        .collect();
    let mut known_exceptions: Vec<&str> = registry
        .waivers
        .iter()
        .map(|w| w.id.as_str())
        .chain(registry.extensions.iter().map(|e| e.id.as_str()))
        .filter(|id| !mapped_exceptions.contains(id))
        .collect();
    known_exceptions.sort_unstable();
    let exceptions: Vec<serde_json::Value> = known_exceptions
        .iter()
        .map(|id| {
            serde_json::json!({
                "exception": id,
                "disposition": SCAFFOLD_SENTINEL,
                "mechanisms": [SCAFFOLD_SENTINEL],
                "preservedDirection": SCAFFOLD_SENTINEL,
                "preservedScope": SCAFFOLD_SENTINEL,
                "rationale": SCAFFOLD_SENTINEL,
            })
        })
        .collect();

    let document = serde_json::json!({
        "mapping": { "records": records, "exceptions": exceptions },
        "residuals": { "records": residuals },
    });
    match serde_json::to_string_pretty(&document) {
        Ok(text) => println!("{text}"),
        Err(e) => {
            eprintln!("error: could not render the scaffold: {e}");
            return 2;
        }
    }
    eprintln!(
        "scaffolded {} unmapped unit(s) across {} program(s) and {} unmapped exception(s); \
         stdout only — nothing was written. Every field carries the `{SCAFFOLD_SENTINEL}` \
         sentinel the loader rejects.",
        unmapped.len(),
        by_program.len(),
        exceptions.len()
    );
    0
}

/// Resolve the committed baseline path: the explicit `--out`/`--baseline` flag, else
/// the migration sibling of the registry directory.
fn resolve_baseline_file(registry_dir: &Path, explicit: Option<PathBuf>) -> PathBuf {
    explicit.unwrap_or_else(|| migration_paths_for(registry_dir).0)
}

/// The freeze sentinel recorded when `--freeze` is omitted and no committed baseline
/// exists. A baseline carrying it is explicitly NOT frozen, so `--force` is not needed
/// to regenerate over it. (`validate` no longer reports drift at all — V25 is retired;
/// `baseline check` reports it informationally.)
use deacon_conformance::baseline::UNFROZEN_REVISION as UNFROZEN;

/// `baseline generate` (contracts/cli-commands.md): enumerate the pre-migration
/// inventory and write it atomically. Exit `0` on write, `1` on an enumeration failure
/// or a refused overwrite, `2` on an IO failure.
fn baseline_generate(
    registry_dir: &Path,
    freeze: Option<String>,
    force: bool,
    out: Option<PathBuf>,
    repo_root: Option<PathBuf>,
) -> i32 {
    let out_file = resolve_baseline_file(registry_dir, out);
    let root = repo_root.unwrap_or_else(workspace_root);

    let committed = match load_baseline(&out_file) {
        Ok(existing) => existing,
        Err(e) => {
            eprintln!("error: {e}");
            return 2;
        }
    };

    // FR-045: a frozen baseline is the bar the conservation claim is measured against.
    // Re-running must never silently lower it.
    if let Some(existing) = &committed {
        let frozen = existing.revision != UNFROZEN;
        if frozen && !force {
            eprintln!(
                "error: {} is frozen at revision `{}`. Refusing to overwrite it — the \
                 baseline is the bar \"no coverage was lost\" is measured against \
                 (FR-045). Remedy: pass --force if the overwrite is deliberate and \
                 reviewed.",
                out_file.display(),
                existing.revision
            );
            return 1;
        }
    }

    let revision = freeze
        .or_else(|| committed.as_ref().map(|b| b.revision.clone()))
        .unwrap_or_else(|| UNFROZEN.to_string());

    let baseline = match generate_baseline(&root, &revision) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("error: baseline enumeration failed: {e}");
            return 1;
        }
    };

    match write_baseline(&out_file, &baseline) {
        Ok(()) => {
            println!("{}", out_file.display());
            eprintln!(
                "wrote {} baseline unit(s) ({} executable + {} recorded-only) at revision \
                 `{}` to {}",
                baseline.records.len(),
                baseline.executable_count(),
                baseline.count(deacon_conformance::baseline::UnitCategory::ExternalCorpusEntry),
                baseline.revision,
                out_file.display()
            );
            0
        }
        Err(e) => {
            eprintln!(
                "error: could not write baseline to {}: {e}",
                out_file.display()
            );
            2
        }
    }
}

/// `baseline check` (contracts/cli-commands.md): recompute in memory and byte-compare
/// against the committed file. NEVER writes. Exit `0` on match, `1` naming each drifted
/// item, `2` when the committed file is absent or unreadable.
fn baseline_check(
    registry_dir: &Path,
    baseline_file: Option<PathBuf>,
    freeze: Option<String>,
    repo_root: Option<PathBuf>,
) -> i32 {
    let committed_file = resolve_baseline_file(registry_dir, baseline_file);
    let root = repo_root.unwrap_or_else(workspace_root);

    let committed_raw = match std::fs::read_to_string(&committed_file) {
        Ok(raw) => raw,
        Err(e) => {
            eprintln!(
                "error: could not read committed baseline {}: {e}. Remedy: run \
                 `baseline generate --freeze <sha>` and commit the result.",
                committed_file.display()
            );
            return 2;
        }
    };
    let committed: BaselineFile = match serde_json::from_str(&committed_raw) {
        Ok(b) => b,
        Err(e) => {
            eprintln!(
                "error: committed baseline {} is malformed: {e}",
                committed_file.display()
            );
            return 2;
        }
    };

    // Without `--freeze`, the regeneration adopts the committed freeze label so only the
    // unit records are compared; with it, a tampered label is reported as drift too.
    let expected_revision = freeze.unwrap_or_else(|| committed.revision.clone());
    let regenerated = match generate_baseline(&root, &expected_revision) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("error: baseline regeneration failed: {e}");
            return 1;
        }
    };

    if committed_raw == baseline::render(&regenerated) {
        eprintln!(
            "ok: {} matches regeneration ({} unit(s))",
            committed_file.display(),
            committed.records.len()
        );
        return 0;
    }

    report_baseline_drift(
        &committed_file,
        &compare_baselines(&committed, &regenerated),
    );
    1
}

/// Thin alias keeping the drift comparison's origin explicit at the call site.
fn compare_baselines(committed: &BaselineFile, regenerated: &BaselineFile) -> BaselineDrift {
    baseline::compare(committed, regenerated)
}

/// Print the drift, naming each added, removed, or changed unit (FR-004). Diagnostics go
/// to stderr per the output-stream contract.
fn report_baseline_drift(path: &Path, drift: &BaselineDrift) {
    eprintln!(
        "error: {} is out of date with respect to the repository tree.",
        path.display()
    );
    let lines = drift.lines();
    if lines.is_empty() {
        // Byte-difference with no record-level drift: formatting or envelope only.
        eprintln!(
            "  the record set matches but the file bytes differ (envelope or formatting \
             drift) — regenerate to normalize"
        );
    }
    for line in lines {
        eprintln!("  {line}");
    }
    eprintln!(
        "Remedy: `cargo run -p deacon-conformance -- baseline generate --force` and review \
         the diff — a baseline change must be a conscious, reviewed act."
    );
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

// ---------------------------------------------------------------------------
// 026-continuous-conformance-certification — lane, drift, and certification report
// ---------------------------------------------------------------------------

/// Assemble the execution evidence certification needs (research D3).
///
/// The required case set is derived, not authored: it is exactly the declarative cases
/// whose `resourceGroup` puts them in a container lane. A hand-authored list here would
/// let a newly added Docker case go unexecuted while certification still reported the
/// manifest complete.
fn build_evidence_inputs(
    registry: &Registry,
    registry_dir: &Path,
    manifest: Option<PathBuf>,
) -> EvidenceInputs {
    use deacon_conformance::lane::case_lane_membership;

    // Fixtures are a sibling of the registry, matching the root the reviewed refresh and
    // `snapshot check` use — three callers, one answer about what a case's inputs are.
    let fixtures_root = registry_dir
        .parent()
        .map(|p| p.join("fixtures"))
        .unwrap_or_else(|| registry_dir.join("fixtures"));

    let manifest_path = manifest.unwrap_or_else(|| {
        workspace_root()
            .join("target")
            .join("conformance")
            .join("execution-manifest.json")
    });

    let mut required_cases = Vec::new();
    let mut applicable_units = std::collections::BTreeSet::new();
    for case in &registry.cases {
        let Some(membership) = case_lane_membership(case) else {
            continue;
        };
        if !membership.needs_container || membership.needs_oracle {
            // Only the oracle-free container tier is the pull-request lane's obligation;
            // live-differential cases are the nightly lane's, and it never gates a release.
            continue;
        }
        // A case whose hashes cannot be computed is skipped rather than defaulted: a
        // fabricated hash would compare unequal and report a stale manifest for a case
        // that is merely unreadable, which sends the reader after the wrong problem.
        let Ok((case_hash, fixture_hash)) = hashes_for_case(case, &fixtures_root) else {
            continue;
        };
        required_cases.push(RequiredCase {
            case_id: case.id.clone(),
            case_hash,
            fixture_hash,
        });
        applicable_units.insert(case.id.clone());
    }

    // The manifest's own cases are folded into the accounted set by
    // `certify_with_evidence`, which already holds the parsed document — so this stays
    // empty rather than becoming a second reader of one file. It would carry the hermetic
    // replay lane's results if this command drove that lane too.
    let accounted_units = std::collections::BTreeSet::new();
    let mut recorded_oracles = Vec::new();

    // Oracle identity recorded in committed snapshot provenance (FR-041(g)). Reading the
    // provenance is the whole check: certification never runs the reference to find out.
    let snapshots_dir = registry_dir
        .parent()
        .map(|p| p.join("snapshots"))
        .unwrap_or_else(|| registry_dir.join("snapshots"));
    // **Scoped to the platform under certification** (research D5, and the scoping
    // `BlockingKind::StaleSnapshot` documents). 022 made committed-snapshot coverage
    // non-blocking on the ground that a snapshot is a reviewed artifact, not a release
    // gate, and that reasoning still holds for platforms nobody is certifying. Walking
    // every `<os-arch>` directory made a snapshot recorded on some other platform block
    // this release — pressure to record snapshots to go green, which is exactly what
    // FR-055 exists to remove.
    let certified_os_arch = std::env::var("DEACON_CERTIFY_PLATFORM")
        .ok()
        .zip(std::env::var("DEACON_CERTIFY_ARCH").ok())
        .map(|(os, arch)| format!("{os}-{arch}"))
        .unwrap_or_else(deacon_conformance::snapshot::current_os_arch);
    let mut stale_snapshots = Vec::new();
    if let Ok(cases) = std::fs::read_dir(snapshots_dir.join(&certified_os_arch)) {
        for case in cases.flatten() {
            let provenance_path = case.path().join("provenance.json");
            let Ok(raw) = std::fs::read_to_string(&provenance_path) else {
                continue;
            };
            let Ok(provenance) =
                serde_json::from_str::<deacon_conformance::snapshot::Provenance>(&raw)
            else {
                continue;
            };
            let case_id = case.file_name().to_string_lossy().to_string();
            recorded_oracles.push((
                format!("snapshot:{case_id}"),
                provenance.oracle_version.clone(),
            ));
            if let Some(registry_case) = registry.cases.iter().find(|c| c.id == case_id) {
                let Ok((case_hash, fixture_hash)) = hashes_for_case(registry_case, &fixtures_root)
                else {
                    continue;
                };
                if provenance.case_hash != case_hash {
                    stale_snapshots.push((case_id.clone(), "caseHash".to_string()));
                } else if provenance.fixture_hash != fixture_hash {
                    stale_snapshots.push((case_id.clone(), "fixtureHash".to_string()));
                }
            }
        }
    }
    stale_snapshots.sort();
    recorded_oracles.sort();

    let profile_is_active = registry.profiles.iter().any(|p| p.active);

    EvidenceInputs {
        revision: std::env::var("DEACON_CERTIFY_REVISION").unwrap_or_default(),
        manifest_path: Some(manifest_path),
        required_cases,
        resolvable_dispositions: registry
            .obligation_dispositions
            .iter()
            .map(|d| d.id.clone())
            .collect(),
        stale_snapshots,
        recorded_oracles,
        applicable_units,
        accounted_units,
        profile_is_active,
    }
}

/// Write `certification.{json,md}` atomically into `dir`.
fn write_certification_report(
    result: &deacon_conformance::certify::Certification,
    registry: &Registry,
    registry_dir: &Path,
    today: &str,
    dir: &Path,
) -> Result<(), i32> {
    let (schemas_dir, _) = inventory_paths_for(registry_dir);
    let (spec_dir, _) = clause_paths_for(registry_dir);
    let inputs = ReportInputs {
        // From the build environment, never from a `git` call at report time — a report
        // that shells out is not byte-reproducible across machines (FR-036).
        deacon_revision: std::env::var("DEACON_CERTIFY_REVISION").unwrap_or_default(),
        environment: report_environment(),
        // From `--today`, never the clock, so the same inputs always yield the same bytes.
        evaluation_date: today.to_string(),
        schema_documents: count_manifest_documents(&schemas_dir),
        prose_documents: count_manifest_documents(&spec_dir),
        admitted_non_deterministic_inputs: Vec::new(),
    };
    let report = build_report(result, registry, &inputs);
    let json_path = dir.join("certification.json");
    let md_path = dir.join("certification.md");
    let rendered = match render_certification_json(&report) {
        Ok(text) => text,
        Err(e) => {
            eprintln!("error: could not serialize the certification report: {e}");
            return Err(2);
        }
    };
    if let Err(e) = atomic_write(&json_path, &rendered) {
        eprintln!("error: could not write {}: {e}", json_path.display());
        return Err(2);
    }
    if let Err(e) = atomic_write(&md_path, &render_certification_md(&report)) {
        eprintln!("error: could not write {}: {e}", md_path.display());
        return Err(2);
    }
    Ok(())
}

/// The environment identity the report records.
///
/// Read from the environment the certifying job declares rather than probed: certification
/// must run with no container engine present (SC-013), so probing one would make the gate
/// depend on the capability it is designed not to need.
///
/// **The fallbacks never invent an identity.** `platform`/`arch` fall back to the build
/// target, which is true by construction; the engine falls back to `"unknown"`, never to
/// `"docker"`. A hard-coded `linux`/`x86_64`/`docker` default made a report produced on any
/// other host claim a combination that was never exercised — and `scope.doesNotCertify` is
/// computed from these values, so the fabrication propagated into the statement of what the
/// certification does *not* cover.
fn report_environment() -> ReportEnvironment {
    ReportEnvironment {
        platform: std::env::var("DEACON_CERTIFY_PLATFORM")
            .unwrap_or_else(|_| std::env::consts::OS.to_string()),
        arch: std::env::var("DEACON_CERTIFY_ARCH")
            .unwrap_or_else(|_| std::env::consts::ARCH.to_string()),
        container_engine: std::env::var("DEACON_CERTIFY_ENGINE")
            .unwrap_or_else(|_| "unknown".into()),
        container_engine_version: std::env::var("DEACON_CERTIFY_ENGINE_VERSION")
            .unwrap_or_else(|_| "unknown".into()),
        compose_version: std::env::var("DEACON_CERTIFY_COMPOSE_VERSION")
            .unwrap_or_else(|_| "unknown".into()),
    }
}

/// Count the documents a vendored `manifest.json` pins.
fn count_manifest_documents(dir: &Path) -> usize {
    let Ok(raw) = std::fs::read_to_string(dir.join("manifest.json")) else {
        return 0;
    };
    let Ok(doc) = serde_json::from_str::<serde_json::Value>(&raw) else {
        return 0;
    };
    doc.get("documents")
        .and_then(|d| d.as_object())
        .map(|m| m.len())
        .unwrap_or(0)
}

/// `lane check` — V34 over the lane root (contracts/cli-lane.md).
fn lane_check(registry_dir: &Path, json: bool) -> i32 {
    let registry = match Registry::load(registry_dir) {
        Ok(registry) => registry,
        Err(e) => {
            eprintln!("error: {e}");
            return 2;
        }
    };
    let lanes_dir = lanes_dir_for(registry_dir);
    let lanes = match load_lanes(&lanes_dir) {
        Ok(lanes) => lanes,
        Err(e) => {
            eprintln!("error: {e}");
            return 2;
        }
    };
    let (units, memberships) = lane_inputs(&registry, registry_dir);
    // The profile-drift sub-case runs here too. Passing `None` skipped it, so `lane check`
    // reported clean on data `validate` rejected — two checkers of one rule disagreeing is
    // worse than either one being absent.
    let profiles = deacon_conformance::validate::parse_nextest_profiles(
        &workspace_root().join(".config").join("nextest.toml"),
    );
    let violations = check_lanes(&lanes, &units, &memberships, profiles.as_ref());

    if json {
        let doc = serde_json::json!({ "ok": violations.is_empty(), "violations": violations });
        match serde_json::to_string_pretty(&doc) {
            Ok(text) => println!("{text}"),
            Err(e) => {
                eprintln!("error: could not serialize violations: {e}");
                return 2;
            }
        }
    } else {
        for violation in &violations {
            println!(
                "{} {}: {}",
                violation.code, violation.record, violation.message
            );
        }
    }
    i32::from(!violations.is_empty())
}

/// `lane report` — writes `target/conformance/lanes.{json,md}`.
///
/// **Reporting never gates.** The exit code reflects whether the report could be written,
/// never what it says: a reporting command whose status depends on its content becomes a
/// gate the moment someone wires it into a required check.
fn lane_report(registry_dir: &Path, out_dir: Option<PathBuf>) -> i32 {
    let registry = match Registry::load(registry_dir) {
        Ok(registry) => registry,
        Err(e) => {
            eprintln!("error: {e}");
            return 2;
        }
    };
    let lanes = match load_lanes(&lanes_dir_for(registry_dir)) {
        Ok(lanes) => lanes,
        Err(e) => {
            eprintln!("error: {e}");
            return 2;
        }
    };
    let (units, memberships) = lane_inputs(&registry, registry_dir);
    let report = build_lane_report(&lanes, &units, &memberships);
    let dir = out_dir.unwrap_or_else(|| workspace_root().join("target").join("conformance"));
    let json = match serde_json::to_string_pretty(&report) {
        Ok(mut text) => {
            text.push('\n');
            text
        }
        Err(e) => {
            eprintln!("error: could not serialize lane report: {e}");
            return 2;
        }
    };
    if let Err(e) = atomic_write(&dir.join("lanes.json"), &json) {
        eprintln!("error: could not write lanes.json: {e}");
        return 2;
    }
    if let Err(e) = atomic_write(&dir.join("lanes.md"), &render_lane_report_md(&report)) {
        eprintln!("error: could not write lanes.md: {e}");
        return 2;
    }
    0
}

/// `lane scaffold` — stdout only, never writes.
fn lane_scaffold(for_unit: &str) -> i32 {
    match serde_json::to_string_pretty(&scaffold_lane(for_unit)) {
        Ok(text) => {
            println!("{text}");
            0
        }
        Err(e) => {
            eprintln!("error: could not render scaffold: {e}");
            2
        }
    }
}

/// The derived denominator and case memberships a lane command needs.
fn lane_inputs(
    registry: &Registry,
    registry_dir: &Path,
) -> (
    Vec<ExecutionUnit>,
    std::collections::BTreeMap<String, CaseMembership>,
) {
    let tests_dir = workspace_root().join("crates").join("deacon").join("tests");
    let snapshots_dir = registry_dir
        .parent()
        .map(|p| p.join("snapshots"))
        .unwrap_or_else(|| registry_dir.join("snapshots"));
    (
        derive_execution_units(registry, &tests_dir, &snapshots_dir),
        case_memberships(registry),
    )
}

/// `drift check` — V36 over the committed observations.
fn drift_check(registry_dir: &Path, json: bool) -> i32 {
    let file = match load_drift(&drift_dir_for(registry_dir)) {
        Ok(file) => file,
        Err(e) => {
            eprintln!("error: {e}");
            return 2;
        }
    };
    let violations = check_drift(&file);
    if json {
        let doc = serde_json::json!({ "ok": violations.is_empty(), "violations": violations });
        match serde_json::to_string_pretty(&doc) {
            Ok(text) => println!("{text}"),
            Err(e) => {
                eprintln!("error: could not serialize violations: {e}");
                return 2;
            }
        }
    } else {
        for violation in &violations {
            println!(
                "{} {}: {}",
                violation.code, violation.record, violation.message
            );
        }
    }
    i32::from(!violations.is_empty())
}

/// `drift report` — writes `target/drift/observations.{json,md}`. Never gates.
fn drift_report(registry_dir: &Path, out_dir: Option<PathBuf>) -> i32 {
    let file = match load_drift(&drift_dir_for(registry_dir)) {
        Ok(file) => file,
        Err(e) => {
            eprintln!("error: {e}");
            return 2;
        }
    };
    let dir = out_dir.unwrap_or_else(|| workspace_root().join("target").join("drift"));
    let json = match serde_json::to_string_pretty(&file) {
        Ok(mut text) => {
            text.push('\n');
            text
        }
        Err(e) => {
            eprintln!("error: could not serialize drift observations: {e}");
            return 2;
        }
    };
    if let Err(e) = atomic_write(&dir.join("observations.json"), &json) {
        eprintln!("error: could not write observations.json: {e}");
        return 2;
    }

    // The report states WHICH of the two empty-list meanings applies. Printing an empty
    // table either way is what FR-025 exists to prevent.
    let mut md = String::from("# Upstream drift observations\n\n");
    if drift_state_is_known(&file) {
        md.push_str("Drift detection has completed over every source kind.\n\n");
    } else {
        md.push_str(
            "**Drift detection has NOT completed over every source kind.** An empty list \
             below means *not looked at*, not *nothing found*.\n\n",
        );
    }
    if file.records.is_empty() {
        md.push_str("No drift observations recorded.\n");
    } else {
        md.push_str("| Kind | Pinned | Observed | Affected |\n|---|---|---|---|\n");
        for record in &file.records {
            md.push_str(&format!(
                "| {} | `{}` | `{}` | {} |\n",
                record.kind.as_str(),
                record.pinned_revision,
                record.observed_revision,
                record.affected_surfaces.len()
            ));
        }
    }
    if let Err(e) = atomic_write(&dir.join("observations.md"), &md) {
        eprintln!("error: could not write observations.md: {e}");
        return 2;
    }
    0
}

/// `drift scaffold` — stdout only, `UNREVIEWED` sentinels the loader rejects.
fn drift_scaffold() -> i32 {
    let skeleton = serde_json::json!({
        "id": "UNREVIEWED",
        "kind": "UNREVIEWED",
        "pinnedRevision": "UNREVIEWED",
        "observedRevision": "UNREVIEWED",
        "affectedSurfaces": [],
        "observedAt": "UNREVIEWED",
        "reviewArtifact": "UNREVIEWED"
    });
    match serde_json::to_string_pretty(&skeleton) {
        Ok(text) => {
            println!("{text}");
            0
        }
        Err(e) => {
            eprintln!("error: could not render scaffold: {e}");
            2
        }
    }
}

/// `drift proposal check` — completeness and determinism of an upgrade bundle.
///
/// A bundle missing any of the seven section keys never reaches the checks below: the
/// type requires all seven, so `serde` rejects the document (FR-030). That is the point of
/// modelling the sections as named fields rather than a map — an unrun analysis cannot be
/// represented as a clean one.
fn drift_proposal_check(path: &Path) -> i32 {
    let proposal = match load_proposal(path) {
        Ok(proposal) => proposal,
        Err(e) => {
            eprintln!("error: {e}");
            return 1;
        }
    };
    let violations = check_proposal(&proposal);
    for violation in &violations {
        println!(
            "{} {}: {}",
            violation.code, violation.record, violation.message
        );
    }
    i32::from(!violations.is_empty())
}

/// `certify` (contracts/cli.md + contracts/cli-inventory.md): validate first (invalid
/// → exit 1), then evaluate strict certification — including the schema-constraint
/// inventory join (V11–V14), which blocks exactly as gaps/uncovered behaviors do. Exit
/// `0` certified, `1` not certified (blocking items listed) or registry invalid, `2`
/// usage/IO. The committed inventory + vendored schemas are resolved as siblings of the
/// registry dir (mirroring `validate`); a fixture registry that ships neither scopes the
/// V11–V14 join out, so certification reduces to the gap/uncovered gate.
fn certify_cmd(
    registry_dir: &Path,
    today: &str,
    json: bool,
    report_dir: Option<PathBuf>,
    manifest: Option<PathBuf>,
) -> i32 {
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
    // Committed snapshots are a sibling of the registry dir (mirrors the inventory/clause
    // sibling resolution); snapshot coverage is NON-BLOCKING info (T073).
    let snapshots_dir = registry_dir
        .parent()
        .map(|p| p.join("snapshots"))
        .unwrap_or_else(|| registry_dir.join("snapshots"));
    // 026: certification is deterministic over committed evidence (FR-033a) — it never
    // resolves or invokes the reference implementation, and needs no container engine or
    // network. What it *does* need is the receipt proving the container-backed lane ran,
    // which arrives as the execution manifest (research D3).
    let evidence = build_evidence_inputs(&registry, registry_dir, manifest);
    let result = certify_with_evidence(
        &registry,
        today,
        &inputs,
        &clause_inputs,
        &snapshots_dir,
        &evidence,
    );

    if let Some(dir) = report_dir.as_deref() {
        // Written on BOTH verdicts: a blocked release must still ship the artifact that
        // explains why it was blocked.
        if let Err(code) = write_certification_report(&result, &registry, registry_dir, today, dir)
        {
            return code;
        }
    }

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
                BlockingKind::Obligation => println!(
                    "blocking obligation ({}): {}",
                    item.code.as_deref().unwrap_or("?"),
                    item.id
                ),
                // 026: each condition prints under the FR-041 name it blocks on, so a
                // reader matches a blocked release to the requirement that blocked it
                // (FR-042) rather than to an internal enum name.
                BlockingKind::StaleSnapshot => println!(
                    "blocking stale-snapshot: {} ({})",
                    item.id,
                    item.code
                        .as_deref()
                        .unwrap_or("evidence-determining input drifted")
                ),
                BlockingKind::MissingExecution => println!(
                    "blocking missing-required-execution: {} ({})",
                    item.id,
                    item.code.as_deref().unwrap_or("V35")
                ),
                BlockingKind::IncorrectOracle => println!(
                    "blocking incorrect-oracle: {} ({})",
                    item.id,
                    item.code
                        .as_deref()
                        .unwrap_or("recorded oracle differs from the pin")
                ),
                BlockingKind::RunnerOmission => println!(
                    "blocking unknown-runner-omission: {} ({})",
                    item.id,
                    item.code
                        .as_deref()
                        .unwrap_or("neither executed nor explicitly accounted for")
                ),
                BlockingKind::SilentlySkippedCase => println!(
                    "blocking silently-skipped-case: {} ({})",
                    item.id,
                    item.code
                        .as_deref()
                        .unwrap_or("outcome outside the enumeration")
                ),
                BlockingKind::FailingCase => println!(
                    "blocking failing-case: {} ({})",
                    item.id,
                    item.code.as_deref().unwrap_or("recorded as failing")
                ),
                BlockingKind::InactiveProfile => println!(
                    "blocking inactive-profile-refused: {} ({})",
                    item.id,
                    item.code
                        .as_deref()
                        .unwrap_or("zero applicable units is not the same as certified")
                ),
            }
        }
        // NON-BLOCKING snapshot coverage info (T073, FR-042).
        for cov in &result.snapshot_coverage {
            println!(
                "info snapshot-coverage: {} [{}]",
                cov.case_id,
                cov.platforms.join(", ")
            );
        }
        for id in &result.no_reference {
            println!("info no-reference-for-platform: {id} (no committed snapshot yet)");
        }
        // NON-BLOCKING residual queue (023 T035, FR-054): representation debt, listed
        // with what it pins, never a blocker.
        for residual in &result.residual_queue {
            println!(
                "info residual: {} blocks {} ({} unit(s)) — missing: {} [follow-up {}]",
                residual.id,
                residual
                    .blocked_carrier
                    .as_deref()
                    .unwrap_or("<no carrier>"),
                residual.units,
                residual.missing_capability,
                residual.follow_up.as_deref().unwrap_or("<untracked>")
            );
        }
        // NON-BLOCKING permanent exclusions (024 P1): reported under a DIFFERENT label so
        // they are never read as a stalled queue — each names why it can never migrate.
        for residual in &result.permanent_residuals {
            println!(
                "info residual (permanent): {} pins {} ({} unit(s)) — missing: {} — out of \
                 scope: {}",
                residual.id,
                residual
                    .blocked_carrier
                    .as_deref()
                    .unwrap_or("<no carrier>"),
                residual.units,
                residual.missing_capability,
                residual
                    .out_of_scope_rationale
                    .as_deref()
                    .unwrap_or("<unjustified>")
            );
        }
        // NON-BLOCKING declared normalization-rule deficiencies (023 T061).
        for rule in &result.non_compliant_rules {
            println!(
                "info normalization-rule non-compliant: {} — {}",
                rule.name, rule.reason
            );
        }
        // The five FR-026 obligation buckets plus the undispositioned queue, on ONE line
        // and never folded together (T068) — the numbers a reviewer needs to tell "the
        // hole is documented" from "the hole is closed".
        let o = &result.obligations;
        if o.total > 0 {
            println!(
                "info obligations ({} total): covered {}, waived {}, non-testable {}, gap {}, \
                 inactive-environment {}, undispositioned {}",
                o.total,
                o.covered,
                o.waived,
                o.non_testable,
                o.gap,
                o.inactive_environment,
                o.undispositioned
            );
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

/// `parity-page` — render the human-facing page, or verify the committed one matches.
///
/// Kept out of `docs/` generation-by-hand deliberately: this page drifted twice within
/// an hour of first publication, both times because a count was typed rather than
/// derived. `--check` is what makes that impossible to repeat.
fn parity_page_cmd(registry_dir: &Path, write: bool, check: bool) -> i32 {
    let registry = match Registry::load(registry_dir) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("error: {e}");
            return 1;
        }
    };
    let rendered = deacon_conformance::parity_page::render(&registry);
    let path = workspace_root().join("docs/PARITY.md");

    if check {
        let committed = std::fs::read_to_string(&path).unwrap_or_default();
        if committed == rendered {
            println!("ok: {} matches a fresh render", path.display());
            return 0;
        }
        eprintln!(
            "error: {} is stale — regenerate with `cargo run -p deacon-conformance -- \
             parity-page --write`",
            path.display()
        );
        return 1;
    }

    if write {
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        match std::fs::write(&path, &rendered) {
            Ok(()) => {
                println!("wrote {}", path.display());
                0
            }
            Err(e) => {
                eprintln!("error: writing {}: {e}", path.display());
                1
            }
        }
    } else {
        print!("{rendered}");
        0
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
