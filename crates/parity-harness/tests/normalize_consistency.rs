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
use parity_harness::exec::Side;
use parity_harness::normalize::DocumentBlock;
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
        match normalize::config(case, raw, Side::Deacon) {
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
            expected: Verdict::Divergent(vec![DiffKind::RefOnly]),
        },
        // An UNLISTED empty value is now compared rather than silently dropped — the
        // regression `prune` made invisible (023 T062).
        Row {
            name: "unlisted-empty-is-compared",
            deacon: r#"{ "name": "demo", "someNewProperty": {} }"#,
            reference: r#"{ "configuration": { "name": "demo" } }"#,
            expected: Verdict::Divergent(vec![DiffKind::DeaconOnly]),
        },
        // `configFilePath` is no longer dropped: the reference emits it and deacon does
        // not, and that is now REPORTED (research D3).
        Row {
            name: "config-file-path-is-compared",
            deacon: r#"{ "name": "demo" }"#,
            reference: r#"{ "configuration": { "name": "demo",
                           "configFilePath": "/w/.devcontainer/devcontainer.json" } }"#,
            expected: Verdict::Divergent(vec![DiffKind::RefOnly]),
        },
        // The reference keeps a key deacon dropped: highest-signal ref-only.
        Row {
            name: "ref-only-key",
            deacon: r#"{ "name": "demo" }"#,
            reference: r#"{ "configuration": { "name": "demo", "remoteUser": "vscode" } }"#,
            expected: Verdict::Divergent(vec![DiffKind::RefOnly]),
        },
        // deacon emits a key the reference lacks: a deacon-only finding, reported with
        // the same significance as any other class (023 T065, FR-020).
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
        // The ONLY difference is a dynamic id inside an ENUMERATED id-bearing field
        // (`mounts`): deacon emits a 12-hex hash where the reference emits the
        // `${devcontainerId}` template. Both tokenize to <ID>, so this is not a
        // divergence.
        Row {
            name: "dynamic-id-only",
            deacon: r#"{ "mounts": ["vol_0123456789ab_data"] }"#,
            reference: r#"{ "configuration": { "mounts": ["vol_${devcontainerId}_data"] } }"#,
            expected: Verdict::Equal,
        },
        // …but a 12-hex run OUTSIDE the enumerated id fields is NOT collapsed, so two
        // genuinely different digests still diverge (023 T063).
        Row {
            name: "hex-outside-id-fields-still-diverges",
            deacon: r#"{ "customizations": { "d": "0123456789ab" } }"#,
            reference: r#"{ "configuration": { "customizations": { "d": "ffffffffffff" } } }"#,
            expected: Verdict::Divergent(vec![DiffKind::Value]),
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
fn diff_ordering_is_stable_regardless_of_input_order() {
    // A pair carrying all three divergence classes at once must order identically no
    // matter which caller normalizes it. Order is deterministic (class, then path) and
    // NOT a significance ranking — `deacon-only` no longer sorts last as "default
    // noise" (023 T065, FR-020).
    let deacon = r#"{ "name": "a", "extra": 1 }"#;
    let reference = r#"{ "configuration": { "name": "b", "dropped": 2 } }"#;
    let a = config_verdict("runner-a", deacon, reference);
    let b = config_verdict("runner-b", deacon, reference);
    assert_eq!(a, b);
    assert_eq!(
        a,
        Verdict::Divergent(vec![
            DiffKind::RefOnly,
            DiffKind::DeaconOnly,
            DiffKind::Value
        ]),
        "divergence order must be single-sourced and stable"
    );
}

/// 023 T061: the `drop_absent_optional` rule's registered `removes` list MUST equal the
/// key list the implementation actually uses.
///
/// The registry is the reviewable statement of what the comparison elides; if it can
/// drift from the code, it documents a fiction. The two live in different crates
/// (`deacon-conformance` owns the registry, `parity-harness` owns the normalizer), so
/// this test is the only place they can be compared — and it belongs here, in the crate
/// that can see both.
#[test]
fn the_registered_removes_list_matches_the_implementation() {
    let registered = deacon_conformance::conservation::NORMALIZATION_RULES
        .iter()
        .find(|r| r.name == "drop_absent_optional")
        .expect("`drop_absent_optional` is registered");

    assert_eq!(
        registered.removes,
        normalize::ABSENT_OPTIONAL_KEYS,
        "the registered `removes` list and `normalize::ABSENT_OPTIONAL_KEYS` must stay in \
         lockstep — a registry that can drift from the code documents a fiction"
    );
    assert_eq!(
        registered.action,
        deacon_conformance::conservation::RuleAction::Drop
    );
}

/// 023 T063: the `devcontainer_id_token` rule is registered as a REWRITE with an empty
/// `removes` — it substitutes, it never deletes.
#[test]
fn the_dynamic_id_rule_is_registered_as_a_rewrite() {
    let registered = deacon_conformance::conservation::NORMALIZATION_RULES
        .iter()
        .find(|r| r.name == "devcontainer_id_token")
        .expect("`devcontainer_id_token` is registered");
    assert_eq!(
        registered.action,
        deacon_conformance::conservation::RuleAction::Rewrite
    );
    assert!(
        registered.removes.is_empty(),
        "a rewrite removes nothing (FR-024)"
    );
    assert!(
        !normalize::DEVCONTAINER_ID_FIELDS.is_empty(),
        "the hex rewrite must be confined to an enumerated, non-empty field set"
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

    let root = parity_harness::workspace_root();
    let mut offenders: Vec<String> = Vec::new();
    let mut scanned = 0usize;

    for dir in ["crates/parity-harness/src", "crates/conformance/src"] {
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
                // `normalize::diff` is the single ranked config differ.
                if rel != NORMALIZER && text.contains("pub fn diff(deacon:") {
                    offenders.push(format!("{rel}: defines a second config `diff`"));
                }
            }
        }
    }

    assert!(
        scanned > 10,
        "expected to scan the harness + conformance sources, only saw {scanned} file(s)"
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
        normalize::diff(&d, &r)
            .iter()
            .map(|x| format!("{:?}:{}", x.kind, x.path))
            .collect::<Vec<_>>()
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
        vec!["RefOnly:forwardPorts".to_string()],
        "an authored null is the residual divergence: deacon omits the key, the reference \
         reports `null`"
    );
    // The reference's own answer is what tells the two authored states apart, and it is
    // retained in the normalized evidence rather than normalized away.
    assert_ne!(ref_null["forwardPorts"], ref_empty["forwardPorts"]);
}

/// **T113 / FR-056, US5 acceptance scenario 4.** A normalization rule that removes or
/// collapses observable content must be named, scoped to a specific field or channel, and
/// justified; an UNSCOPED rule is rejected (V24).
///
/// Two halves, and both matter. The first is that the real registry is clean. The second is
/// that the check is LIVE — a guard that reports nothing because it accepts everything is
/// indistinguishable from a clean registry, so each way a rule can be unscoped is fed to it
/// as a negative control and must be reported.
#[test]
fn an_unscoped_normalization_rule_is_rejected() {
    use deacon_conformance::conservation::{
        NORMALIZATION_RULES, NormalizationRule, RuleAction, check_normalization_rules,
    };

    assert!(
        check_normalization_rules(NORMALIZATION_RULES).is_empty(),
        "the shipped rule set must be clean; a V24 here is a real finding, not a test bug"
    );

    let ok = NormalizationRule {
        name: "us5_probe",
        scopes: &["channel:chan-container-state"],
        action: RuleAction::Drop,
        removes: &["SOME_KEY"],
        justification: Some("a scoped, enumerated, justified drop"),
        known_non_compliant: None,
    };
    assert!(
        check_normalization_rules(&[ok]).is_empty(),
        "a well-formed rule must NOT be reported, or the check proves nothing"
    );

    for (label, rule) in [
        ("no scope at all", NormalizationRule { scopes: &[], ..ok }),
        (
            "an `all` pseudo-scope",
            NormalizationRule {
                scopes: &["all"],
                ..ok
            },
        ),
        (
            "an unqualified scope",
            NormalizationRule {
                scopes: &["container-state"],
                ..ok
            },
        ),
        (
            "an open-ended removal set",
            NormalizationRule {
                removes: &["devcontainer.*"],
                ..ok
            },
        ),
        (
            "a drop with no justification",
            NormalizationRule {
                justification: None,
                ..ok
            },
        ),
    ] {
        let problems = check_normalization_rules(&[rule]);
        assert!(
            !problems.is_empty(),
            "V24 must reject {label}; a rule a reviewer cannot bound by reading its registry \
             entry is the blanket rule FR-056 forbids"
        );
    }
}

/// Every rule the 024 US5 audit touched is registered, and each registration says the same
/// thing the implementation does.
///
/// `compose_project_prefix` and `user_default_root` were applied for two stories without
/// appearing in the registry at all, which is the failure mode V24 cannot catch on its own:
/// it validates the rules it is GIVEN, and an unregistered rule is never given to it.
#[test]
fn the_us5_audited_rules_are_registered_with_the_right_action() {
    use deacon_conformance::conservation::{NORMALIZATION_RULES, RuleAction};

    for (name, action) in [
        ("compose_project_prefix", RuleAction::Rewrite),
        ("container_hostname_token", RuleAction::Rewrite),
        ("user_default_root", RuleAction::Canonicalize),
        ("drop_noise_env", RuleAction::Drop),
        ("drop_absent_optional", RuleAction::Drop),
    ] {
        let rule = NORMALIZATION_RULES
            .iter()
            .find(|r| r.name == name)
            .unwrap_or_else(|| panic!("`{name}` must be registered"));
        assert_eq!(rule.action, action, "`{name}` action");
        assert!(
            rule.justification.is_some_and(|j| !j.trim().is_empty()),
            "`{name}` must carry a justification"
        );
        if action == RuleAction::Rewrite || action == RuleAction::Canonicalize {
            assert!(
                rule.removes.is_empty(),
                "`{name}` rewrites or canonicalizes; it must remove nothing"
            );
        }
    }

    // `drop_noise_env` no longer runs at capture, so its registered scope must no longer
    // claim the declarative channel — the registry entry is the reviewable statement of
    // where a rule reaches, and it read as a channel-wide removal while it was one.
    let noise = NORMALIZATION_RULES
        .iter()
        .find(|r| r.name == "drop_noise_env")
        .expect("registered");
    assert!(
        !noise.scopes.contains(&"channel:chan-container-state"),
        "`drop_noise_env` applies to the legacy comparison, not to the channel; a scope \
         that overstates a drop's reach is how PATH stayed uncompared"
    );
}
