//! Injected-regression records (`reg-`) — `conformance/registry/regressions.json`
//! (024-deterministic-conformance-coverage, data-model.md §7,
//! contracts/regression-harness.md).
//!
//! A regression record is a **declarative, reversible perturbation of one evidence
//! source**, applied so that the suite is forced to notice. It is the mechanism that
//! gives every other green result its meaning: a channel nobody can make fail is a
//! channel that proves nothing, and a green suite whose channels are inert is worse than
//! no suite because it is trusted.
//!
//! **Ownership**: hand-authored. No generator writes this file.
//!
//! ## What a record may target
//!
//! [`EvidenceTarget`] names the RAW captured artifact — a completed process result, a
//! `docker inspect` document, the structured result document, or file bytes. It never
//! names an observer's return value: perturbing what an observer *returns* would let a
//! DEAD observer (one that ignores its input and always returns the same thing) appear
//! live, because the perturbed return value differs even though nothing was observed
//! (research Decision 5, FR-065b). The harness enforces that structurally as well — see
//! `parity_harness::inject` — but the vocabulary is closed here so a record cannot even
//! *ask* for the forbidden injection point.
//!
//! ## Why the perturbation set is closed
//!
//! [`PerturbationKind`] is five kinds and no more. Each is declarative (data, not code),
//! reversible (the harness reverts it on success AND on unwind, FR-066), and applies to a
//! named source. An open set would drift toward "run this snippet", which is neither
//! reviewable nor revertible.
//!
//! Cross-field coherence (`set-exit-code` carries an `exitCode` and nothing else, a
//! JSON-pointer kind carries a `pointer`, …) is enforced at **deserialize** time via
//! [`RawPerturbation`], mirroring `residual.rs`: a record that could never be applied must
//! not load at all, rather than fail later in a place that reads like an environment
//! fault.

use std::path::Path;

use serde::{Deserialize, Deserializer, Serialize};

use crate::load::{LoadError, SchemaError, deserialize_located, read_file};

/// The evidence SOURCE a perturbation is applied to (contract regression-harness.md,
/// "The injection point").
///
/// Every variant is a RAW captured artifact that exists **before** any observer runs.
/// There is deliberately no variant naming an observer's output (FR-065b).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum EvidenceTarget {
    /// The completed process result of an operation — its exit status.
    ProcessResult,
    /// The verbatim stdout bytes an operation produced.
    ProcessStdout,
    /// The verbatim stderr bytes an operation produced.
    ProcessStderr,
    /// The parsed structured result document, perturbed at its source: the stdout bytes
    /// are re-parsed as JSON, the pointer operation applied, and the document written
    /// back. The structured observer then parses the perturbed bytes exactly as it would
    /// parse real output.
    StructuredOutputDocument,
    /// The `docker inspect` document of the container a case brought up — the single
    /// pre-fetched inspect every Docker channel observer reads.
    ContainerInspectDocument,
    /// The image configuration slice of that same inspect document. A distinct name
    /// because it is a distinct CLAIM about what was perturbed (the image the container
    /// was created from, not the container's runtime state), even though deacon's image
    /// channel derives it from `.Config` of the container inspect.
    ImageInspectDocument,
    /// Bytes (or the presence) of a file in the case's workspace.
    WorkspaceFile,
}

impl EvidenceTarget {
    /// The stable wire spelling — used in diagnostics and the run report.
    pub fn as_str(self) -> &'static str {
        match self {
            EvidenceTarget::ProcessResult => "process-result",
            EvidenceTarget::ProcessStdout => "process-stdout",
            EvidenceTarget::ProcessStderr => "process-stderr",
            EvidenceTarget::StructuredOutputDocument => "structured-output-document",
            EvidenceTarget::ContainerInspectDocument => "container-inspect-document",
            EvidenceTarget::ImageInspectDocument => "image-inspect-document",
            EvidenceTarget::WorkspaceFile => "workspace-file",
        }
    }

    /// Whether this target is a JSON DOCUMENT (so the pointer kinds apply to it).
    pub fn is_json_document(self) -> bool {
        matches!(
            self,
            EvidenceTarget::StructuredOutputDocument
                | EvidenceTarget::ContainerInspectDocument
                | EvidenceTarget::ImageInspectDocument
        )
    }
}

/// The CLOSED perturbation vocabulary (contract regression-harness.md, "Perturbation
/// kinds"). Adding a sixth kind is an infrastructure decision, never case authoring.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PerturbationKind {
    /// Set one JSON pointer to a literal value.
    SetJsonPointer,
    /// Remove one JSON pointer.
    RemoveJsonPointer,
    /// Replace the exit status of a process result.
    SetExitCode,
    /// Append a marker to stdout, stderr, or a file's content.
    AppendBytes,
    /// Drop one entry from the filesystem the observer reads.
    RemovePath,
}

impl PerturbationKind {
    /// The stable wire spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            PerturbationKind::SetJsonPointer => "set-json-pointer",
            PerturbationKind::RemoveJsonPointer => "remove-json-pointer",
            PerturbationKind::SetExitCode => "set-exit-code",
            PerturbationKind::AppendBytes => "append-bytes",
            PerturbationKind::RemovePath => "remove-path",
        }
    }
}

/// One declarative, reversible perturbation.
///
/// Held as a struct with a closed [`kind`](Perturbation::kind) rather than as an
/// externally/internally tagged enum so the cross-field rules produce *named-field*
/// diagnostics ("`set-exit-code` requires `exitCode`") instead of serde's untagged
/// "data did not match any variant", which names nothing an author can act on.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", try_from = "RawPerturbation")]
pub struct Perturbation {
    /// Which of the five kinds this is.
    pub kind: PerturbationKind,
    /// RFC-6901 JSON pointer — required by the two pointer kinds, forbidden otherwise.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pointer: Option<String>,
    /// The literal to set — required by `set-json-pointer`, forbidden otherwise.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<serde_json::Value>,
    /// The replacement exit status — required by `set-exit-code`, forbidden otherwise.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,
    /// The marker to append — required by `append-bytes`, forbidden otherwise.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bytes: Option<String>,
    /// The workspace-relative path — required by `remove-path`, and by `append-bytes`
    /// when (and only when) the target is a workspace file.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
}

/// The on-disk perturbation shape, validated into [`Perturbation`] by [`TryFrom`].
/// `deny_unknown_fields` lives here, on the shape that actually reads the file.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RawPerturbation {
    kind: PerturbationKind,
    #[serde(default)]
    pointer: Option<String>,
    #[serde(default)]
    value: Option<serde_json::Value>,
    #[serde(default)]
    exit_code: Option<i32>,
    #[serde(default)]
    bytes: Option<String>,
    #[serde(default)]
    path: Option<String>,
}

impl TryFrom<RawPerturbation> for Perturbation {
    type Error = String;

    fn try_from(raw: RawPerturbation) -> Result<Self, Self::Error> {
        use PerturbationKind::*;

        // `(field name, is-present)` for every field the kind does NOT use — all of which
        // must be absent, so a record can never carry a payload that is silently ignored.
        let all = [
            ("pointer", raw.pointer.is_some()),
            ("value", raw.value.is_some()),
            ("exitCode", raw.exit_code.is_some()),
            ("bytes", raw.bytes.is_some()),
            ("path", raw.path.is_some()),
        ];
        let required: &[&str] = match raw.kind {
            SetJsonPointer => &["pointer", "value"],
            RemoveJsonPointer => &["pointer"],
            SetExitCode => &["exitCode"],
            AppendBytes => &["bytes"],
            RemovePath => &["path"],
        };
        // `append-bytes` may additionally carry a `path` (appending to a workspace file
        // rather than to a stream). Every other combination is refused.
        let optional: &[&str] = match raw.kind {
            AppendBytes => &["path"],
            _ => &[],
        };

        for field in required {
            if !all.iter().any(|(name, present)| name == field && *present) {
                return Err(format!(
                    "perturbation kind `{}` requires `{field}`, which is absent — a \
                     perturbation missing its payload could never be applied, so it would \
                     manufacture a false `inert` verdict for its channel",
                    raw.kind.as_str()
                ));
            }
        }
        for (name, present) in all {
            if present && !required.contains(&name) && !optional.contains(&name) {
                return Err(format!(
                    "perturbation kind `{}` does not use `{name}`; carrying it would be a \
                     silently ignored payload. Remove the field, or change the kind.",
                    raw.kind.as_str()
                ));
            }
        }
        if let Some(bytes) = &raw.bytes
            && bytes.is_empty()
        {
            return Err(
                "`bytes` is empty — appending nothing perturbs nothing, so the channel \
                 would report `inert` for a reason that has nothing to do with the channel"
                    .to_string(),
            );
        }
        if let Some(pointer) = &raw.pointer
            && !pointer.starts_with('/')
        {
            return Err(format!(
                "`pointer` must be an RFC-6901 JSON pointer starting with `/`, got {pointer:?}"
            ));
        }

        Ok(Perturbation {
            kind: raw.kind,
            pointer: raw.pointer,
            value: raw.value,
            exit_code: raw.exit_code,
            bytes: raw.bytes,
            path: raw.path,
        })
    }
}

/// One injected-regression record (data-model.md §7).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RegressionRecord {
    /// `reg-<slug>`.
    pub id: String,
    /// The declared observable channel this record proves is live. Every declared
    /// channel needs ≥1 record, and a channel with no registered observer is refused
    /// (**V30**).
    pub channel: String,
    /// The evidence source the perturbation is applied to — never an observer's return
    /// value (FR-065b).
    pub target: EvidenceTarget,
    /// The declarative, reversible perturbation.
    pub perturbation: Perturbation,
    /// The cases the record is evaluated against.
    ///
    /// data-model.md §7 calls this *informational*, and it is: the run reports what
    /// ACTUALLY detected the regression (`detectedBy`), which may be a strict subset. It
    /// is nevertheless required to be non-empty, because it is also the candidate set the
    /// harness runs — a record naming no case could never be detected by anything, and
    /// would report `inert` for a reason that says nothing about the channel.
    #[serde(deserialize_with = "non_empty_vec")]
    pub expected_detecting_cases: Vec<String>,
    /// Free-form reviewer note: why this perturbation is a MEANINGFUL difference on this
    /// channel, and why the named cases should catch it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
}

/// The `regressions.json` envelope — a `records` collection, matching every other
/// hand-authored registry file.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RegressionFile {
    #[serde(default)]
    pub records: Vec<RegressionRecord>,
}

/// Reject an empty candidate list at deserialize time (see
/// [`RegressionRecord::expected_detecting_cases`]).
fn non_empty_vec<'de, D>(de: D) -> Result<Vec<String>, D::Error>
where
    D: Deserializer<'de>,
{
    let value = Vec::<String>::deserialize(de)?;
    if value.is_empty() {
        return Err(serde::de::Error::custom(
            "must name at least one candidate case — a regression evaluated against no \
             case can never be detected, so it would report a false `inert` for its channel",
        ));
    }
    Ok(value)
}

/// Load `regressions.json` from `path`.
///
/// A missing file yields an empty list (a fixture registry that ships none is not an
/// error; V30 is what refuses an EMPTY set for a registry that has opted in). A
/// present-but-malformed file is a located [`LoadError::Schema`] — never silently empty
/// (constitution IV).
pub fn load_regressions(path: &Path) -> Result<Vec<RegressionRecord>, LoadError> {
    if !path.exists() {
        return Ok(Vec::new());
    }
    let raw = read_file(path).map_err(|e| LoadError::Schema(vec![e]))?;
    let file: RegressionFile =
        deserialize_located(path, &raw).map_err(|e| LoadError::Schema(vec![e]))?;
    Ok(file.records)
}

/// Collect duplicate-`id` regressions as located schema errors, so two records can never
/// silently claim the same identity.
pub(crate) fn duplicate_id_errors(path: &Path, records: &[RegressionRecord]) -> Vec<SchemaError> {
    let mut seen: std::collections::HashSet<&str> = std::collections::HashSet::new();
    let mut out = Vec::new();
    for record in records {
        if !seen.insert(record.id.as_str()) {
            out.push(SchemaError {
                file: path.to_path_buf(),
                location: Some(record.id.clone()),
                message: format!("duplicate regression id `{}`", record.id),
            });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const GOOD: &str = r##"{
      "records": [
        {
          "id": "reg-chan-image-label",
          "channel": "chan-image",
          "target": "image-inspect-document",
          "perturbation": {
            "kind": "set-json-pointer",
            "pointer": "/Config/Labels/devcontainer.metadata",
            "value": "injected"
          },
          "expectedDetectingCases": ["case-build-image-metadata-labels"]
        }
      ]
    }"##;

    #[test]
    fn the_data_model_example_loads() {
        let file: RegressionFile = serde_json::from_str(GOOD).expect("data-model §7 example loads");
        let record = &file.records[0];
        assert_eq!(record.id, "reg-chan-image-label");
        assert_eq!(record.target, EvidenceTarget::ImageInspectDocument);
        assert_eq!(record.perturbation.kind, PerturbationKind::SetJsonPointer);
        assert_eq!(
            record.perturbation.pointer.as_deref(),
            Some("/Config/Labels/devcontainer.metadata")
        );
        assert_eq!(record.expected_detecting_cases.len(), 1);
        assert!(record.target.is_json_document());
    }

    #[test]
    fn every_kind_round_trips_with_its_own_payload() {
        let cases = [
            (
                r#"{"kind":"remove-json-pointer","pointer":"/State/Status"}"#,
                PerturbationKind::RemoveJsonPointer,
            ),
            (
                r#"{"kind":"set-exit-code","exitCode":0}"#,
                PerturbationKind::SetExitCode,
            ),
            (
                r#"{"kind":"append-bytes","bytes":"INJECTED"}"#,
                PerturbationKind::AppendBytes,
            ),
            (
                r#"{"kind":"remove-path","path":"applied/x.json"}"#,
                PerturbationKind::RemovePath,
            ),
        ];
        for (raw, kind) in cases {
            let p: Perturbation =
                serde_json::from_str(raw).unwrap_or_else(|e| panic!("{raw} must load: {e}"));
            assert_eq!(p.kind, kind);
            // Round-trips: the serialized form omits every unused field.
            let back = serde_json::to_string(&p).expect("serializes");
            let reparsed: Perturbation = serde_json::from_str(&back).expect("re-loads");
            assert_eq!(reparsed, p, "round-trip is lossless for {raw}");
        }
        // `append-bytes` may carry a path (a workspace file rather than a stream).
        let p: Perturbation =
            serde_json::from_str(r#"{"kind":"append-bytes","bytes":"X","path":"a/b.txt"}"#)
                .expect("append-bytes may target a file");
        assert_eq!(p.path.as_deref(), Some("a/b.txt"));
    }

    #[test]
    fn a_kind_missing_its_payload_is_rejected_at_deserialize_time() {
        let err = serde_json::from_str::<Perturbation>(r#"{"kind":"set-exit-code"}"#)
            .expect_err("a payload-less set-exit-code must not load");
        assert!(
            err.to_string().contains("exitCode"),
            "the diagnosis must name the missing field, got: {err}"
        );
        let err =
            serde_json::from_str::<Perturbation>(r#"{"kind":"set-json-pointer","pointer":"/a"}"#)
                .expect_err("set-json-pointer without a value must not load");
        assert!(err.to_string().contains("value"), "got: {err}");
    }

    #[test]
    fn a_payload_the_kind_does_not_use_is_rejected() {
        // An `exitCode` on a pointer kind would be silently ignored — which is exactly how
        // a regression ends up not perturbing what its author believed it perturbed.
        let err = serde_json::from_str::<Perturbation>(
            r#"{"kind":"remove-json-pointer","pointer":"/a","exitCode":3}"#,
        )
        .expect_err("an unused payload must not load");
        assert!(err.to_string().contains("exitCode"), "got: {err}");
    }

    #[test]
    fn an_empty_marker_or_a_relative_pointer_is_rejected() {
        assert!(
            serde_json::from_str::<Perturbation>(r#"{"kind":"append-bytes","bytes":""}"#).is_err(),
            "appending nothing perturbs nothing"
        );
        let err = serde_json::from_str::<Perturbation>(
            r#"{"kind":"remove-json-pointer","pointer":"State/Status"}"#,
        )
        .expect_err("a pointer must be RFC-6901");
        assert!(err.to_string().contains("RFC-6901"), "got: {err}");
    }

    #[test]
    fn a_record_naming_no_candidate_case_is_rejected() {
        let raw = GOOD.replace(r#"["case-build-image-metadata-labels"]"#, "[]");
        let err = serde_json::from_str::<RegressionFile>(&raw)
            .expect_err("a record with no candidate case can never be detected");
        assert!(err.to_string().contains("at least one"), "got: {err}");
    }

    #[test]
    fn unknown_fields_are_rejected() {
        let raw = GOOD.replace("\"channel\":", "\"surprise\":");
        assert!(serde_json::from_str::<RegressionFile>(&raw).is_err());
    }

    #[test]
    fn there_is_no_target_naming_an_observer_output() {
        // FR-065b at the vocabulary level: the closed target set contains only RAW
        // captured artifacts, so a record cannot even ASK to be injected downstream of an
        // observer. (`parity_harness::inject` enforces the same thing at the type level.)
        for spelling in [
            "\"raw-channel-evidence\"",
            "\"observer-output\"",
            "\"normalized-evidence\"",
        ] {
            assert!(
                serde_json::from_str::<EvidenceTarget>(spelling).is_err(),
                "{spelling} must not be a declarable evidence target"
            );
        }
    }

    #[test]
    fn missing_file_loads_empty_and_malformed_file_fails_loud() {
        let dir = tempfile::tempdir().expect("tempdir");
        assert!(
            load_regressions(&dir.path().join("regressions.json"))
                .expect("absent file is not an error")
                .is_empty()
        );
        let bad = dir.path().join("bad.json");
        std::fs::write(&bad, "{ not json").expect("write fixture");
        assert!(
            load_regressions(&bad).is_err(),
            "a malformed regressions.json must fail loud, never default to empty"
        );
    }

    #[test]
    fn duplicate_ids_are_reported() {
        let file: RegressionFile = serde_json::from_str(GOOD).expect("loads");
        let mut records = file.records.clone();
        records.push(file.records[0].clone());
        let errors = duplicate_id_errors(Path::new("regressions.json"), &records);
        assert_eq!(errors.len(), 1);
        assert!(errors[0].message.contains("reg-chan-image-label"));
        assert!(duplicate_id_errors(Path::new("regressions.json"), &file.records).is_empty());
    }
}
