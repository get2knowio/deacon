//! Every subcommand deacon advertises must appear in `parity/SPEC_STATUS.md`.
//!
//! The ledger calls itself the claim-by-claim answer to "does deacon behave like
//! the reference?". Its existing guards check the Summary against its own row
//! census — they compare the file to itself, so a subcommand that was never
//! written down at all is invisible to them.
//!
//! That is not hypothetical: `deacon config` shipped with zero rows, no section
//! and no mention, while every sibling extension (`down`, `doctor`, host-CA,
//! profiles) had one. Found by diffing `deacon --help` against
//! `devcontainer --help` while mining `cli.test`, which is a check nothing ran.
//!
//! Hermetic: reads the built binary's `--help` and one markdown file.

use assert_cmd::Command;

/// Subcommands that legitimately have no ledger section, with the reason.
///
/// Keep this list SHORT and each entry justified — it is the escape hatch, and a
/// long one would defeat the guard.
const EXEMPT: &[(&str, &str)] = &[(
    "help",
    "clap's built-in help command, not a deacon behavior surface",
)];

fn advertised_subcommands() -> Vec<String> {
    let output = Command::cargo_bin("deacon")
        .unwrap()
        .arg("--help")
        .output()
        .unwrap();
    let help = String::from_utf8_lossy(&output.stdout);

    let mut in_commands = false;
    let mut found = Vec::new();
    for line in help.lines() {
        if line.starts_with("Commands:") {
            in_commands = true;
            continue;
        }
        if in_commands {
            // The block ends at the first non-indented line.
            if !line.starts_with("  ") {
                if line.trim().is_empty() {
                    continue;
                }
                break;
            }
            if let Some(name) = line.split_whitespace().next() {
                if name.chars().all(|c| c.is_ascii_lowercase() || c == '-') {
                    found.push(name.to_string());
                }
            }
        }
    }
    found.sort();
    found.dedup();
    assert!(
        found.len() > 5,
        "could not parse the subcommand list out of `deacon --help`; got {found:?}"
    );
    found
}

#[test]
fn every_advertised_subcommand_has_a_ledger_section() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("workspace root");
    let ledger = std::fs::read_to_string(root.join("parity/SPEC_STATUS.md"))
        .expect("reading parity/SPEC_STATUS.md");

    let missing: Vec<String> = advertised_subcommands()
        .into_iter()
        .filter(|name| !EXEMPT.iter().any(|(e, _)| e == name))
        // A section may be narrower than the subcommand — `templates apply` is
        // recorded as `### `templates-apply``, which covers `templates`. Accept
        // an exact section or any `<name>-…` refinement of one.
        .filter(|name| {
            !ledger.contains(&format!("### `{name}`")) && !ledger.contains(&format!("### `{name}-"))
        })
        .collect();

    assert!(
        missing.is_empty(),
        "these subcommands are advertised by `deacon --help` but have no `### `<name>`` \
         section in parity/SPEC_STATUS.md: {missing:?}\n\n\
         Every surface deacon exposes owes the ledger at least one row — a deacon-only \
         one is a `deacon extension`. If a subcommand genuinely owes none, add it to \
         EXEMPT in this file WITH the reason."
    );
}
