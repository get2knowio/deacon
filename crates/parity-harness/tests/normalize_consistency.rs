//! Cross-caller equivalence proof (018-harden-parity-harness, T040; SC-005, FR-019).
//!
//! There is exactly ONE equivalence definition — `parity_harness::normalize` for
//! normalization, `parity_harness::compare` for the comparison — and every caller
//! reaches its verdict THROUGH them. These hermetic tests prove that single-sourcing
//! observably: the SAME pair of raw CLI outputs, normalized and compared under DIFFERENT
//! caller contexts (distinct `case` labels standing in for distinct callers), yields the
//! IDENTICAL verdict — and that `merged_config` agrees with `config` on the shared
//! configuration block. No live oracle, Docker, or network is involved.
//!
//! The verdict used to be a *ranked class* (`ref-only` / `deacon-only` / `value`) produced
//! by a second differ living in `normalize`. That differ is gone; the declarative
//! comparison names the diverging PATH and leaves both sides' values in the preserved
//! evidence. The property these tests defend is unchanged — a difference is detected, and
//! a ONE-SIDED difference is detected in both directions.

use parity_harness::HarnessError;
use parity_harness::compare::{Tolerances, verdict_differential};
use parity_harness::evidence::{NormalizedChannelEvidence, Outcome};
use parity_harness::exec::Side;
use parity_harness::model::CHAN_STRUCTURED_OUTPUT;
use parity_harness::normalize::{self, DocumentBlock};
use serde_json::{Value, json};

/// The verdict a caller reaches for one (deacon, reference) output pair, reduced to
/// exactly what drives pass/fail: equal-after-normalization, the diverging observable
/// paths, or a hard normalization failure (never a raw-compare fallback).
#[derive(Debug, PartialEq, Eq)]
enum Verdict {
    Equal,
    Divergent(Vec<String>),
    NormalizationFailed,
}

/// The observable paths on which two normalized documents differ, reached through the
/// production comparison. Channel-prefixed paths are trimmed to the document-relative
/// path so the expectations read as the field names they are about.
fn diverging_paths(deacon: &Value, reference: &Value) -> Vec<String> {
    let side = |value: &Value| NormalizedChannelEvidence {
        channel: CHAN_STRUCTURED_OUTPUT.to_string(),
        operation: "op-read".to_string(),
        present: true,
        value: value.clone(),
    };
    let no_tolerances = Tolerances::new(&[], &[]);
    let mut consumed = std::collections::HashSet::new();
    let verdict = verdict_differential(
        CHAN_STRUCTURED_OUTPUT,
        &side(deacon),
        &side(reference),
        &no_tolerances,
        &mut consumed,
    );
    let prefix = format!("{CHAN_STRUCTURED_OUTPUT}.");
    match verdict.outcome {
        Outcome::Agree => Vec::new(),
        Outcome::Diverge => verdict
            .detail
            .as_ref()
            .and_then(|d| d.get("divergingPaths"))
            .and_then(Value::as_array)
            .expect("a differential divergence names its paths")
            .iter()
            .map(|p| {
                let p = p.as_str().expect("a diverging path is a string");
                p.strip_prefix(&prefix).unwrap_or(p).to_string()
            })
            .collect(),
        other => panic!("unexpected outcome with no tolerances declared: {other:?}"),
    }
}

/// Compare a (deacon, reference) config-output pair exactly as every caller does:
/// normalize both sides through the single module, then compare through the single
/// comparison. `case` is the caller-context label — the only thing that varies.
fn config_verdict(case: &str, deacon_raw: &str, reference_raw: &str) -> Verdict {
    let normalize_one = |raw: &str| -> Result<Value, ()> {
        match normalize::config(case, raw, Side::Deacon) {
            Ok(v) => Ok(v),
            Err(HarnessError::Normalization { .. }) => Err(()),
            Err(other) => panic!("unexpected non-normalization error for `{case}`: {other:?}"),
        }
    };
    let (Ok(d), Ok(r)) = (normalize_one(deacon_raw), normalize_one(reference_raw)) else {
        return Verdict::NormalizationFailed;
    };
    let paths = diverging_paths(&d, &r);
    if paths.is_empty() {
        Verdict::Equal
    } else {
        Verdict::Divergent(paths)
    }
}

/// Sugar for a one-path divergence expectation.
fn diverges_at(path: &str) -> Verdict {
    Verdict::Divergent(vec![path.to_string()])
}

/// One row of the equivalence table: a (deacon, reference) output pair — the
/// reference side carries the CLI's real `{configuration}` wrapper — and the verdict
/// every runner MUST reach.
struct Row {
    name: &'static str,
    deacon: &'static str,
    reference: &'static str,
    expected: Verdict,
}

fn table() -> Vec<Row> {
    vec![
        // Equal after the named rules: the reference's wrapper is unwrapped and a
        // `${devcontainerId}` digest is tokenized by `devcontainer_id_token`; nothing
        // else differs. (023 T062 — `prune` used to drop ANY empty value and
        // `configFilePath` too; both are compared now.)
        Row {
            name: "equal-after-named-rules",
            deacon: r#"{ "name": "demo" }"#,
            reference: r#"{ "configuration": { "name": "demo" } }"#,
            expected: Verdict::Equal,
        },
        // #398: an authored empty on the `configuration` block is now COMPARED. The
        // reference echoes what the author wrote, and deacon does too — so a side that
        // emits `customizations: {}` while the other omits it is a real difference, not
        // a serializer artifact to be normalized away.
        Row {
            name: "authored-empty-is-compared",
            deacon: r#"{ "name": "demo" }"#,
            reference: r#"{ "configuration": { "name": "demo", "customizations": {} } }"#,
            expected: diverges_at("customizations"),
        },
        // An UNLISTED empty value is now compared rather than silently dropped — the
        // regression `prune` made invisible (023 T062).
        Row {
            name: "unlisted-empty-is-compared",
            deacon: r#"{ "name": "demo", "someNewProperty": {} }"#,
            reference: r#"{ "configuration": { "name": "demo" } }"#,
            expected: diverges_at("someNewProperty"),
        },
        // `configFilePath` is no longer dropped: the reference emits it and deacon does
        // not, and that is now REPORTED (research D3).
        Row {
            name: "config-file-path-is-compared",
            deacon: r#"{ "name": "demo" }"#,
            reference: r#"{ "configuration": { "name": "demo",
                           "configFilePath": "/w/.devcontainer/devcontainer.json" } }"#,
            expected: diverges_at("configFilePath"),
        },
        // The reference keeps a key deacon dropped: highest-signal ref-only.
        Row {
            name: "ref-only-key",
            deacon: r#"{ "name": "demo" }"#,
            reference: r#"{ "configuration": { "name": "demo", "remoteUser": "vscode" } }"#,
            expected: diverges_at("remoteUser"),
        },
        // deacon emits a key the reference lacks: a deacon-only finding, reported with
        // the same significance as any other class (023 T065, FR-020).
        Row {
            name: "deacon-only-key",
            deacon: r#"{ "name": "demo", "extra": 1 }"#,
            reference: r#"{ "configuration": { "name": "demo" } }"#,
            expected: diverges_at("extra"),
        },
        // Same key, differing value: a value mismatch.
        Row {
            name: "value-mismatch",
            deacon: r#"{ "name": "demo-a" }"#,
            reference: r#"{ "configuration": { "name": "demo-b" } }"#,
            expected: diverges_at("name"),
        },
        // The ONLY difference is a dynamic id inside an ENUMERATED id-bearing field
        // (`mounts`): one side emits a substituted id where the other emits the
        // `${devcontainerId}` template. Both tokenize to <ID>, so this is not a
        // divergence. The substituted form is 52 base-32 digits since #670 — the
        // spec's own computation, which both CLIs now produce.
        Row {
            name: "dynamic-id-only",
            deacon: r#"{ "mounts": ["vol_0uhonu0v70vmigpqqrkg1kqr7ohoam9veqjrfaqt8darhei1toib_data"] }"#,
            reference: r#"{ "configuration": { "mounts": ["vol_${devcontainerId}_data"] } }"#,
            expected: Verdict::Equal,
        },
        // The negative twin, and the reason the rewrite is still SCOPED: a run that is
        // id-SHAPED but sits outside `DEVCONTAINER_ID_FIELDS` is left alone, so two
        // genuinely different values there still diverge.
        Row {
            name: "id-shaped-run-outside-an-id-field-is-compared",
            deacon: r#"{ "name": "x", "image": "img:0uhonu0v70vmigpqqrkg1kqr7ohoam9veqjrfaqt8darhei1toib" }"#,
            reference: r#"{ "configuration": { "name": "x", "image": "img:1og6o4ofpm4echrl8crv0sf9g2btg2i0hgiq83563kvr5k3cfn27" } }"#,
            expected: diverges_at("image"),
        },
        // …but a 12-hex run OUTSIDE the enumerated id fields is NOT collapsed, so two
        // genuinely different digests still diverge (023 T063).
        Row {
            name: "hex-outside-id-fields-still-diverges",
            deacon: r#"{ "customizations": { "d": "0123456789ab" } }"#,
            reference: r#"{ "configuration": { "customizations": { "d": "ffffffffffff" } } }"#,
            expected: diverges_at("customizations.d"),
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
    // For a configuration body carrying no enumerated ABSENT optional, the block
    // extracted by `merged_config` from a `{mergedConfiguration: body}` document must
    // normalize IDENTICALLY to the body unwrapped by `config` from a
    // `{configuration: body}` document — the same named rule chain, the same dynamic-id
    // tokenization.
    //
    // Bodies that DO carry one are excluded and covered by
    // `the_absent_optional_drop_no_longer_touches_the_configuration_block` instead: since
    // #398 `drop_absent_optional` runs on `mergedConfiguration` alone, so demanding
    // identical output there would be demanding the narrowing be undone.
    let bodies = [
        json!({ "name": "demo", "unlisted": {} }),
        json!({ "name": "demo", "remoteUser": "vscode" }),
        json!({ "mounts": ["vol_0123456789ab_data"], "name": "${devcontainerId}" }),
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
            Side::Deacon,
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
            Side::Deacon,
        )
        .expect("merged_config normalizes");

        assert_eq!(
            via_config, via_merged,
            "config and merged_config must agree on the shared block for body {body}"
        );
    }
}

#[test]
fn every_difference_is_reported_in_a_stable_order() {
    // A pair carrying a reference-only key, a deacon-only key and a differing value at
    // once must report ALL THREE, in the same order no matter which caller normalized it.
    // The deacon-only one is the load-bearing member: a comparison that treated the
    // reference as the truth would drop it, and a key deacon emits and the reference does
    // not is either a genuine extension or a genuine over-emission, never noise (FR-020).
    let deacon = r#"{ "name": "a", "extra": 1 }"#;
    let reference = r#"{ "configuration": { "name": "b", "dropped": 2 } }"#;
    let a = config_verdict("runner-a", deacon, reference);
    let b = config_verdict("runner-b", deacon, reference);
    assert_eq!(a, b);
    assert_eq!(
        a,
        Verdict::Divergent(vec![
            "dropped".to_string(), // reference-only
            "extra".to_string(),   // deacon-only
            "name".to_string(),    // differing value
        ]),
        "every difference must be reported, in a single-sourced stable order"
    );
}

// ===========================================================================
// T090 (US6, FR-029/FR-030, Constitution VIII): ONE comparison implementation.
// ===========================================================================

/// The migration's end state is one normalizer and one diff. A second implementation is
/// the failure this guards, and it is insidious rather than obvious: two normalizers that
/// agree today drift apart later, and the drift shows up as a parity result that changes
/// depending on which caller produced it — exactly the class of bug that made the
/// `equivalence-report` stale-binary defect so hard to see.
///
/// Checked structurally, on the property that matters: every entry point that normalizes a
/// resolved-configuration document must route through the SAME rule chain, so the same
/// body cannot compare differently depending on who asked.
#[test]
fn every_config_entry_point_routes_through_one_rule_chain() {
    let body = json!({
        "name": "demo",
        "image": null,
        "customizations": {},
        "unlistedEmpty": {},
        "mounts": ["vol_0123456789ab_data"],
        "forwardPorts": [3000],
    });

    // 1. the legacy `configuration` entry point
    let via_config = normalize::config(
        "one",
        &Value::Object(
            [("configuration".to_string(), body.clone())]
                .into_iter()
                .collect(),
        )
        .to_string(),
        Side::Deacon,
    )
    .expect("config normalizes");

    // 2. the legacy `mergedConfiguration` entry point
    let via_merged = normalize::merged_config(
        "one",
        &Value::Object(
            [("mergedConfiguration".to_string(), body.clone())]
                .into_iter()
                .collect(),
        )
        .to_string(),
        Side::Deacon,
    )
    .expect("merged_config normalizes");

    // 3. the rule chain the declarative `chan-structured-output` channel applies. It is
    //    handed the WHOLE CLI document, so the same body is reached through the wrapper
    //    key rather than as the root — which is exactly the shape the channel sees.
    let wrapped = Value::Object(
        [("configuration".to_string(), body.clone())]
            .into_iter()
            .collect(),
    );
    let via_rules = normalize::config_document_rules(
        &wrapped,
        Side::Deacon,
        DocumentBlock::Wrapper,
    )["configuration"]
        .clone();

    let merged_wrapped = Value::Object(
        [("mergedConfiguration".to_string(), body.clone())]
            .into_iter()
            .collect(),
    );
    let via_merged_rules = normalize::config_document_rules(
        &merged_wrapped,
        Side::Deacon,
        DocumentBlock::Wrapper,
    )["mergedConfiguration"]
        .clone();

    // Each legacy entry point must agree with the declarative channel ON ITS OWN BLOCK.
    // The two blocks legitimately differ — `drop_absent_optional` runs on
    // `mergedConfiguration` and, since #398, never on `configuration` — so asserting the
    // two entry points equal EACH OTHER would be asserting the narrowing away. What must
    // hold is that there is one rule chain, not two implementations (Constitution VIII,
    // FR-030).
    assert_eq!(
        via_config, via_rules,
        "the legacy `configuration` entry point and the declarative channel must share \
         ONE rule chain"
    );
    assert_eq!(
        via_merged, via_merged_rules,
        "the legacy `mergedConfiguration` entry point and the declarative channel must \
         share ONE rule chain"
    );
    assert_ne!(
        via_config, via_merged,
        "the blocks are expected to differ: an authored empty is authorship information \
         on `configuration` and a computed default on `mergedConfiguration`"
    );
}

/// #398: `drop_absent_optional` no longer touches the `configuration` block on EITHER
/// side. It existed because deacon serialized every modeled optional unconditionally;
/// deacon now omits what the author did not write, so an empty value on that block is the
/// author's on both sides — and dropping it from deacon's copy alone would turn an
/// agreement into a reported divergence.
#[test]
fn the_absent_optional_drop_no_longer_touches_the_configuration_block() {
    let raw = json!({ "configuration": { "name": "demo", "forwardPorts": [], "image": null } })
        .to_string();

    let authored = json!({ "name": "demo", "forwardPorts": [], "image": null });
    for side in [Side::Deacon, Side::Oracle] {
        assert_eq!(
            normalize::config("side", &raw, side).expect("normalize"),
            authored,
            "an authored empty survives on {side:?}'s configuration block — eliding it \
             is what made an authored empty, an authored null and an omission the same \
             observation (FR-055)"
        );
    }

    // `mergedConfiguration` is SYNTHESIZED rather than echoed, and the rule still applies
    // to both sides there: the pinned reference emits computed `containerEnv: {}` /
    // `remoteEnv: {}` / `portsAttributes: {}` that deacon omits
    // (`bhv-readconfig-merged-computed-empties-omitted`), and neither carries an
    // authorship signal.
    let merged =
        json!({ "mergedConfiguration": { "name": "demo", "containerEnv": {} } }).to_string();
    for side in [Side::Deacon, Side::Oracle] {
        assert_eq!(
            normalize::merged_config("side", &merged, side).expect("normalize"),
            json!({ "name": "demo" }),
            "the merged block is a computed default on {side:?}'s side"
        );
    }
}

/// No second implementation of a comparison or normalization rule exists in the tree.
///
/// A source scan, because the property is about what EXISTS, not about what a particular
/// call returns: a duplicate that nothing currently calls is still a duplicate waiting to
/// be called. The retired blanket rules are named explicitly — they were deleted rather
/// than renamed, and a function reappearing under those names would mean research D3's
/// defect came back wearing a label.
#[test]
fn no_second_normalization_or_comparison_implementation_exists() {
    /// Signatures that would constitute a second implementation.
    const FORBIDDEN_DEFINITIONS: &[&str] = &[
        "fn prune(",
        "fn replace_hex12(",
        "fn sanitize_dynamic_values(",
        "fn drop_empty_values(",
    ];
    /// The ONE file allowed to define the normalization rules.
    const NORMALIZER: &str = "crates/parity-harness/src/normalize.rs";
    /// The ONE file allowed to define the comparison.
    const COMPARISON: &str = "crates/parity-harness/src/compare.rs";

    let root = parity_harness::workspace_root();
    let mut offenders: Vec<String> = Vec::new();
    let mut scanned = 0usize;

    for dir in ["crates/parity-harness/src"] {
        let mut stack = vec![root.join(dir)];
        while let Some(current) = stack.pop() {
            let Ok(entries) = std::fs::read_dir(&current) else {
                continue;
            };
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    stack.push(path);
                    continue;
                }
                if path.extension().and_then(|s| s.to_str()) != Some("rs") {
                    continue;
                }
                let Ok(text) = std::fs::read_to_string(&path) else {
                    continue;
                };
                scanned += 1;
                let rel = path
                    .strip_prefix(&root)
                    .unwrap_or(&path)
                    .to_string_lossy()
                    .replace('\\', "/");
                for forbidden in FORBIDDEN_DEFINITIONS {
                    if text.contains(forbidden) {
                        offenders.push(format!(
                            "{rel}: defines `{forbidden}` — a retired blanket rule (023 \
                             T062/T063) must not return under any name"
                        ));
                    }
                }
                // `compare::verdict_differential` is the single comparison. A second
                // config differ living in the normalizer is what this file used to call
                // through, and it was retired precisely so there is one.
                if rel != NORMALIZER && text.contains("pub fn diff(deacon:") {
                    offenders.push(format!("{rel}: defines a second config `diff`"));
                }
                if rel != COMPARISON && text.contains("pub fn verdict_differential(") {
                    offenders.push(format!("{rel}: defines a second differential comparison"));
                }
            }
        }
    }

    assert!(
        scanned > 10,
        "expected to scan the harness sources, only saw {scanned} file(s)"
    );
    assert!(
        offenders.is_empty(),
        "a second comparison/normalization implementation exists (FR-030):\n{}",
        offenders.join("\n")
    );
}

// ===========================================================================
// 024 US5 (T111/T113): the three FR-055 states, and the V24 rule contract.
// ===========================================================================

/// **T111 / FR-055, US5 acceptance scenario 2.** An authored `null`, an authored empty
/// collection, and an OMITTED property must produce THREE distinguishable observations.
///
/// This is the property the pre-024-US5 normalizer destroyed. `drop_absent_optional` ran on
/// both sides of every differential, so all three states normalized to "the key is absent on
/// both sides" — one observation where the requirement asks for three, and a comparison that
/// was green while proving nothing about any of them.
///
/// Checked on BOTH sides. Since #398 retired `drop_absent_optional` on the `configuration`
/// block, the normalizer preserves all three states on either side — so what a verdict shows
/// is now exactly what the two CLIs actually emitted, with no compensation in between.
///
/// The residual deacon defect is narrower than it was and is asserted against deacon's REAL
/// output, not simulated by feeding the reference's document to deacon's side: deacon now
/// preserves an authored empty collection (it agrees with the reference), and collapses only
/// an authored `null` into an omission — `bhv-readconfig-authored-null-omitted-collapsed`.
#[test]
fn null_empty_and_omitted_are_three_distinguishable_observations() {
    let doc = |body: &str| format!(r#"{{ "configuration": {{ "name": "demo"{body} }} }}"#);
    let authored_null = doc(r#", "forwardPorts": null"#);
    let authored_empty = doc(r#", "forwardPorts": []"#);
    let omitted = doc("");

    let observe = |raw: &str, side: Side| normalize::config("fr055", raw, side).expect("normalize");

    let ref_null = observe(&authored_null, Side::Oracle);
    let ref_empty = observe(&authored_empty, Side::Oracle);
    let ref_omitted = observe(&omitted, Side::Oracle);

    assert_ne!(
        ref_null, ref_empty,
        "an authored null and an authored empty collection are different documents"
    );
    assert_ne!(ref_null, ref_omitted, "an authored null is not an omission");
    assert_ne!(
        ref_empty, ref_omitted,
        "an authored empty collection is not an omission"
    );
    assert_eq!(ref_null["forwardPorts"], json!(null));
    assert_eq!(ref_empty["forwardPorts"], json!([]));
    assert!(ref_omitted.get("forwardPorts").is_none());

    // The normalizer no longer collapses anything on this block, so deacon's side keeps
    // whatever deacon emitted — the three documents stay three observations there too.
    assert_ne!(
        observe(&authored_null, Side::Deacon),
        observe(&omitted, Side::Deacon),
        "the normalizer must not re-merge an authored null with an omission on deacon's \
         side either; what deacon's SERIALIZER does is measured separately below"
    );

    // Now against the documents deacon ACTUALLY emits (measured against the pinned oracle
    // at #398). An authored empty collection is preserved and agrees; an authored `null`
    // is still collapsed into an omission, which is the one remaining half of #398 and is
    // characterized as `bhv-readconfig-authored-null-omitted-collapsed`.
    let deacon_doc = |body: &str| format!(r#"{{ "configuration": {{ "name": "demo"{body} }} }}"#);
    let verdict = |deacon_raw: &str, reference_raw: &str| {
        let d = observe(deacon_raw, Side::Deacon);
        let r = observe(reference_raw, Side::Oracle);
        diverging_paths(&d, &r)
    };
    assert!(
        verdict(&deacon_doc(""), &omitted).is_empty(),
        "an omitted property agrees on both sides"
    );
    assert!(
        verdict(&deacon_doc(r#", "forwardPorts": []"#), &authored_empty).is_empty(),
        "an authored empty collection now agrees — deacon emits it, the reference emits \
         it, and nothing in between hides either"
    );
    assert_eq!(
        verdict(&deacon_doc(""), &authored_null),
        vec!["forwardPorts".to_string()],
        "an authored null is the residual divergence: deacon omits the key, the reference \
         reports `null`"
    );
    // The reference's own answer is what tells the two authored states apart, and it is
    // retained in the normalized evidence rather than normalized away.
    assert_ne!(ref_null["forwardPorts"], ref_empty["forwardPorts"]);
}
