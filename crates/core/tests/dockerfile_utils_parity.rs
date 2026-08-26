//! Differential parity for `dockerfile_utils` against the reference CLI.
//!
//! Every expectation in `fixtures/dockerfile_utils_oracle.json` was **measured**,
//! not written by hand: the reference's own `src/spec-node/dockerfileUtils.ts` at
//! tag `v0.87.0` (the version `parity/oracle.json` pins) was compiled and invoked
//! over this table, and its answers recorded. The compiled oracle was itself
//! checked first — it reproduces every expectation in upstream's own
//! `src/test/dockerfileUtils.test.ts` before being trusted to judge deacon.
//!
//! Cases tagged `upstream` come from that test suite; cases tagged `extra` are
//! adversarial additions (stage cycles, forward references, case folding, CRLF,
//! nested variable expressions) covering shapes upstream does not assert but a
//! real Dockerfile can reach.
//!
//! This ran 27 of its first 89 cases red before #686. Regenerating the fixture requires re-measuring
//! against the pinned reference — do not hand-edit an expectation to make a test
//! pass, because the expectation IS the reference's behavior.
//!
//! Hermetic: no network, no Docker, no oracle at run time.

use deacon_core::dockerfile_utils::{
    ensure_dockerfile_has_final_stage_name, extract_dockerfile, find_user_statement,
    resolve_base_image,
};
use std::collections::HashMap;

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct Expected {
    base_image: Option<String>,
    user: Option<String>,
    last_stage_name: Option<String>,
    modified_dockerfile: Option<String>,
    ensure_error: bool,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct Case {
    id: String,
    src: String,
    dockerfile: String,
    build_args: HashMap<String, String>,
    target: Option<String>,
    base_image_env: HashMap<String, String>,
    expected: Expected,
}

fn cases() -> Vec<Case> {
    serde_json::from_str(include_str!("fixtures/dockerfile_utils_oracle.json"))
        .expect("oracle fixture must parse")
}

/// The whole table, reported as one failure listing every divergence — a
/// per-case `assert` would stop at the first and hide the shape of a regression.
#[test]
fn dockerfile_utils_matches_the_reference_on_every_measured_case() {
    let cases = cases();
    assert_eq!(cases.len(), 91, "fixture case count changed unexpectedly");

    let mut diffs: Vec<String> = Vec::new();
    let mut record = |case: &Case, field: &str, want: String, got: String| {
        if want != got {
            diffs.push(format!(
                "[{field}] {} ({})\n   reference: {want}\n   deacon:    {got}",
                case.id, case.src
            ));
        }
    };

    for case in &cases {
        let parsed = extract_dockerfile(&case.dockerfile);
        let target = case.target.as_deref();

        record(
            case,
            "baseImage",
            format!("{:?}", case.expected.base_image),
            format!("{:?}", parsed.base_image(&case.build_args, target)),
        );
        record(
            case,
            "user",
            format!("{:?}", case.expected.user),
            format!(
                "{:?}",
                parsed.user_statement(&case.build_args, &case.base_image_env, target)
            ),
        );

        let ensured = ensure_dockerfile_has_final_stage_name(&case.dockerfile, "placeholder");
        record(
            case,
            "ensureError",
            format!("{:?}", case.expected.ensure_error),
            format!("{:?}", ensured.is_err()),
        );
        let (modified, stage) = match ensured {
            Ok((modified, stage)) => (Some(modified), Some(stage)),
            Err(_) => (None, None),
        };
        record(
            case,
            "lastStageName",
            format!("{:?}", case.expected.last_stage_name),
            format!("{:?}", stage),
        );
        record(
            case,
            "modifiedDockerfile",
            format!("{:?}", case.expected.modified_dockerfile),
            format!("{:?}", modified),
        );
    }

    assert!(
        diffs.is_empty(),
        "{} divergence(s) from the reference across {} cases:\n\n{}",
        diffs.len(),
        cases.len(),
        diffs.join("\n\n")
    );
}

/// `resolve_base_image` is `base_image` plus a guard that is OURS: the reference
/// reports `scratch` and `""` as base images and its callers cope, while every
/// deacon caller uses the result to inspect a real image. Asserting the guard
/// separately keeps it from being mistaken for reference behavior.
#[test]
fn resolve_base_image_applies_deacons_inspectable_guard_on_top() {
    let no_args = HashMap::new();
    for case in cases() {
        let faithful = extract_dockerfile(&case.dockerfile)
            .base_image(&case.build_args, case.target.as_deref());
        let guarded =
            resolve_base_image(&case.dockerfile, &case.build_args, case.target.as_deref());
        let expected =
            faithful.filter(|image| !image.is_empty() && !image.eq_ignore_ascii_case("scratch"));
        assert_eq!(guarded, expected, "guard mismatch for {}", case.id);
    }

    assert_eq!(resolve_base_image("FROM scratch\n", &no_args, None), None);
    assert_eq!(resolve_base_image("FROM $UNSET\n", &no_args, None), None);
    assert_eq!(
        resolve_base_image("FROM alpine:3.19\n", &no_args, None).as_deref(),
        Some("alpine:3.19")
    );
}

/// The free function must agree with the method it wraps, including the
/// base-image-env fallback that only the method's caller can supply.
#[test]
fn find_user_statement_free_function_agrees_with_the_parsed_form() {
    for case in cases() {
        let via_parse = extract_dockerfile(&case.dockerfile).user_statement(
            &case.build_args,
            &case.base_image_env,
            case.target.as_deref(),
        );
        let via_free = find_user_statement(
            &case.dockerfile,
            &case.build_args,
            &case.base_image_env,
            case.target.as_deref(),
        );
        assert_eq!(via_free, via_parse, "mismatch for {}", case.id);
    }
}
