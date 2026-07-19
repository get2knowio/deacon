//! Cross-runner equivalence proof (018-harden-parity-harness, T040; SC-005, FR-019).
//!
//! There is exactly ONE equivalence definition — `parity_harness::normalize` — and
//! every runner (the `read-configuration` scenario binary, the tier1 corpus runner,
//! the merged-config runner) reaches its verdict THROUGH it. These hermetic tests
//! prove that single-sourcing observably: the SAME pair of raw CLI outputs, run
//! through `normalize::config` + `diff` under DIFFERENT caller contexts (distinct
//! `case` labels standing in for distinct runners), yields the IDENTICAL verdict —
//! and that `merged_config` agrees with `config` on the shared configuration block.
//! No live oracle, Docker, or network is involved.

use parity_harness::HarnessError;
use parity_harness::normalize::{self, DiffKind};
use serde_json::{Value, json};

/// The verdict a runner reaches for one (deacon, reference) output pair, reduced to
/// exactly what drives pass/fail: equal-after-normalization, a *ranked* list of
/// divergence classes, or a hard normalization failure (never a raw-compare
/// fallback).
#[derive(Debug, PartialEq, Eq)]
enum Verdict {
    Equal,
    Divergent(Vec<DiffKind>),
    NormalizationFailed,
}

/// Compare a (deacon, reference) config-output pair exactly as every runner does:
/// normalize both sides through the single module, then rank-diff. `case` is the
/// caller-context label — the only thing that varies between runners.
fn config_verdict(case: &str, deacon_raw: &str, reference_raw: &str) -> Verdict {
    let normalize_one = |raw: &str| -> Result<Value, ()> {
        match normalize::config(case, raw) {
            Ok(v) => Ok(v),
            Err(HarnessError::Normalization { .. }) => Err(()),
            Err(other) => panic!("unexpected non-normalization error for `{case}`: {other:?}"),
        }
    };
    let (Ok(d), Ok(r)) = (normalize_one(deacon_raw), normalize_one(reference_raw)) else {
        return Verdict::NormalizationFailed;
    };
    let divs = normalize::diff(&d, &r);
    if divs.is_empty() {
        Verdict::Equal
    } else {
        Verdict::Divergent(divs.iter().map(|x| x.kind).collect())
    }
}

/// One row of the equivalence table: a (deacon, reference) output pair — the
/// reference side carries the CLI's real `{configuration}` wrapper + `configFilePath`
/// noise the runners must normalize away — and the verdict every runner MUST reach.
struct Row {
    name: &'static str,
    deacon: &'static str,
    reference: &'static str,
    expected: Verdict,
}

fn table() -> Vec<Row> {
    vec![
        // Identical after prune: reference's wrapper, configFilePath, and empty
        // container are all dropped; nothing left to diverge.
        Row {
            name: "equal-after-prune",
            deacon: r#"{ "name": "demo" }"#,
            reference: r#"{ "configuration": { "name": "demo", "empty": {} },
                           "configFilePath": "/w/.devcontainer/devcontainer.json" }"#,
            expected: Verdict::Equal,
        },
        // The reference keeps a key deacon dropped: highest-signal ref-only.
        Row {
            name: "ref-only-key",
            deacon: r#"{ "name": "demo" }"#,
            reference: r#"{ "configuration": { "name": "demo", "remoteUser": "vscode" } }"#,
            expected: Verdict::Divergent(vec![DiffKind::RefOnly]),
        },
        // deacon emits a key the reference lacks: lowest-signal deacon-only.
        Row {
            name: "deacon-only-key",
            deacon: r#"{ "name": "demo", "extra": 1 }"#,
            reference: r#"{ "configuration": { "name": "demo" } }"#,
            expected: Verdict::Divergent(vec![DiffKind::DeaconOnly]),
        },
        // Same key, differing value: a value mismatch.
        Row {
            name: "value-mismatch",
            deacon: r#"{ "name": "demo-a" }"#,
            reference: r#"{ "configuration": { "name": "demo-b" } }"#,
            expected: Verdict::Divergent(vec![DiffKind::Value]),
        },
        // The ONLY difference is a dynamic id (deacon emits a 12-hex hash where the
        // reference emits the `${devcontainerId}` template). Both sanitize to <ID>,
        // so a runner must NOT flag this as a divergence.
        Row {
            name: "dynamic-id-only",
            deacon: r#"{ "mount": "vol_0123456789ab_data" }"#,
            reference: r#"{ "configuration": { "mount": "vol_${devcontainerId}_data" } }"#,
            expected: Verdict::Equal,
        },
        // Malformed JSON on one side: a hard normalization failure, never a
        // fall-through to raw comparison.
        Row {
            name: "malformed-json",
            deacon: r#"{ not json"#,
            reference: r#"{ "configuration": { "name": "demo" } }"#,
            expected: Verdict::NormalizationFailed,
        },
    ]
}

#[test]
fn config_verdict_is_identical_across_caller_contexts() {
    for row in table() {
        // Two distinct caller contexts standing in for two distinct runners that
        // both route through the single normalization module.
        let read_config_ctx = config_verdict(
            &format!("read-configuration/{}", row.name),
            row.deacon,
            row.reference,
        );
        let tier1_ctx = config_verdict(
            &format!("corpus-tier1/{}", row.name),
            row.deacon,
            row.reference,
        );

        assert_eq!(
            read_config_ctx, tier1_ctx,
            "case `{}`: verdict must not depend on the caller context",
            row.name
        );
        assert_eq!(
            read_config_ctx, row.expected,
            "case `{}`: verdict differs from the single-sourced expectation",
            row.name
        );
    }
}

#[test]
fn merged_config_agrees_with_config_on_the_shared_block() {
    // For any configuration body, the block extracted by `merged_config` from a
    // `{mergedConfiguration: body}` document must normalize IDENTICALLY to the body
    // unwrapped by `config` from a `{configuration: body}` document — same prune,
    // same dynamic-id sanitization. Reusing the equivalence-table bodies keeps the
    // two entry points provably in lockstep on the shared block.
    let bodies = [
        json!({ "name": "demo", "empty": {}, "n": null }),
        json!({ "name": "demo", "remoteUser": "vscode" }),
        json!({ "mount": "vol_0123456789ab_data", "id": "${devcontainerId}" }),
        json!({ "forwardPorts": [3000, 8080], "runArgs": ["--rm"] }),
    ];

    for body in bodies {
        let via_config = normalize::config(
            "shared",
            &Value::Object(
                [("configuration".to_string(), body.clone())]
                    .into_iter()
                    .collect(),
            )
            .to_string(),
        )
        .expect("config normalizes");
        let via_merged = normalize::merged_config(
            "shared",
            &Value::Object(
                [("mergedConfiguration".to_string(), body.clone())]
                    .into_iter()
                    .collect(),
            )
            .to_string(),
        )
        .expect("merged_config normalizes");

        assert_eq!(
            via_config, via_merged,
            "config and merged_config must agree on the shared block for body {body}"
        );
    }
}

#[test]
fn diff_ranking_is_stable_regardless_of_input_order() {
    // A pair carrying all three divergence classes at once must rank identically no
    // matter which caller normalizes it: ref-only → value → deacon-only.
    let deacon = r#"{ "name": "a", "extra": 1 }"#;
    let reference = r#"{ "configuration": { "name": "b", "dropped": 2 } }"#;
    let a = config_verdict("runner-a", deacon, reference);
    let b = config_verdict("runner-b", deacon, reference);
    assert_eq!(a, b);
    assert_eq!(
        a,
        Verdict::Divergent(vec![
            DiffKind::RefOnly,
            DiffKind::Value,
            DiffKind::DeaconOnly
        ]),
        "ranked divergence order must be single-sourced and stable"
    );
}
