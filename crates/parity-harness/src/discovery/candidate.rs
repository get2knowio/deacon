//! Reviewable-candidate assembly under `target/discovery/candidates/<fnd-id>/`
//! (025-exploratory-parity-discovery, T049 – T051, data-model.md § 9,
//! FR-024 – FR-027).
//!
//! Six parts, all required for the candidate to be self-contained (FR-024/FR-027):
//!
//! | Part | File | What it answers |
//! |---|---|---|
//! | minimal fixture | `fixture/` | *what input?* — a materializable workspace tree |
//! | operations + argv | `context.json` | *run how?* — plus the campaign, seed, and pinned input set (FR-026) |
//! | raw evidence | `raw.json` | *what did each side actually emit?* |
//! | normalized difference | `normalized.json` | *what did the comparison conclude?* |
//! | reference provenance | `provenance.json` | *compared against what, and how was the input produced?* |
//! | suggested behavior mapping | `mapping.json` | *does the project already have a name for this?* |
//!
//! ## Raw and normalized are separate files (T050, FR-014)
//!
//! Mirroring the committed-snapshot layout (the FR-016 precedent from 022). Conflating them
//! would make it impossible to tell a difference the *implementations* produced from one the
//! *normalizer* produced — which is precisely the `normalizer-defect` classification the
//! triage vocabulary reserves, and a reviewer who cannot separate the two cannot reach it.
//!
//! ## `mapping.json` never invents an identity (T051, FR-025)
//!
//! It carries either a `bhv-` id **that resolves in the loaded registry** or an explicit
//! `{"match": "none"}`. The guard is structural rather than careful: whatever the suggestion
//! rule proposes is filtered against `registry.behaviors` before it is written, and a
//! proposal that does not survive the filter becomes a no-match. A suggestion that fabricated
//! a plausible-looking id would turn the reviewer's job into *verifying* a mapping rather
//! than *deciding* one, which is worse than offering none.
//!
//! The rule itself is deliberately narrow: a behavior is suggested only when a committed case
//! already declares an assertion **on the same channel, at the same observable path**. That is
//! a real, reviewed statement that the project reasons about that field; anything looser —
//! matching by area, by subcommand, by word overlap — would be the report asserting a
//! relationship nobody established.

use std::path::{Path, PathBuf};

use deacon_conformance::discovery::queue::PinnedInputSet;
use deacon_conformance::discovery::shrink::Reduction;
use deacon_conformance::discovery::signature::Signature;
use deacon_conformance::load::Registry;
use serde_json::{Map, Value, json};

use crate::HarnessError;
use crate::normalize::NORMALIZER_VERSION;
use crate::oracle::{OracleSource, VerifiedOracle};

use super::campaign::write_workspace_tree;
use super::differential::{DifferentialResult, Observation, SideEvidence};

/// The six file names, in the order data-model.md § 9 lists them.
///
/// Named once so the writer and the completeness check cannot disagree about what "all six
/// parts" means — SC-005 is a claim about this list, and a second copy of it would be a
/// second definition of the claim.
pub const CANDIDATE_PARTS: [&str; 6] = [
    "fixture",
    "context.json",
    "raw.json",
    "normalized.json",
    "provenance.json",
    "mapping.json",
];

/// What the deacon side of a comparison was measured against — the honest answer to
/// `provenance.json`'s "compared against what?".
///
/// A two-variant enum rather than an unconditional [`VerifiedOracle`] because there are
/// genuinely two answers and conflating them would be a lie in the one file whose job is to
/// say what the evidence is. Every *campaign* tier compares against the verified pinned
/// oracle; the FR-042a pipeline proof compares deacon against **its own unperturbed run**,
/// with a known difference injected into one side at the sealed evidence-source boundary.
///
/// The proof's counterpart is not a reference implementation and must never be recorded as
/// one: a `provenance.json` that claimed a verified oracle for a run that never invoked one
/// would make the candidate's central claim — *this is what the two implementations did* —
/// false, and a reviewer has no way to detect that from inside the file.
#[derive(Debug, Clone, Copy)]
pub enum ReferenceProvenance<'a> {
    /// The verified pinned oracle. Taking the verified type rather than a path is what
    /// makes "never compare against an unverified reference" (FR-003) a fact about the
    /// value rather than a rule the caller has to remember.
    Oracle(&'a VerifiedOracle),
    /// deacon's own unperturbed run, with a difference injected into the other side at the
    /// sealed evidence-source boundary (FR-042a, research D7).
    ///
    /// Carries the injection's record id so the candidate names *what was planted*: this
    /// candidate documents the machinery working, not a divergence between two
    /// implementations, and it says so in its own provenance.
    InjectedSelfComparison {
        /// The `reg-`-shaped perturbation record that was applied.
        injection: &'a str,
    },
}

/// Everything one reviewable candidate is assembled from.
pub struct CandidateInputs<'a> {
    /// The finding this candidate belongs to (`fnd-…`) — also its directory name.
    pub finding_id: &'a str,
    /// The signature under review.
    pub signature: &'a Signature,
    /// The observation that produced it — the concrete values behind the signature.
    pub observation: &'a Observation,
    /// The campaign record: seed, lane, tier, profile, and the pinned input set (FR-026).
    pub campaign_id: &'a str,
    pub seed_hex: &'a str,
    pub lane: &'a str,
    pub tier: &'a str,
    pub profile: &'a str,
    pub pinned_input_set: &'a PinnedInputSet,
    /// The generated candidate that surfaced it.
    pub candidate_id: &'a str,
    /// The operations, `${WORKSPACE}`-tokenized.
    pub operations: &'a [deacon_conformance::discovery::generate::Operation],
    /// The `mop-` operators applied to produce the candidate (FR-009 attribution).
    pub mutation_operators: &'a [String],
    /// The reduction: the minimal fixture and how it was reached.
    pub reduction: &'a Reduction,
    /// The comparison the observation came from — the raw and normalized evidence.
    pub result: &'a DifferentialResult,
    /// What the deacon side was compared **against** (FR-026's provenance).
    pub reference: ReferenceProvenance<'a>,
    /// The loaded registry, for resolving the suggested mapping. **Read only** — FR-018
    /// forbids discovery writing anything the registry owns.
    pub registry: &'a Registry,
    /// The candidates root, normally `target/discovery/candidates`.
    pub root: &'a Path,
}

/// Write one reviewable candidate, returning its directory.
///
/// The directory is **replaced** rather than merged into: a stale file from an earlier
/// campaign sitting beside a fresh one would be evidence about a comparison nobody ran, and
/// the reviewer has no way to tell which is which.
pub fn write(inputs: CandidateInputs<'_>) -> Result<PathBuf, HarnessError> {
    let dir = inputs.root.join(inputs.finding_id);
    if dir.exists() {
        std::fs::remove_dir_all(&dir).map_err(|e| io_error(&dir, e))?;
    }
    std::fs::create_dir_all(&dir).map_err(|e| io_error(&dir, e))?;

    // 1. The minimal fixture, as a materializable workspace tree — the SAME shape the
    //    campaign compared, so FR-027's "reproducing requires only the candidate" holds.
    let fixture = dir.join("fixture");
    std::fs::create_dir_all(&fixture).map_err(|e| io_error(&fixture, e))?;
    write_workspace_tree(&fixture, &inputs.reduction.document)
        .map_err(|e| io_error(&fixture, e))?;

    // 2. Operations + argv, the campaign and seed, and the pinned input set (FR-026).
    write_json(&dir.join("context.json"), &context(&inputs))?;

    // 3./4. Raw and normalized, SEPARATELY (T050, FR-014).
    write_json(&dir.join("raw.json"), &raw(&inputs))?;
    write_json(&dir.join("normalized.json"), &normalized(&inputs))?;

    // 5. Reference provenance + how the input was produced and reduced.
    write_json(&dir.join("provenance.json"), &provenance(&inputs))?;

    // 6. The suggested behavior mapping, or an explicit no-match (T051, FR-025).
    write_json(&dir.join("mapping.json"), &mapping(&inputs))?;

    Ok(dir)
}

/// The candidate directory for a finding under `root`.
pub fn candidate_dir(root: &Path, finding_id: &str) -> PathBuf {
    root.join(finding_id)
}

// ---------------------------------------------------------------------------
// The six parts
// ---------------------------------------------------------------------------

fn context(inputs: &CandidateInputs<'_>) -> Value {
    json!({
        "findingId": inputs.finding_id,
        "signature": inputs.signature,
        "campaignId": inputs.campaign_id,
        "seed": inputs.seed_hex,
        "lane": inputs.lane,
        "tier": inputs.tier,
        "profile": inputs.profile,
        "candidateId": inputs.candidate_id,
        // `${WORKSPACE}` is the same token the declarative conformance runner uses, so a
        // candidate's operations read exactly the way a case's do — which is the shape a
        // reviewer will need if they promote this finding.
        "operations": inputs.operations,
        "fixture": "fixture",
        "pinnedInputSet": pinned_input_set(inputs.pinned_input_set),
        "reproduce": format!(
            "materialize `fixture/` into a workspace W, then run each operation with \
             ${{WORKSPACE}} = W against deacon and against @devcontainers/cli@{}",
            inputs.pinned_input_set.oracle_version
        ),
    })
}

/// Both sides' evidence, **unnormalized** — the bytes each CLI actually produced.
fn raw(inputs: &CandidateInputs<'_>) -> Value {
    json!({
        "deacon": raw_side(&inputs.result.deacon),
        "reference": raw_side(&inputs.result.reference),
    })
}

fn raw_side(side: &SideEvidence) -> Value {
    json!({
        "outcome": side.outcome.as_str(),
        // Preserved but never compared (FR-016): two implementations spell "I refused
        // this" with different non-zero codes, and comparing the numbers would report the
        // wording of a status rather than its meaning.
        "exitCode": side.exit_code,
        "stdout": read_capture(&side.stdout_path),
        "stderr": read_capture(&side.stderr_path),
        "stdoutPath": side.stdout_path.to_string_lossy(),
        "stderrPath": side.stderr_path.to_string_lossy(),
    })
}

/// Read a capture back, or record why it could not be read.
///
/// A missing capture is recorded as an explicit note rather than as an empty string: an
/// empty stdout and an unreadable one are different facts, and the reviewer reading
/// `raw.json` is precisely the person who needs to tell them apart.
fn read_capture(path: &Path) -> Value {
    match std::fs::read(path) {
        Ok(bytes) => Value::String(String::from_utf8_lossy(&bytes).into_owned()),
        Err(e) => json!({ "unavailable": format!("{}: {e}", path.display()) }),
    }
}

/// Both sides' normalized evidence and the difference the comparison concluded.
fn normalized(inputs: &CandidateInputs<'_>) -> Value {
    json!({
        "normalizerVersion": NORMALIZER_VERSION,
        "deacon": inputs.result.deacon.normalized,
        "reference": inputs.result.reference.normalized,
        "difference": {
            "signature": inputs.signature,
            "channel": inputs.signature.channel,
            "path": inputs.signature.path,
            "deacon": inputs.observation.observed.deacon,
            "reference": inputs.observation.observed.reference,
        },
        // Every difference the same comparison saw, so a reviewer can tell a lone
        // divergence from one member of a cluster without re-running anything.
        "allObservations": inputs
            .result
            .observations
            .iter()
            .map(|o| json!({
                "signature": o.signature,
                "deacon": o.observed.deacon,
                "reference": o.observed.reference,
                "new": o.is_new(),
            }))
            .collect::<Vec<Value>>(),
        "parseStageFailure": inputs.result.parse_stage_failure,
    })
}

fn provenance(inputs: &CandidateInputs<'_>) -> Value {
    json!({
        "reference": reference_provenance(inputs),
        "mutationOperators": inputs.mutation_operators,
        "reduction": {
            "steps": inputs.reduction.steps,
            "isMinimal": inputs.reduction.is_minimal,
            "notMinimalReason": inputs.reduction.not_minimal_reason,
            "probes": inputs.reduction.probes,
            "originalSize": inputs.reduction.original_size,
            "reducedSize": inputs.reduction.reduced_size,
            "sizeReductionFraction": inputs.reduction.size_reduction_fraction(),
            "remainingMutations": inputs.reduction.remaining_mutations,
            "driftedSignatures": inputs
                .reduction
                .drifted
                .iter()
                .map(|d| d.signature.id.clone())
                .collect::<Vec<String>>(),
            "catalogue": deacon_conformance::discovery::shrink::reduction_catalogue_identity(),
        },
        "pinnedInputSet": pinned_input_set(inputs.pinned_input_set),
    })
}

/// What the deacon side was compared against, said plainly.
///
/// `kind` is the first key on purpose: a reviewer reading `provenance.json` must be able to
/// tell "this documents a divergence from the reference" from "this documents the pipeline
/// proving itself" without inferring it from which other keys happen to be present.
fn reference_provenance(inputs: &CandidateInputs<'_>) -> Value {
    match inputs.reference {
        ReferenceProvenance::Oracle(oracle) => json!({
            "kind": "verified-oracle",
            "version": oracle.version,
            "path": oracle.path.to_string_lossy(),
            "source": match oracle.source {
                OracleSource::Override => "override",
                OracleSource::PathLookup => "path-lookup",
            },
            // The oracle is a `VerifiedOracle`, and only the verification path hands one
            // out — so this is a statement about how the value was obtained, not a hopeful
            // assertion (FR-003).
            "verified": true,
            "verification": format!(
                "reported version equals the pinned {}",
                inputs.pinned_input_set.oracle_version
            ),
        }),
        ReferenceProvenance::InjectedSelfComparison { injection } => json!({
            "kind": "injected-self-comparison",
            "injection": injection,
            "verified": false,
            "verification": "NOT a reference comparison: deacon was compared against its \
                             own unperturbed run, with the named difference injected into \
                             one side at the sealed evidence-source boundary. This \
                             candidate is evidence that the PIPELINE works (FR-042a), not \
                             evidence that the two implementations disagree.",
        }),
    }
}

fn pinned_input_set(pins: &PinnedInputSet) -> Value {
    json!({
        "schemaPin": pins.schema_pin,
        "prosePin": pins.prose_pin,
        "oracleVersion": pins.oracle_version,
        "normalizerVersion": pins.normalizer_version,
        "grammarVersion": pins.grammar_version,
        "mutationCatalogVersion": pins.mutation_catalog_version,
        "generatorVersion": pins.generator_version,
    })
}

// ---------------------------------------------------------------------------
// T051 — the suggested behavior mapping
// ---------------------------------------------------------------------------

/// The suggested mapping: a resolvable `bhv-` id, or an explicit no-match (FR-025).
fn mapping(inputs: &CandidateInputs<'_>) -> Value {
    let suggestions = suggest(inputs.registry, inputs.signature);

    match suggestions.as_slice() {
        [only] => json!({
            "match": "behavior",
            "behavior": only.behavior,
            "basis": format!(
                "case `{}` already declares an assertion on `{}` at `{}`",
                only.case, inputs.signature.channel, only.path
            ),
            "confidence": "suggestion",
            "note": "A suggestion, not a decision. The reviewer assigns the behavior; this \
                     names the one the registry already reasons about at this exact \
                     observable path.",
        }),
        // Zero is the ordinary case for a genuinely new difference. Several is ambiguity,
        // and picking one of them would be a coin flip wearing a suggestion's clothes — so
        // the candidates are LISTED (every one resolvable) and the decision is left where
        // it belongs.
        _ => json!({
            "match": "none",
            "reason": if suggestions.is_empty() {
                format!(
                    "no committed case declares an assertion on `{}` at `{}`, so the \
                     registry has no existing identity for this difference",
                    inputs.signature.channel, inputs.signature.path
                )
            } else {
                format!(
                    "{} committed behaviors are declared at `{}` on `{}`; choosing one \
                     would be a guess presented as a suggestion",
                    suggestions.len(),
                    inputs.signature.path,
                    inputs.signature.channel
                )
            },
            "considered": suggestions
                .iter()
                .map(|s| s.behavior.clone())
                .collect::<Vec<String>>(),
        }),
    }
}

/// One suggestion and the reviewed record it rests on.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Suggestion {
    behavior: String,
    case: String,
    path: String,
}

/// Behaviors the registry already declares at this signature's exact observable location.
///
/// Every returned id is filtered against `registry.behaviors`, so an id that does not
/// resolve cannot escape this function — FR-025 held structurally rather than by care.
fn suggest(registry: &Registry, signature: &Signature) -> Vec<Suggestion> {
    let mut out: Vec<Suggestion> = Vec::new();
    for case in &registry.cases {
        for expected in &case.expected {
            if expected.channel != signature.channel {
                continue;
            }
            let Some(assertion) = &expected.assertion else {
                continue;
            };
            for path in assertion_paths(assertion) {
                if path != signature.path {
                    continue;
                }
                for behavior in &case.behaviors {
                    // The structural guard: only an id that RESOLVES may be suggested.
                    if !registry.behaviors.iter().any(|b| &b.id == behavior) {
                        continue;
                    }
                    let suggestion = Suggestion {
                        behavior: behavior.clone(),
                        case: case.id.clone(),
                        path: path.clone(),
                    };
                    if !out.iter().any(|s| s.behavior == suggestion.behavior) {
                        out.push(suggestion);
                    }
                }
            }
        }
    }
    out.sort_by(|a, b| a.behavior.cmp(&b.behavior));
    out
}

/// The dotted observable paths a declarative assertion names.
///
/// Only the document-shaped assertion kinds contribute: `jsonSubset` and `jsonEquals` carry
/// a path space that is the same one a signature's `path` lives in. `equals` / `contains` /
/// `matches` / `nonZero` do not — they assert about a whole stream or a whole status — so
/// they name no path and contribute nothing rather than contributing the empty path, which
/// would match every signature on their channel.
fn assertion_paths(assertion: &Value) -> Vec<String> {
    let Some(object) = assertion.as_object() else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for kind in ["jsonSubset", "jsonEquals"] {
        if let Some(payload) = object.get(kind) {
            collect_paths(payload, &mut String::new(), &mut out);
        }
    }
    out
}

fn collect_paths(value: &Value, prefix: &mut String, out: &mut Vec<String>) {
    let Value::Object(map) = value else {
        return;
    };
    for (key, child) in map {
        let restore = prefix.len();
        if !prefix.is_empty() {
            prefix.push('.');
        }
        prefix.push_str(key);
        out.push(prefix.clone());
        collect_paths(child, prefix, out);
        prefix.truncate(restore);
    }
}

// ---------------------------------------------------------------------------
// Writing
// ---------------------------------------------------------------------------

/// Write one part atomically: a temp file beside the target, then a rename.
///
/// A candidate directory is read by a human while campaigns may still be running, and a
/// plain write truncates-then-streams — a reader arriving mid-write sees a truncated
/// document and concludes the evidence is malformed.
fn write_json(path: &Path, value: &Value) -> Result<(), HarnessError> {
    let mut rendered = serde_json::to_string_pretty(value).map_err(|e| HarnessError::Report {
        cause: format!("could not render {}: {e}", path.display()),
    })?;
    rendered.push('\n');
    let temp = path.with_extension("json.tmp");
    std::fs::write(&temp, rendered).map_err(|e| io_error(&temp, e))?;
    std::fs::rename(&temp, path).map_err(|e| io_error(path, e))
}

fn io_error(path: &Path, error: std::io::Error) -> HarnessError {
    HarnessError::Report {
        cause: format!(
            "could not write the reviewable candidate at {}: {error}",
            path.display()
        ),
    }
}

/// Whether every one of the six parts is present under `dir` (SC-005's claim).
///
/// Exposed so a test asserts the claim against the same list the writer honors, rather than
/// against a second list that could drift from it.
pub fn missing_parts(dir: &Path) -> Vec<&'static str> {
    CANDIDATE_PARTS
        .into_iter()
        .filter(|part| !dir.join(part).exists())
        .collect()
}

/// Every JSON part, parsed — so a caller can assert on content without re-deriving the
/// file list. A part that is absent or unparseable is simply missing from the map, which
/// [`missing_parts`] reports separately.
pub fn read_parts(dir: &Path) -> Map<String, Value> {
    let mut out = Map::new();
    for part in CANDIDATE_PARTS {
        if !part.ends_with(".json") {
            continue;
        }
        if let Ok(raw) = std::fs::read_to_string(dir.join(part))
            && let Ok(value) = serde_json::from_str::<Value>(&raw)
        {
            out.insert(part.to_string(), value);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use deacon_conformance::model::{CHAN_EXIT_CODE, CHAN_STRUCTURED_OUTPUT};

    fn signature(channel: &str, path: &str) -> Signature {
        use deacon_conformance::discovery::signature::{Divergence, DivergenceKind};
        Signature::derive(
            channel,
            &Divergence {
                kind: DivergenceKind::Value,
                path,
                deacon: None,
                reference: None,
            },
        )
    }

    #[test]
    fn the_six_parts_are_declared_once() {
        assert_eq!(
            CANDIDATE_PARTS,
            [
                "fixture",
                "context.json",
                "raw.json",
                "normalized.json",
                "provenance.json",
                "mapping.json",
            ],
            "SC-005 is a claim about THIS list; a second copy of it would be a second \
             definition of the claim"
        );
    }

    #[test]
    fn assertion_paths_are_read_only_from_document_shaped_assertions() {
        let subset = json!({ "jsonSubset": { "configuration": { "name": "x" } } });
        assert_eq!(
            assertion_paths(&subset),
            vec!["configuration", "configuration.name"],
            "a nested subset names every path it constrains, parent included"
        );

        // A whole-stream or whole-status assertion names NO path. Contributing the empty
        // path instead would match every signature on the channel and turn the suggestion
        // into "the first case that ever asserted on this channel".
        for whole in [
            json!({ "equals": 0 }),
            json!({ "nonZero": true }),
            json!({ "contains": "something" }),
            json!({ "matches": "^.*$" }),
        ] {
            assert!(assertion_paths(&whole).is_empty(), "{whole} named a path");
        }
        assert!(assertion_paths(&json!("not an object")).is_empty());
    }

    #[test]
    fn a_suggestion_names_only_a_behavior_the_committed_registry_resolves() {
        // The FR-025 guard, against the REAL registry: whatever the rule proposes, every id
        // it emits must resolve. An invented id would make the reviewer's job verifying a
        // plausible-looking identity rather than deciding one.
        let registry = Registry::load(&crate::conformance_registry_root())
            .expect("the committed registry loads");
        assert!(
            !registry.behaviors.is_empty(),
            "an empty behavior set would make every assertion below vacuous"
        );

        // A path the registry demonstrably reasons about (`case-readconfig-*` assert on it).
        let known = signature(CHAN_STRUCTURED_OUTPUT, "configuration.name");
        let suggestions = suggest(&registry, &known);
        assert!(
            !suggestions.is_empty(),
            "the committed registry declares assertions at `configuration.name`; finding \
             none means the reader stopped seeing them and every candidate would report \
             `match: none` regardless of what the registry knows"
        );
        for suggestion in &suggestions {
            assert!(
                registry
                    .behaviors
                    .iter()
                    .any(|b| b.id == suggestion.behavior),
                "`{}` does not resolve in the registry",
                suggestion.behavior
            );
            assert!(registry.cases.iter().any(|c| c.id == suggestion.case));
        }

        // A path nothing declares yields nothing — and the mapping then says so explicitly
        // rather than reaching for the nearest plausible id.
        let novel = signature(
            CHAN_STRUCTURED_OUTPUT,
            "configuration.thisFieldDoesNotExist",
        );
        assert!(suggest(&registry, &novel).is_empty());

        // Scoped to its channel: the same path on a different channel is a different
        // observable location.
        assert!(
            suggest(&registry, &signature(CHAN_EXIT_CODE, "configuration.name")).is_empty(),
            "a suggestion must be scoped to the channel the assertion was declared on"
        );
    }

    #[test]
    fn suggestions_are_deduplicated_and_ordered() {
        let registry = Registry::load(&crate::conformance_registry_root()).expect("loads");
        let suggestions = suggest(
            &registry,
            &signature(CHAN_STRUCTURED_OUTPUT, "configuration"),
        );
        let ids: Vec<String> = suggestions.iter().map(|s| s.behavior.clone()).collect();
        let mut sorted = ids.clone();
        sorted.sort();
        assert_eq!(ids, sorted, "suggestions are emitted in a stable order");
        let mut deduped = sorted.clone();
        deduped.dedup();
        assert_eq!(deduped, sorted, "a behavior is suggested at most once");
    }
}
