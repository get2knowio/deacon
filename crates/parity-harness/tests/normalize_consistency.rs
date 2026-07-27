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
        // Equal after the named rules: the reference's wrapper is unwrapped and an
        // ENUMERATED absent optional (`customizations: {}`) is elided by
        // `drop_absent_optional`; nothing else differs. (023 T062 — `prune` used to
        // drop ANY empty value and `configFilePath` too; both are now compared unless
        // the key is on the enumerated list.)
        Row {
            name: "equal-after-named-rules",
            deacon: r#"{ "name": "demo" }"#,
            reference: r#"{ "configuration": { "name": "demo", "customizations": {} } }"#,
            expected: Verdict::Equal,
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
    // For any configuration body, the block extracted by `merged_config` from a
    // `{mergedConfiguration: body}` document must normalize IDENTICALLY to the body
    // unwrapped by `config` from a `{configuration: body}` document — the same named
    // rule chain, the same dynamic-id tokenization. Reusing the equivalence-table
    // bodies keeps the two entry points provably in lockstep on the shared block.
    let bodies = [
        json!({ "name": "demo", "customizations": {}, "image": null, "unlisted": {} }),
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

    assert_eq!(
        via_config, via_merged,
        "the two legacy entry points must share one definition of equivalence"
    );
    assert_eq!(
        via_config, via_rules,
        "the legacy entry points and the declarative channel must share ONE rule chain — \
         a second implementation is what Constitution VIII forbids (FR-030)"
    );
}

/// 024 US5 (T123): `drop_absent_optional` is narrowed to the SIDE whose serializer defect
/// it compensates. On the reference's `configuration` block — an echo of the authored
/// document — nothing is elided, because an empty value there is the AUTHOR's.
#[test]
fn the_absent_optional_drop_applies_to_deacons_configuration_only() {
    let raw = json!({ "configuration": { "name": "demo", "forwardPorts": [], "image": null } })
        .to_string();

    let deacon = normalize::config("side", &raw, Side::Deacon).expect("normalize");
    let reference = normalize::config("side", &raw, Side::Oracle).expect("normalize");

    assert_eq!(
        deacon,
        json!({ "name": "demo" }),
        "deacon's side still elides the enumerated absent optionals it serializes \
         unconditionally"
    );
    assert_eq!(
        reference,
        json!({ "name": "demo", "forwardPorts": [], "image": null }),
        "the reference's side keeps them — eliding them there is what made an authored \
         empty, an authored null and an omission the same observation (FR-055)"
    );

    // `mergedConfiguration` is synthesized by BOTH CLIs, so the rule still applies to
    // both there: the pinned reference emits its own computed `containerEnv: {}` /
    // `remoteEnv: {}` / `portsAttributes: {}`, which carry no authorship signal.
    let merged =
        json!({ "mergedConfiguration": { "name": "demo", "containerEnv": {} } }).to_string();
    assert_eq!(
        normalize::merged_config("side", &merged, Side::Oracle).expect("normalize"),
        json!({ "name": "demo" }),
        "the merged block is a computed default on both sides"
    );
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
/// Checked on the REFERENCE side, because that is the side that has the information: its
/// `configuration` is an echo of the authored document. The deacon side is asserted too, and
/// deliberately: it collapses two of the three, which is the characterized defect
/// (`bhv-readconfig-authored-empty-omitted-collapsed`) and must stay visible here rather than
/// being quietly re-hidden by a future widening of the rule.
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

    // deacon's side: the characterized collapse, pinned so it cannot widen unnoticed.
    let d_null = observe(&authored_null, Side::Deacon);
    let d_empty = observe(&authored_empty, Side::Deacon);
    let d_omitted = observe(&omitted, Side::Deacon);
    assert_eq!(
        d_null, d_omitted,
        "deacon cannot distinguish an authored null from an omission — the defect the \
         differential must now SHOW rather than normalize away"
    );
    assert_eq!(d_empty, d_omitted, "…nor an authored empty collection");

    // And therefore the three states yield three distinguishable VERDICTS: two divergences
    // that differ in what the reference held, and one agreement.
    let verdict = |raw: &str| {
        let d = observe(raw, Side::Deacon);
        let r = observe(raw, Side::Oracle);
        normalize::diff(&d, &r)
            .iter()
            .map(|x| format!("{:?}:{}", x.kind, x.path))
            .collect::<Vec<_>>()
    };
    let v_null = verdict(&authored_null);
    let v_empty = verdict(&authored_empty);
    let v_omitted = verdict(&omitted);
    assert!(
        v_omitted.is_empty(),
        "an omitted property agrees on both sides: {v_omitted:?}"
    );
    assert_eq!(v_null, vec!["RefOnly:forwardPorts".to_string()]);
    assert_eq!(v_empty, vec!["RefOnly:forwardPorts".to_string()]);
    // The two divergences agree on SHAPE, so the recorded evidence is what separates them.
    assert_ne!(
        ref_null["forwardPorts"], ref_empty["forwardPorts"],
        "the two divergences are told apart by the value the reference reported, which is \
         retained in the normalized evidence"
    );
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
