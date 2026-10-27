//! Differential parity for the generated Feature Dockerfile's `# syntax=` line
//! (#732) against the reference CLI.
//!
//! Every expectation in `fixtures/feature_syntax_directive_oracle.json` was
//! **measured**, not written by hand: the reference's own compiled
//! `dist/spec-node/devContainersSpecCLI.js` at the version `parity/oracle.json`
//! pins (`0.89.0`) was sliced — its `parseVersion` / `isEarlierVersion`,
//! `extractDockerfile`, `supportsBuildContexts`, the `dockerfilePrefixContent`
//! template expression and its callers' leading-directive strip regex, all
//! verbatim — and evaluated over the table below. The slice was self-checked
//! first: its `supportsBuildContexts` reproduces all 91 expectations of
//! `dockerfile_utils_oracle.json`.
//!
//! Regenerate with `scripts/parity/measure-feature-syntax-directive/`:
//!
//! ```text
//! python3 -I build_oracle.py <cli>/dist/spec-node/devContainersSpecCLI.js oracle.js
//! node measure.js <semver module dir> ./oracle.js <this fixture>
//! ```
//!
//! Do not hand-edit an expectation to make this test pass — the expectation IS
//! the reference's behavior.
//!
//! The table crosses every Dockerfile shape with every engine-version probe
//! outcome (including a failed probe and output with no `MAJOR.MINOR.PATCH`),
//! every BuildKit probe outcome, and `--omit-syntax-directive` on and off, so
//! each of the four branches is reached from every side.
//!
//! Hermetic: no network, no Docker, no oracle at run time.

use std::collections::HashMap;

use deacon_core::dockerfile_utils::extract_dockerfile;
use deacon_core::feature_syntax_directive::{
    SyntaxDirectiveInputs, apply_syntax_directive, feature_dockerfile_syntax_directive,
    strip_leading_syntax_directive, version_triplet,
};

#[derive(serde::Deserialize)]
struct DockerfileEntry {
    content: String,
    stripped: String,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct Fixture {
    dockerfiles: HashMap<String, Option<DockerfileEntry>>,
    engine_outputs: HashMap<String, Option<String>>,
    buildkit_outputs: HashMap<String, Option<String>>,
    /// `[dockerfile id, engine id, buildkit id, omit, directive]`, where an empty
    /// directive is the reference's `''`.
    cases: Vec<(String, String, String, bool, String)>,
}

fn fixture() -> Fixture {
    serde_json::from_str(include_str!(
        "fixtures/feature_syntax_directive_oracle.json"
    ))
    .expect("oracle fixture must parse")
}

/// The whole table, reported as one failure listing every divergence.
#[test]
fn syntax_directive_matches_the_reference_on_every_measured_case() {
    let fx = fixture();
    let mut divergences = Vec::new();
    let mut injected = 0usize;

    for (df_id, engine_id, buildkit_id, omit, expected) in &fx.cases {
        let entry = &fx.dockerfiles[df_id];
        let parsed = entry.as_ref().map(|e| extract_dockerfile(&e.content));
        let engine = fx.engine_outputs[engine_id]
            .as_deref()
            .and_then(version_triplet);
        let buildkit = fx.buildkit_outputs[buildkit_id]
            .as_deref()
            .and_then(version_triplet);

        let actual = feature_dockerfile_syntax_directive(SyntaxDirectiveInputs {
            omit_syntax_directive: *omit,
            docker_engine_version: engine,
            buildkit_version: buildkit,
            base_dockerfile: parsed.as_ref(),
        })
        .unwrap_or_default();

        if actual.ends_with("docker/dockerfile:1.4") && !df_id.contains("1.4") {
            injected += 1;
        }
        if &actual != expected {
            divergences.push(format!(
                "{df_id}/{engine_id}/{buildkit_id}{}: expected {expected:?}, got {actual:?}",
                if *omit { "/omit" } else { "" }
            ));
        }
    }

    assert!(
        divergences.is_empty(),
        "{} of {} cases diverge from the reference:\n{}",
        divergences.len(),
        fx.cases.len(),
        divergences.join("\n")
    );
    // The injecting branch must actually be exercised, or a table that never
    // reaches it would pass against an implementation that never injects.
    assert!(injected > 0, "no case reached the injecting branch");
}

#[test]
fn leading_directive_is_stripped_exactly_as_the_reference_strips_it() {
    let fx = fixture();
    let mut divergences = Vec::new();
    for (id, entry) in &fx.dockerfiles {
        let Some(entry) = entry else { continue };
        let actual = strip_leading_syntax_directive(&entry.content);
        if actual != entry.stripped {
            divergences.push(format!(
                "{id}: expected {:?}, got {actual:?}",
                entry.stripped
            ));
        }
        // And the merge built on it heads the strip with the decided line.
        let merged = apply_syntax_directive(&entry.content, Some("# syntax=x"));
        if merged != format!("# syntax=x\n{}", entry.stripped) {
            divergences.push(format!("{id}: merged head wrong: {merged:?}"));
        }
    }
    assert!(divergences.is_empty(), "{}", divergences.join("\n"));
}
