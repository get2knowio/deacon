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

use std::path::{Path, PathBuf};

use anyhow::Context;
use clap::{Parser, Subcommand};

use deacon_conformance::certify::certify;
use deacon_conformance::default_registry_dir;
use deacon_conformance::load::{LoadError, Registry};
use deacon_conformance::report::write_reports;
use deacon_conformance::validate::{Violation, validate_path};
use deacon_conformance::workspace_root;

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
    }
}

/// Structural validation (V1–V10 + SCHEMA), per contracts/cli.md:
///
/// - text mode: one violation per line on stdout, nothing on success;
/// - `--json` mode: a single `{ "ok", "violations" }` document on stdout;
///
/// with logs/diagnostics always on stderr. Exit codes: `0` valid, `1` one or more
/// violations (all reported, not first-failure), `2` unreadable registry root.
fn validate(registry_dir: &Path, today: &str, json: bool) -> i32 {
    let repo_root = workspace_root();
    let violations = match validate_path(registry_dir, today, &repo_root) {
        Ok(violations) => violations,
        Err(LoadError::Root { path, cause }) => {
            eprintln!("error: cannot read registry root {path:?}: {cause}");
            return 2;
        }
        // `validate_path` folds schema failures into SCHEMA-class violations, so the
        // only `Err` it returns is `Root`; treat anything else defensively as usage.
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

    let out_dir = out_dir.unwrap_or_else(default_report_dir);
    match write_reports(&registry, &out_dir) {
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

/// `certify` (contracts/cli.md): validate first (invalid → exit 1), then evaluate
/// strict certification. Exit `0` certified, `1` not certified (blocking items
/// listed) or registry invalid, `2` usage/IO.
fn certify_cmd(registry_dir: &Path, today: &str, json: bool) -> i32 {
    let registry = match load_and_validate(registry_dir, today) {
        Ok(registry) => registry,
        Err(code) => return code,
    };

    let result = certify(&registry);

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
            let kind = if item.kind == deacon_conformance::certify::BlockingKind::Gap {
                "gap"
            } else {
                "uncovered"
            };
            println!("blocking {kind}: {}", item.id);
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
