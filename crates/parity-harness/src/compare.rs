//! Per-channel comparison → [`ChannelVerdict`] (T021, 022-conformance-runner).
//!
//! Compares NORMALIZED evidence (never raw) for each declared channel and produces a
//! verdict. Two entry points:
//! - [`verdict_spec_expectation`] evaluates a declared `assertion` against one side's
//!   normalized evidence (no reference run);
//! - [`verdict_differential`] compares deacon's normalized evidence to the reference's
//!   for the same channel.
//!
//! Allowed-difference integration (US4, T064): a divergence whose every diverging
//! observable path is covered by a scoped [`Tolerances`] entry becomes
//! `allowed-difference`; an uncovered divergence stays `diverge`. Detail payloads are
//! path-free and deterministic so the verdict report stays byte-stable (contract
//! runner-cli.md, T018).

use serde_json::{Value, json};

use deacon_conformance::model::{
    CHAN_EXIT_CODE, CHAN_FILE_CONTENT, CHAN_FILESYSTEM, CHAN_IMAGE, CHAN_INJECTED_PROCESS,
    CHAN_PROCESS_GRAPH, CHAN_STDERR, CHAN_STDOUT, CHAN_STRUCTURED_OUTPUT, CHAN_TEMPORAL,
};

use crate::HarnessError;
use crate::evidence::{ChannelVerdict, NormalizedChannelEvidence, Outcome};

/// Evaluate a declared `assertion` against one side's normalized `evidence` for
/// `channel` (spec-expectation oracle). Returns an `agree`/`diverge` verdict, or a
/// fail-loud [`HarnessError`] when the assertion is malformed or targets a channel this
/// comparison does not understand (a case-authoring error, constitution IV).
pub fn verdict_spec_expectation(
    channel: &str,
    evidence: &NormalizedChannelEvidence,
    assertion: &Value,
) -> Result<ChannelVerdict, HarnessError> {
    let outcome = evaluate_assertion(channel, evidence, assertion)?;
    Ok(match outcome {
        AssertionResult::Pass => agree(channel),
        AssertionResult::Fail(detail) => diverge(channel, detail),
    })
}

/// Compare deacon's normalized evidence to the reference's for `channel`
/// (live-differential oracle). Equal normalized values agree; otherwise diverge with a
/// path-free detail. `present:false` on either side is itself part of the comparison
/// (not-captured must match not-captured — FR-018).
///
/// A divergence whose EVERY diverging observable path is covered by a scoped
/// [`Tolerances`] entry becomes `allowed-difference` (with the backing waiver/divergence
/// ids in the detail, FR-033); a divergence with any UNCOVERED path stays `diverge`. Each
/// consumed tolerance's key is recorded in `consumed` so the caller can report the
/// unconsumed (stale) ones — the same self-invalidating pattern as the registry waiver
/// check (FR-034, reused, not re-implemented).
pub fn verdict_differential(
    channel: &str,
    deacon: &NormalizedChannelEvidence,
    reference: &NormalizedChannelEvidence,
    tolerances: &Tolerances<'_>,
    consumed: &mut std::collections::HashSet<String>,
) -> ChannelVerdict {
    // The diverging observable paths (channel-prefixed dotted paths).
    let mut diverging: Vec<String> = Vec::new();
    if deacon.present != reference.present {
        diverging.push(channel.to_string());
    } else if deacon.present {
        diff_paths(channel, &deacon.value, &reference.value, &mut diverging);
    }
    if diverging.is_empty() {
        return agree(channel);
    }

    let mut uncovered: Vec<String> = Vec::new();
    let mut covered: Vec<Value> = Vec::new();
    for path in &diverging {
        match tolerances.covering(path) {
            Some(ad) => {
                let backing = ad.resolved_id().unwrap_or("<unresolved>");
                covered.push(json!({ "observablePath": path, "backingId": backing }));
                consumed.insert(tolerance_key(ad));
            }
            None => uncovered.push(path.clone()),
        }
    }

    if uncovered.is_empty() {
        ChannelVerdict {
            channel: channel.to_string(),
            outcome: Outcome::AllowedDifference,
            detail: Some(json!({ "allowed": covered })),
        }
    } else {
        diverge(
            channel,
            json!({ "kind": "differential", "divergingPaths": uncovered }),
        )
    }
}

/// The scoped tolerances applicable to a case: its allowed differences plus the
/// behaviors it links (a tolerance applies only under a linked behavior, FR-033).
pub struct Tolerances<'a> {
    allowed: &'a [deacon_conformance::model::AllowedDifference],
    behaviors: &'a [String],
}

impl<'a> Tolerances<'a> {
    /// Build the tolerance set for a case.
    pub fn new(
        allowed: &'a [deacon_conformance::model::AllowedDifference],
        behaviors: &'a [String],
    ) -> Tolerances<'a> {
        Tolerances { allowed, behaviors }
    }

    /// Whether there are no tolerances (the common case).
    pub fn is_empty(&self) -> bool {
        self.allowed.is_empty()
    }

    /// The allowed difference covering `observable_path` — matched by a LINKED behavior
    /// and an `observablePath` that equals `observable_path` or is a segment-prefix of it
    /// (so `chan-image.labels` covers `chan-image.labels.foo`). `None` if uncovered.
    fn covering(
        &self,
        observable_path: &str,
    ) -> Option<&'a deacon_conformance::model::AllowedDifference> {
        self.allowed.iter().find(|ad| {
            self.behaviors.contains(&ad.behavior)
                && path_covers(&ad.observable_path, observable_path)
        })
    }

    /// The tolerance keys that were NOT consumed by any covered divergence this run — the
    /// STALE allowed differences (their characterized difference no longer reproduces,
    /// FR-034). The caller surfaces them in the report.
    pub fn stale(&self, consumed: &std::collections::HashSet<String>) -> Vec<String> {
        self.allowed
            .iter()
            .filter(|ad| self.behaviors.contains(&ad.behavior))
            .filter(|ad| !consumed.contains(&tolerance_key(ad)))
            .map(|ad| {
                format!(
                    "{} @ {} ({})",
                    ad.behavior,
                    ad.observable_path,
                    ad.resolved_id().unwrap_or("<unresolved>")
                )
            })
            .collect()
    }
}

/// The stable consumption key for a tolerance: `(behavior, observablePath)` (FR-033).
fn tolerance_key(ad: &deacon_conformance::model::AllowedDifference) -> String {
    format!("{}\u{0}{}", ad.behavior, ad.observable_path)
}

/// Whether an allowed `observablePath` covers a diverging `path`: exact match, or the
/// allowed path is a segment-boundary prefix of the diverging path.
fn path_covers(allowed: &str, path: &str) -> bool {
    allowed == path
        || (path.len() > allowed.len()
            && path.starts_with(allowed)
            && path.as_bytes().get(allowed.len()) == Some(&b'.'))
}

/// Collect the channel-prefixed dotted paths where `a` and `b` differ. Objects recurse
/// key-wise; arrays/scalars that differ emit their path. `prefix` starts as the channel id.
fn diff_paths(prefix: &str, a: &Value, b: &Value, out: &mut Vec<String>) {
    match (a, b) {
        (Value::Object(oa), Value::Object(ob)) => {
            let mut keys: Vec<&String> = oa.keys().chain(ob.keys()).collect();
            keys.sort();
            keys.dedup();
            for k in keys {
                let child = format!("{prefix}.{k}");
                match (oa.get(k), ob.get(k)) {
                    (Some(va), Some(vb)) => diff_paths(&child, va, vb, out),
                    // Present on one side only → a divergence at that key.
                    _ => out.push(child),
                }
            }
        }
        _ if a != b => out.push(prefix.to_string()),
        _ => {}
    }
}

/// An `agree` verdict with no detail.
fn agree(channel: &str) -> ChannelVerdict {
    ChannelVerdict {
        channel: channel.to_string(),
        outcome: Outcome::Agree,
        detail: None,
    }
}

/// A `diverge` verdict carrying a path-free, deterministic `detail`.
fn diverge(channel: &str, detail: Value) -> ChannelVerdict {
    ChannelVerdict {
        channel: channel.to_string(),
        outcome: Outcome::Diverge,
        detail: Some(detail),
    }
}

/// The outcome of evaluating one assertion.
enum AssertionResult {
    Pass,
    /// Failed with a path-free detail describing the mismatch.
    Fail(Value),
}

/// Dispatch an assertion to its channel-appropriate evaluator. Unknown channels /
/// assertion keys are fail-loud authoring errors.
fn evaluate_assertion(
    channel: &str,
    evidence: &NormalizedChannelEvidence,
    assertion: &Value,
) -> Result<AssertionResult, HarnessError> {
    let obj = assertion
        .as_object()
        .ok_or_else(|| malformed(channel, "assertion must be an object"))?;
    // Exactly one predicate key; take it fallibly so there is no unchecked panic path.
    let mut entries = obj.iter();
    let (key, expected) = match (entries.next(), entries.next()) {
        (Some(entry), None) => entry,
        _ => {
            return Err(malformed(
                channel,
                "assertion must carry exactly one predicate key",
            ));
        }
    };

    match channel {
        CHAN_EXIT_CODE => exit_code_assertion(channel, key, expected, evidence),
        CHAN_STDOUT | CHAN_STDERR => text_assertion(channel, key, expected, evidence),
        // JSON-object channels (structured output, file content, and the four Docker
        // channels) share the `jsonEquals`/`jsonSubset` evaluator.
        CHAN_STRUCTURED_OUTPUT
        | CHAN_FILE_CONTENT
        | CHAN_IMAGE
        | CHAN_PROCESS_GRAPH
        | CHAN_INJECTED_PROCESS
        | CHAN_TEMPORAL => structured_assertion(channel, key, expected, evidence),
        CHAN_FILESYSTEM => filesystem_assertion(channel, key, expected, evidence),
        other => Err(malformed(
            other,
            "no spec-expectation assertion evaluator for this channel",
        )),
    }
}

/// `chan-filesystem`: `{ exists: "<relpath>" } | { absent: "<relpath>" } | { mode: {"<relpath>": "0644"} }`.
/// Evaluated against the observer's `{ "<relpath>": { exists, mode } }` map (normalized,
/// so any path in a value is already tokenized).
fn filesystem_assertion(
    channel: &str,
    key: &str,
    expected: &Value,
    evidence: &NormalizedChannelEvidence,
) -> Result<AssertionResult, HarnessError> {
    let map = evidence
        .value
        .as_object()
        .ok_or_else(|| malformed(channel, "filesystem evidence is not an object"))?;
    match key {
        "exists" | "absent" => {
            let path = expected
                .as_str()
                .ok_or_else(|| malformed(channel, &format!("`{key}` must be a path string")))?;
            let exists = map
                .get(path)
                .and_then(|e| e.get("exists"))
                .and_then(Value::as_bool)
                .unwrap_or(false);
            let want_exists = key == "exists";
            Ok(pass_if(
                exists == want_exists,
                json!({ "predicate": key, "path": path, "actualExists": exists }),
            ))
        }
        "mode" => {
            let want = expected
                .as_object()
                .ok_or_else(|| malformed(channel, "`mode` must be a { path: mode } object"))?;
            for (path, mode) in want {
                let actual = map.get(path).and_then(|e| e.get("mode")).cloned();
                if actual.as_ref() != Some(mode) {
                    return Ok(AssertionResult::Fail(
                        json!({ "predicate": "mode", "path": path, "expectedMode": mode, "actualMode": actual }),
                    ));
                }
            }
            Ok(AssertionResult::Pass)
        }
        other => Err(malformed(
            channel,
            &format!("unknown filesystem assertion `{other}` (expected `exists`/`absent`/`mode`)"),
        )),
    }
}

/// `chan-exit-code`: `{ equals: <int> }` | `{ nonZero: true }`.
fn exit_code_assertion(
    channel: &str,
    key: &str,
    expected: &Value,
    evidence: &NormalizedChannelEvidence,
) -> Result<AssertionResult, HarnessError> {
    let actual = evidence.value.as_i64();
    match key {
        "equals" => {
            let want = expected
                .as_i64()
                .ok_or_else(|| malformed(channel, "`equals` must be an integer"))?;
            Ok(if actual == Some(want) {
                AssertionResult::Pass
            } else {
                AssertionResult::Fail(json!({ "expectedExitCode": want, "actualExitCode": actual }))
            })
        }
        "nonZero" => {
            let want = expected
                .as_bool()
                .ok_or_else(|| malformed(channel, "`nonZero` must be a boolean"))?;
            let is_non_zero = matches!(actual, Some(code) if code != 0);
            Ok(if is_non_zero == want {
                AssertionResult::Pass
            } else {
                AssertionResult::Fail(json!({ "expectedNonZero": want, "actualExitCode": actual }))
            })
        }
        other => Err(malformed(
            channel,
            &format!("unknown exit-code assertion `{other}` (expected `equals`/`nonZero`)"),
        )),
    }
}

/// `chan-stdout`/`chan-stderr`: `{ equals } | { contains } | { matches }` (regex).
fn text_assertion(
    channel: &str,
    key: &str,
    expected: &Value,
    evidence: &NormalizedChannelEvidence,
) -> Result<AssertionResult, HarnessError> {
    let actual = evidence
        .value
        .as_str()
        .ok_or_else(|| malformed(channel, "text-channel evidence is not a string"))?;
    let want = expected
        .as_str()
        .ok_or_else(|| malformed(channel, &format!("`{key}` must be a string")))?;
    match key {
        "equals" => Ok(pass_if(actual == want, json!({ "predicate": "equals" }))),
        "contains" => Ok(pass_if(
            actual.contains(want),
            json!({ "predicate": "contains", "missingSubstring": want }),
        )),
        "matches" => {
            let re = regex::Regex::new(want)
                .map_err(|e| malformed(channel, &format!("`matches` is not a valid regex: {e}")))?;
            Ok(pass_if(
                re.is_match(actual),
                json!({ "predicate": "matches", "pattern": want }),
            ))
        }
        other => Err(malformed(
            channel,
            &format!("unknown text assertion `{other}` (expected `equals`/`contains`/`matches`)"),
        )),
    }
}

/// `chan-structured-output`/`chan-file-content`: `{ jsonEquals } | { jsonSubset }`.
/// `present:false` (channel not captured) fails any structured assertion — the runner
/// declared it and it was not observable (FR-018).
fn structured_assertion(
    channel: &str,
    key: &str,
    expected: &Value,
    evidence: &NormalizedChannelEvidence,
) -> Result<AssertionResult, HarnessError> {
    if !evidence.present {
        return Ok(AssertionResult::Fail(
            json!({ "reason": "structured output not captured (stdout was not valid JSON)" }),
        ));
    }
    match key {
        "jsonEquals" => Ok(pass_if(
            &evidence.value == expected,
            json!({ "predicate": "jsonEquals" }),
        )),
        "jsonSubset" => {
            let mut missing = Vec::new();
            let ok = is_json_subset(expected, &evidence.value, "", &mut missing);
            Ok(pass_if(
                ok,
                json!({ "predicate": "jsonSubset", "firstMismatchPath": missing.first() }),
            ))
        }
        other => Err(malformed(
            channel,
            &format!("unknown structured assertion `{other}` (expected `jsonEquals`/`jsonSubset`)"),
        )),
    }
}

/// Pass/fail helper with a supplied fail-detail.
fn pass_if(ok: bool, fail_detail: Value) -> AssertionResult {
    if ok {
        AssertionResult::Pass
    } else {
        AssertionResult::Fail(fail_detail)
    }
}

/// Recursive JSON-subset check: every object key in `subset` must be present in `actual`
/// and recurse; an `subset` ARRAY is order-insensitive "contains" — each subset element
/// must be a subset of SOME actual element (so `mounts: [{target: "/w"}]` matches a mount
/// with more fields, in any position); scalars require exact equality. `jsonEquals` is the
/// assertion for exact equality. On the first mismatch, records the dotted JSON path (NOT
/// a filesystem path) into `missing` for a path-free, byte-stable detail.
fn is_json_subset(subset: &Value, actual: &Value, path: &str, missing: &mut Vec<String>) -> bool {
    match subset {
        Value::Object(sub_map) => {
            let Some(act_map) = actual.as_object() else {
                missing.push(path.to_string());
                return false;
            };
            for (k, v) in sub_map {
                let child = if path.is_empty() {
                    k.clone()
                } else {
                    format!("{path}.{k}")
                };
                match act_map.get(k) {
                    Some(av) => {
                        if !is_json_subset(v, av, &child, missing) {
                            return false;
                        }
                    }
                    None => {
                        missing.push(child);
                        return false;
                    }
                }
            }
            true
        }
        Value::Array(sub_items) => {
            let Some(act_items) = actual.as_array() else {
                missing.push(path.to_string());
                return false;
            };
            for (idx, sub) in sub_items.iter().enumerate() {
                // Some actual element must contain this subset element (order-insensitive).
                let found = act_items
                    .iter()
                    .any(|act| is_json_subset(sub, act, path, &mut Vec::new()));
                if !found {
                    missing.push(format!("{path}[{idx}]"));
                    return false;
                }
            }
            true
        }
        other => {
            if other == actual {
                true
            } else {
                missing.push(path.to_string());
                false
            }
        }
    }
}

/// A malformed-assertion / unsupported-channel authoring error (fail-loud).
fn malformed(channel: &str, cause: &str) -> HarnessError {
    HarnessError::NormalizationFailed {
        channel: channel.to_string(),
        cause: format!("malformed assertion: {cause}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn norm(channel: &str, value: Value) -> NormalizedChannelEvidence {
        NormalizedChannelEvidence {
            channel: channel.to_string(),
            operation: "op".to_string(),
            present: true,
            value,
        }
    }

    #[test]
    fn exit_code_equals_agree_and_diverge() {
        let ev = norm(CHAN_EXIT_CODE, json!(0));
        assert_eq!(
            verdict_spec_expectation(CHAN_EXIT_CODE, &ev, &json!({"equals":0}))
                .unwrap()
                .outcome,
            Outcome::Agree
        );
        assert_eq!(
            verdict_spec_expectation(CHAN_EXIT_CODE, &ev, &json!({"equals":1}))
                .unwrap()
                .outcome,
            Outcome::Diverge
        );
    }

    #[test]
    fn exit_code_non_zero() {
        let fail = norm(CHAN_EXIT_CODE, json!(3));
        assert_eq!(
            verdict_spec_expectation(CHAN_EXIT_CODE, &fail, &json!({"nonZero":true}))
                .unwrap()
                .outcome,
            Outcome::Agree
        );
    }

    #[test]
    fn text_equals_contains_matches() {
        let ev = norm(CHAN_STDOUT, json!("hello world"));
        assert_eq!(
            verdict_spec_expectation(CHAN_STDOUT, &ev, &json!({"contains":"world"}))
                .unwrap()
                .outcome,
            Outcome::Agree
        );
        assert_eq!(
            verdict_spec_expectation(CHAN_STDOUT, &ev, &json!({"matches":"^hello"}))
                .unwrap()
                .outcome,
            Outcome::Agree
        );
        assert_eq!(
            verdict_spec_expectation(CHAN_STDOUT, &ev, &json!({"equals":"nope"}))
                .unwrap()
                .outcome,
            Outcome::Diverge
        );
    }

    #[test]
    fn json_subset_nested() {
        let ev = norm(
            CHAN_STRUCTURED_OUTPUT,
            json!({ "configuration": { "customUnknownKey": "preserved", "name": "x" } }),
        );
        let ok = verdict_spec_expectation(
            CHAN_STRUCTURED_OUTPUT,
            &ev,
            &json!({ "jsonSubset": { "configuration": { "customUnknownKey": "preserved" } } }),
        )
        .unwrap();
        assert_eq!(ok.outcome, Outcome::Agree);

        let bad = verdict_spec_expectation(
            CHAN_STRUCTURED_OUTPUT,
            &ev,
            &json!({ "jsonSubset": { "configuration": { "customUnknownKey": "WRONG" } } }),
        )
        .unwrap();
        assert_eq!(bad.outcome, Outcome::Diverge);
    }

    #[test]
    fn structured_absent_diverges() {
        let mut ev = norm(CHAN_STRUCTURED_OUTPUT, Value::Null);
        ev.present = false;
        let v = verdict_spec_expectation(
            CHAN_STRUCTURED_OUTPUT,
            &ev,
            &json!({ "jsonSubset": { "a": 1 } }),
        )
        .unwrap();
        assert_eq!(v.outcome, Outcome::Diverge);
    }

    #[test]
    fn malformed_assertion_is_fail_loud() {
        let ev = norm(CHAN_EXIT_CODE, json!(0));
        assert!(verdict_spec_expectation(CHAN_EXIT_CODE, &ev, &json!({"weird": 1})).is_err());
        assert!(verdict_spec_expectation(CHAN_EXIT_CODE, &ev, &json!("not-an-object")).is_err());
    }

    use deacon_conformance::model::AllowedDifference;
    use std::collections::HashSet;

    fn no_tolerances() -> Tolerances<'static> {
        Tolerances::new(&[], &[])
    }

    #[test]
    fn differential_equal_agree_unequal_diverge() {
        let a = norm(CHAN_EXIT_CODE, json!(0));
        let b = norm(CHAN_EXIT_CODE, json!(0));
        let mut consumed = HashSet::new();
        assert_eq!(
            verdict_differential(CHAN_EXIT_CODE, &a, &b, &no_tolerances(), &mut consumed).outcome,
            Outcome::Agree
        );
        let c = norm(CHAN_EXIT_CODE, json!(1));
        assert_eq!(
            verdict_differential(CHAN_EXIT_CODE, &a, &c, &no_tolerances(), &mut consumed).outcome,
            Outcome::Diverge
        );
    }

    fn tz_difference() -> AllowedDifference {
        AllowedDifference {
            behavior: "bhv-x".to_string(),
            context: vec![],
            observable_path: "chan-injected-process.env.TZ".to_string(),
            rationale: "reference leaks host TZ; deacon does not".to_string(),
            waiver_id: Some("wvr-tz".to_string()),
            divergence_id: None,
        }
    }

    #[test]
    fn covered_divergence_is_allowed_difference_uncovered_stays_diverge() {
        let allowed = vec![tz_difference()];
        let behaviors = vec!["bhv-x".to_string()];
        let tol = Tolerances::new(&allowed, &behaviors);

        // A divergence ONLY at the covered path (env.TZ) → allowed-difference, with the
        // backing waiver id in the detail.
        let deacon = norm(
            CHAN_INJECTED_PROCESS,
            json!({ "env": { "TZ": "UTC" }, "user": "vscode" }),
        );
        let reference = norm(
            CHAN_INJECTED_PROCESS,
            json!({ "env": { "TZ": "America/NY" }, "user": "vscode" }),
        );
        let mut consumed = HashSet::new();
        let v = verdict_differential(
            CHAN_INJECTED_PROCESS,
            &deacon,
            &reference,
            &tol,
            &mut consumed,
        );
        assert_eq!(
            v.outcome,
            Outcome::AllowedDifference,
            "TZ path is tolerated: {v:?}"
        );
        assert!(
            v.detail.as_ref().unwrap().to_string().contains("wvr-tz"),
            "the backing waiver id is in the detail: {v:?}"
        );
        assert!(!consumed.is_empty(), "the tolerance was consumed");

        // The SAME difference on a DIFFERENT path (env.LANG) is NOT covered → diverge.
        let deacon2 = norm(CHAN_INJECTED_PROCESS, json!({ "env": { "LANG": "C" } }));
        let reference2 = norm(CHAN_INJECTED_PROCESS, json!({ "env": { "LANG": "en_US" } }));
        let mut consumed2 = HashSet::new();
        let v2 = verdict_differential(
            CHAN_INJECTED_PROCESS,
            &deacon2,
            &reference2,
            &tol,
            &mut consumed2,
        );
        assert_eq!(
            v2.outcome,
            Outcome::Diverge,
            "the same difference on path B still fails (FR-033): {v2:?}"
        );
    }

    #[test]
    fn a_mix_of_covered_and_uncovered_paths_stays_diverge() {
        let allowed = vec![tz_difference()];
        let behaviors = vec!["bhv-x".to_string()];
        let tol = Tolerances::new(&allowed, &behaviors);
        // Divergence at env.TZ (covered) AND user (uncovered) → the whole channel diverges.
        let deacon = norm(
            CHAN_INJECTED_PROCESS,
            json!({ "env": { "TZ": "UTC" }, "user": "root" }),
        );
        let reference = norm(
            CHAN_INJECTED_PROCESS,
            json!({ "env": { "TZ": "America/NY" }, "user": "vscode" }),
        );
        let mut consumed = HashSet::new();
        let v = verdict_differential(
            CHAN_INJECTED_PROCESS,
            &deacon,
            &reference,
            &tol,
            &mut consumed,
        );
        assert_eq!(
            v.outcome,
            Outcome::Diverge,
            "an uncovered path makes the whole channel diverge: {v:?}"
        );
    }

    #[test]
    fn unconsumed_tolerance_is_stale() {
        // The tolerance is declared but its difference does not reproduce (deacon ==
        // reference) → it is unconsumed → stale (self-invalidating, FR-034).
        let allowed = vec![tz_difference()];
        let behaviors = vec!["bhv-x".to_string()];
        let tol = Tolerances::new(&allowed, &behaviors);
        let same = norm(CHAN_INJECTED_PROCESS, json!({ "env": { "TZ": "UTC" } }));
        let mut consumed = HashSet::new();
        let v = verdict_differential(CHAN_INJECTED_PROCESS, &same, &same, &tol, &mut consumed);
        assert_eq!(v.outcome, Outcome::Agree, "no divergence when values match");
        let stale = tol.stale(&consumed);
        assert_eq!(
            stale.len(),
            1,
            "the unused tolerance is reported stale: {stale:?}"
        );
        assert!(stale[0].contains("wvr-tz"));
    }
}
