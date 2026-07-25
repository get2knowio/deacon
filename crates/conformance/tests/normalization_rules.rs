//! T057 (US4, FR-021 / FR-047): **V24** — a normalization rule must say where it applies
//! and, if it removes anything, exactly what and why.
//!
//! A normalization rule decides what a comparison is allowed to ignore. Left
//! unconstrained it is the most effective way to make a parity suite pass while proving
//! less — and unlike a weakened assertion it is invisible in the test data, because the
//! case still declares the channel and still reports `agree`. Hence the rule registry and
//! this guard.
//!
//! The distinction the guard draws: an **undeclared** blanket rule is a V24 violation and
//! blocks; a **declared** deficiency (`known_non_compliant`, carrying a reason that names
//! a tracked follow-up) is reported as debt. That is the same discipline this feature uses
//! for residuals versus gaps — an admitted, queued problem is debt; an unadmitted one
//! blocks. Declaring is a conspicuous source edit with a mandatory tracked reason, so it
//! is not a cheap escape.
//!
//! Hermetic: pure functions over synthetic rules plus the real registry constant.

use deacon_conformance::conservation::{
    NORMALIZATION_RULES, NormalizationRule, RuleAction, check_normalization_rules,
    declared_non_compliant_rules,
};

fn rewrite(scopes: &'static [&'static str]) -> NormalizationRule {
    NormalizationRule {
        name: "probe",
        scopes,
        action: RuleAction::Rewrite,
        removes: &[],
        justification: Some("probe"),
        known_non_compliant: None,
    }
}

fn drop_rule(
    removes: &'static [&'static str],
    justification: Option<&'static str>,
) -> NormalizationRule {
    NormalizationRule {
        name: "probe",
        scopes: &["field:/configuration"],
        action: RuleAction::Drop,
        removes,
        justification,
        known_non_compliant: None,
    }
}

fn messages(rules: &[NormalizationRule]) -> Vec<String> {
    check_normalization_rules(rules)
        .into_iter()
        .inspect(|v| assert_eq!(v.code, "V24", "this guard only produces V24"))
        .map(|v| v.message)
        .collect()
}

#[test]
fn a_rule_with_no_scope_fails() {
    let problems = messages(&[rewrite(&[])]);
    assert!(
        problems.iter().any(|m| m.contains("declares no scope")),
        "{problems:#?}"
    );
}

#[test]
fn an_all_scope_fails() {
    for scope in ["all", "*", "any", "global", "everything", "-", ""] {
        let leaked: &'static [&'static str] = Box::leak(vec![scope].into_boxed_slice());
        let problems = messages(&[rewrite(leaked)]);
        assert!(
            problems.iter().any(|m| m.contains("is not a scope")),
            "scope {scope:?} must be rejected: {problems:#?}"
        );
    }
}

#[test]
fn a_scope_that_is_neither_channel_nor_field_qualified_fails() {
    let problems = messages(&[rewrite(&["chan-stdout"])]);
    assert!(
        problems
            .iter()
            .any(|m| m.contains("`channel:<chan-id>` or `field:<json-pointer>`")),
        "{problems:#?}"
    );
}

#[test]
fn a_drop_without_a_justification_fails() {
    let problems = messages(&[drop_rule(&["appPort"], None)]);
    assert!(
        problems
            .iter()
            .any(|m| m.contains("requires a justification")),
        "a drop loses information, so it must say why that is sound: {problems:#?}"
    );

    let blank = messages(&[drop_rule(&["appPort"], Some("   "))]);
    assert!(
        blank.iter().any(|m| m.contains("requires a justification")),
        "a blank justification is as absent as a missing one: {blank:#?}"
    );
}

#[test]
fn a_drop_with_an_empty_removes_list_fails() {
    let problems = messages(&[drop_rule(&[], Some("because"))]);
    assert!(
        problems
            .iter()
            .any(|m| m.contains("must enumerate the field names")),
        "an empty removal list is an unbounded removal set: {problems:#?}"
    );
}

#[test]
fn an_open_ended_removes_entry_fails() {
    // Every shape of "a category rather than a field name" (FR-021).
    for (entry, why) in [
        ("devcontainer.*", "a glob"),
        ("com.docker.", "a prefix"),
        ("noise_", "a prefix"),
        ("every", "a category predicate"),
        ("empty", "a category predicate"),
        ("null", "a category predicate"),
        ("", "empty"),
    ] {
        let leaked: &'static [&'static str] = Box::leak(vec![entry].into_boxed_slice());
        let problems = messages(&[drop_rule(leaked, Some("because"))]);
        assert!(
            problems.iter().any(|m| m.contains("is open-ended")),
            "removes entry {entry:?} ({why}) must be rejected: {problems:#?}"
        );
    }
}

#[test]
fn a_non_drop_rule_declaring_removes_fails() {
    let mut rule = rewrite(&["channel:chan-stdout"]);
    rule.removes = &["appPort"];
    let problems = messages(&[rule]);
    assert!(
        problems
            .iter()
            .any(|m| m.contains("only a `drop` removes anything")),
        "{problems:#?}"
    );
}

#[test]
fn a_compliant_enumerated_drop_passes() {
    let problems = messages(&[drop_rule(
        &["appPort", "workspaceMount"],
        Some(
            "both are absent-valued optional properties deacon serializes and the \
              reference omits",
        ),
    )]);
    assert!(
        problems.is_empty(),
        "an enumerated, justified, scoped drop is exactly what FR-021 permits: {problems:#?}"
    );
}

#[test]
fn a_declared_deficiency_is_debt_not_a_blocker_but_must_be_tracked() {
    // Declared WITHOUT a tracked follow-up → still a V24 violation. Declaring is not a
    // way to park a problem.
    let mut untracked = drop_rule(&["devcontainer.*"], Some("because"));
    untracked.known_non_compliant = Some("it matches prefixes and we know it");
    let problems = messages(&[untracked]);
    assert!(
        problems
            .iter()
            .any(|m| m.contains("without a reason naming a tracked follow-up")),
        "an untracked declaration must still fail: {problems:#?}"
    );

    // Declared WITH a tracked follow-up → reported as debt, no V24.
    let mut tracked = drop_rule(&["devcontainer.*"], Some("because"));
    tracked.known_non_compliant =
        Some("prefix matching, not an enumerated set; tracked at specs/x/tasks.md#T112");
    assert!(
        messages(&[tracked]).is_empty(),
        "an admitted, tracked deficiency is debt, like a residual — not a blocker"
    );
    assert_eq!(
        declared_non_compliant_rules(&[tracked]).len(),
        1,
        "…but it MUST appear in the declared-deficiency report"
    );
}

#[test]
fn the_real_rule_registry_has_no_undeclared_blanket_rules() {
    let violations = check_normalization_rules(NORMALIZATION_RULES);
    assert!(
        violations.is_empty(),
        "every registered rule must be scoped and justified, or declare its deficiency \
         with a tracked follow-up:\n{violations:#?}"
    );
}

#[test]
fn the_retired_blanket_rules_are_not_registered() {
    // `prune` and `replace_hex12`/`sanitize_dynamic_values` are the two rules research D3
    // identifies as blanket. They were retired (023 T062/T063), not renamed — a rule
    // named after them reappearing here would mean the defect came back with a label on.
    for retired in [
        "prune",
        "replace_hex12",
        "sanitize_dynamic_values",
        "drop_empty_values",
    ] {
        assert!(
            !NORMALIZATION_RULES.iter().any(|r| r.name == retired),
            "`{retired}` is a blanket rule and must not be registered (FR-021)"
        );
    }
}

#[test]
fn the_declared_deficiency_set_is_exactly_the_one_known_case() {
    let declared = declared_non_compliant_rules(NORMALIZATION_RULES);
    let names: Vec<&str> = declared.iter().map(|(n, _)| *n).collect();
    assert_eq!(
        names,
        vec!["strip_intentional_labels"],
        "data-model §6 names exactly one non-compliant existing rule; a second one \
         appearing here needs its own review"
    );
    for (name, reason) in &declared {
        assert!(
            reason.contains(".md#") || reason.contains('#'),
            "{name}'s deficiency must name a tracked follow-up"
        );
    }
}
