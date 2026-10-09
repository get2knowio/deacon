//! A dependency whose DEFAULT features introduce `unsafe` must be taken with
//! `default-features = false`.
//!
//! This workspace sets `unsafe_code = "deny"` (root `Cargo.toml` `[lints]`), which
//! governs OUR code and says nothing about a dependency's. Most dependencies carry
//! `unsafe` and that is unremarkable. The case this guards is narrower and sharper:
//! a crate that is unsafe-free when a feature is OFF, ships that feature ON by
//! default, and so starts compiling `unsafe` into the graph as a side effect of a
//! routine version bump — with no diff to review beyond a version string.
//!
//! That is not hypothetical. base64 0.23.0 added SIMD engines behind `simd-unsafe`
//! and put it in `default`; base64's own manifest calls it "the only feature that
//! introduces `unsafe`; with it disabled the crate is `#![forbid(unsafe_code)]`".
//! Dependabot's #456 would have enabled it silently. The opt-out landed with that
//! bump, and nothing but this test keeps a future one from undoing it — a later
//! bump that rewrites the whole dependency line back to `base64 = "0.24"` restores
//! the default and reads like every other version bump in the diff.
//!
//! Hermetic: reads one manifest off disk. No cargo, no network, no daemon.

use std::path::PathBuf;

/// A dependency that is unsafe-free only with its default features disabled.
struct UnsafeByDefault {
    /// Dependency key as spelled in the manifest.
    name: &'static str,
    /// The feature that introduces `unsafe`, for the failure message.
    feature: &'static str,
    /// Release that made it default-on, so a reader can check the claim.
    since: &'static str,
}

/// Keep this list SHORT and each entry checked against the crate's own manifest
/// before adding it. "This crate contains unsafe" does NOT qualify — the entry has
/// to be a feature that is default-ON and whose absence makes the crate unsafe-free.
const UNSAFE_BY_DEFAULT: &[UnsafeByDefault] = &[UnsafeByDefault {
    name: "base64",
    feature: "simd-unsafe",
    since: "0.23.0",
}];

#[test]
fn a_dependency_that_is_unsafe_by_default_disables_default_features() {
    let manifest_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml");
    let manifest = std::fs::read_to_string(&manifest_path)
        .unwrap_or_else(|e| panic!("reading {}: {e}", manifest_path.display()));

    for dep in UNSAFE_BY_DEFAULT {
        let decl = dependency_declaration(&manifest, dep.name).unwrap_or_else(|| {
            panic!(
                "`{}` is listed in UNSAFE_BY_DEFAULT but {} declares no such dependency. \
                 If it was removed, drop the entry; if it moved to another crate, move \
                 this guard with it.",
                dep.name,
                manifest_path.display()
            )
        });

        assert!(
            decl.contains("default-features = false"),
            "`{}` must be taken with `default-features = false`: its `{}` feature is \
             default-ON since {} and is the only thing that introduces `unsafe` in it, \
             while this workspace denies `unsafe_code`. Found:\n  {}\nIf the crate has \
             since stopped gating `unsafe` behind a default feature, verify that against \
             its manifest and remove the entry from UNSAFE_BY_DEFAULT — do not weaken \
             this assertion.",
            dep.name,
            dep.feature,
            dep.since,
            decl.trim()
        );

        assert!(
            !decl.contains(dep.feature),
            "`{}` disables default features but then asks for `{}` back explicitly, \
             which defeats the point. Found:\n  {}",
            dep.name,
            dep.feature,
            decl.trim()
        );
    }
}

/// The full text of `name`'s dependency declaration, or `None` if absent.
///
/// Handles the two shapes a manifest can use — an inline table on the dependency's
/// own line (possibly wrapped across lines) and a `[dependencies.<name>]` section —
/// because a guard that only understood the current spelling could be silently
/// defeated by a reformat, which is the same class of miss it exists to prevent.
fn dependency_declaration(manifest: &str, name: &str) -> Option<String> {
    // `[dependencies.<name>]` / `[dev-dependencies.<name>]` section form.
    for (idx, line) in manifest.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            let header = trimmed.trim_start_matches('[').trim_end_matches(']');
            if header
                .rsplit_once('.')
                .is_some_and(|(prefix, key)| key == name && prefix.ends_with("dependencies"))
            {
                let body: Vec<&str> = manifest
                    .lines()
                    .skip(idx + 1)
                    .take_while(|l| !l.trim_start().starts_with('['))
                    .collect();
                return Some(body.join("\n"));
            }
        }
    }

    // `name = …` form. Collect until braces balance, so a wrapped inline table is
    // captured whole rather than truncated at the first newline.
    let mut lines = manifest.lines().peekable();
    while let Some(line) = lines.next() {
        let trimmed = line.trim_start();
        let Some(rest) = trimmed.strip_prefix(name) else {
            continue;
        };
        let rest = rest.trim_start();
        // `name = …`, not `name-other = …` or `# name …`.
        if !rest.starts_with('=') {
            continue;
        }
        let mut decl = line.to_string();
        let mut depth = brace_delta(line);
        while depth > 0 {
            match lines.next() {
                Some(next) => {
                    decl.push('\n');
                    decl.push_str(next);
                    depth += brace_delta(next);
                }
                None => break,
            }
        }
        return Some(decl);
    }

    None
}

/// Net `{` minus `}` on a line, for balancing a wrapped inline table.
fn brace_delta(line: &str) -> i32 {
    line.chars().fold(0, |acc, c| match c {
        '{' => acc + 1,
        '}' => acc - 1,
        _ => acc,
    })
}

#[test]
fn the_declaration_reader_understands_both_manifest_shapes() {
    // Guard the guard: each shape must be found, and the brace balancer must keep a
    // wrapped inline table intact. Without this, a reader bug reads as "the
    // dependency is absent" — which the main test turns into a panic rather than a
    // silent pass, but the message would send a reader hunting the wrong thing.
    let inline = "base64 = { version = \"0.23\", default-features = false }\n";
    assert!(
        dependency_declaration(inline, "base64")
            .expect("inline form")
            .contains("default-features = false")
    );

    let wrapped =
        "base64 = {\n  version = \"0.23\",\n  default-features = false,\n}\nserde = \"1\"\n";
    let found = dependency_declaration(wrapped, "base64").expect("wrapped form");
    assert!(found.contains("default-features = false"));
    assert!(
        !found.contains("serde"),
        "the reader must stop at the closing brace, got:\n{found}"
    );

    let section = "[dependencies.base64]\nversion = \"0.23\"\ndefault-features = false\n\n[dependencies.serde]\nversion = \"1\"\n";
    let found = dependency_declaration(section, "base64").expect("section form");
    assert!(found.contains("default-features = false"));
    assert!(
        !found.contains("serde"),
        "the reader must stop at the next section, got:\n{found}"
    );

    // A similarly-named key must not match, or the guard could pass on the wrong line.
    let lookalike = "base64-simd = { version = \"1\" }\n";
    assert!(dependency_declaration(lookalike, "base64").is_none());

    assert!(dependency_declaration("serde = \"1\"\n", "base64").is_none());
}
