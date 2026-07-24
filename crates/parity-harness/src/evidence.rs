//! Evidence + verdict base types for the declarative conformance runner (data-model
//! §9, 022-conformance-runner).
//!
//! An observer captures a channel as [`RawChannelEvidence`]; the single normalizer
//! ([`crate::normalize::normalize_channel`]) maps it to [`NormalizedChannelEvidence`];
//! [`crate::compare`] turns a pair (or a spec expectation) into a [`ChannelVerdict`];
//! the runner aggregates those into a [`CaseVerdict`]. Raw and normalized evidence are
//! persisted **separately** (raw.json / normalized.json, FR-016); the atomic write path
//! + separate persistence land in US2/US3 (T034/T045).
//!
//! `present: bool` is load-bearing: `present:false` means the channel was NOT captured
//! for this operation, which is distinct from a captured-but-empty value
//! (`present:true` with `value` `null`/`""`/`[]`) — FR-018. Named normalization rules
//! keep the null/empty/default distinction intact (FR-025).

use std::path::Path;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use deacon_conformance::model::OracleType;
use deacon_conformance::snapshot::Provenance;

use crate::HarnessError;

/// Verbatim per-channel evidence captured by an observer (data-model §9, FR-018).
///
/// Temp paths, host values, etc. are preserved as-observed; tokenization happens only
/// in the normalized copy (FR-024).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RawChannelEvidence {
    /// The channel id (`chan-…`).
    pub channel: String,
    /// The operation id this evidence was captured for.
    pub operation: String,
    /// `false` ⇒ the channel could not be observed for this op (not captured); distinct
    /// from a captured-but-empty `value` (FR-018).
    pub present: bool,
    /// The captured value (channel-specific shape). `null`/`""`/`[]` are captured-empty
    /// and remain distinct from `present:false`.
    pub value: Value,
}

/// Per-channel evidence after the named normalization rules (data-model §9). Same shape
/// as [`RawChannelEvidence`], but a distinct type so raw and normalized evidence can
/// never be accidentally compared or persisted to the wrong file.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NormalizedChannelEvidence {
    /// The channel id (`chan-…`).
    pub channel: String,
    /// The operation id this evidence was captured for.
    pub operation: String,
    /// `false` ⇒ not captured (preserved from the raw evidence, FR-018).
    pub present: bool,
    /// The normalized value; null/empty/default preserved distinctly (FR-025), nothing
    /// blanket-removed (FR-029).
    pub value: Value,
}

/// The verdict outcome on one channel (data-model §9, contract observer-channel.md).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Outcome {
    /// deacon and the reference/expectation agree after normalization.
    Agree,
    /// An uncharacterized divergence.
    Diverge,
    /// A divergence fully covered by a scoped `AllowedDifference` (US4).
    AllowedDifference,
    /// No committed snapshot exists for the current platform (coverage gap, FR-016a).
    NoReferenceForPlatform,
    /// A committed snapshot is stale (a provenance/hash field drifted, FR-020).
    Stale,
    /// The channel could not be verdicted (capture/normalization fault).
    Error,
}

impl Outcome {
    /// Severity rank for `CaseVerdict.overall = worst channel outcome` (data-model §9).
    /// Higher is worse. Ordered to match the runner's exit-code severity (contract
    /// runner-cli.md): `agree`/`allowed-difference` are clean (exit 0), then
    /// `no-reference-for-platform` (non-blocking coverage gap) < `diverge` (exit 1) <
    /// `stale` (exit 3) < `error` (exit 4).
    pub fn severity(self) -> u8 {
        match self {
            Outcome::Agree => 0,
            Outcome::AllowedDifference => 1,
            Outcome::NoReferenceForPlatform => 2,
            Outcome::Diverge => 3,
            Outcome::Stale => 4,
            Outcome::Error => 5,
        }
    }
}

/// A per-channel verdict (data-model §9, FR-015).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChannelVerdict {
    /// The channel id (`chan-…`).
    pub channel: String,
    /// The outcome on this channel.
    pub outcome: Outcome,
    /// Cause-specific detail (waiver id, mismatched field, …) or `null`. Always
    /// serialized (even when `null`) so the report body is byte-deterministic
    /// (contract runner-cli.md).
    pub detail: Option<Value>,
}

/// The whole-case verdict (data-model §9, report shape contract runner-cli.md).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CaseVerdict {
    /// The case id.
    pub case_id: String,
    /// The oracle type the case was evaluated under.
    pub oracle_type: OracleType,
    /// The behaviors this case is attributable to (FR-042).
    pub behaviors: Vec<String>,
    /// Per-channel verdicts, in declaration order (never `BTreeMap`).
    pub channels: Vec<ChannelVerdict>,
    /// The worst channel outcome (see [`Outcome::severity`]).
    pub overall: Outcome,
    /// Allowed differences declared on this case whose characterized divergence did NOT
    /// reproduce this run — self-invalidating STALE tolerances to remove or re-characterize
    /// (FR-034, US4). Empty (and omitted) when every declared tolerance was consumed.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub stale_allowed_differences: Vec<String>,
}

impl CaseVerdict {
    /// The worst channel outcome across `channels` (data-model §9). An empty channel
    /// list verdicts as [`Outcome::Agree`] (nothing diverged).
    pub fn compute_overall(channels: &[ChannelVerdict]) -> Outcome {
        channels
            .iter()
            .map(|c| c.outcome)
            .max_by_key(|o| o.severity())
            .unwrap_or(Outcome::Agree)
    }
}

/// One run's captured evidence, with raw and normalized held **separately** (FR-016,
/// SC-006). Each is independently retrievable, and the two are never conflated: raw
/// preserves temp paths verbatim; normalized shows the `<WORKSPACE>` tokens. US2 (T034)
/// persists these to the sibling `raw.json` / `normalized.json` files atomically; US3
/// keeps the in-memory separation and threads it through the runner.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CaseEvidence {
    /// Verbatim per-channel evidence (temp paths preserved).
    pub raw: Vec<RawChannelEvidence>,
    /// Rule-normalized per-channel evidence (paths tokenized, nulls preserved).
    pub normalized: Vec<NormalizedChannelEvidence>,
}

impl CaseEvidence {
    /// An empty evidence set.
    pub fn new() -> CaseEvidence {
        CaseEvidence::default()
    }

    /// Record a channel's raw + normalized evidence as a matched pair.
    pub fn push(&mut self, raw: RawChannelEvidence, normalized: NormalizedChannelEvidence) {
        self.raw.push(raw);
        self.normalized.push(normalized);
    }

    /// The raw evidence for `(channel, operation)`, if captured — independently
    /// retrievable from the normalized copy.
    pub fn raw_for(&self, channel: &str, operation: &str) -> Option<&RawChannelEvidence> {
        self.raw
            .iter()
            .find(|e| e.channel == channel && e.operation == operation)
    }

    /// The normalized evidence for `(channel, operation)`, if captured.
    pub fn normalized_for(
        &self,
        channel: &str,
        operation: &str,
    ) -> Option<&NormalizedChannelEvidence> {
        self.normalized
            .iter()
            .find(|e| e.channel == channel && e.operation == operation)
    }

    /// Serialize the raw evidence array to a byte-stable pretty JSON string.
    fn raw_json(&self) -> Result<String, HarnessError> {
        render_json(&self.raw)
    }

    /// Serialize the normalized evidence array to a byte-stable pretty JSON string.
    fn normalized_json(&self) -> Result<String, HarnessError> {
        render_json(&self.normalized)
    }
}

/// Serialize a value to pretty JSON with a trailing newline (byte-stable), mapping a
/// serialization failure to a fail-loud [`HarnessError`].
fn render_json<T: Serialize>(value: &T) -> Result<String, HarnessError> {
    let mut s = serde_json::to_string_pretty(value).map_err(|e| HarnessError::Report {
        cause: format!("could not serialize snapshot evidence: {e}"),
    })?;
    s.push('\n');
    Ok(s)
}

/// Write a committed snapshot's three files — `provenance.json`, `raw.json`,
/// `normalized.json` — into `dir` ATOMICALLY (temp file + `fs::rename` via
/// [`crate::atomic_write`], FR-019/D4). Raw and normalized are kept SEPARATE (FR-016).
/// Because each write renames a fully-written temp file into place, a shorter payload
/// can never leave trailing bytes from a previous longer file. Used ONLY by the reviewed
/// refresh bin — ordinary runs never call it (FR-021).
pub async fn write_snapshot(
    dir: &Path,
    provenance: &Provenance,
    evidence: &CaseEvidence,
) -> Result<(), HarnessError> {
    let provenance_json = render_json(provenance)?;
    crate::atomic_write(&dir.join("provenance.json"), provenance_json.as_bytes()).await?;
    crate::atomic_write(&dir.join("raw.json"), evidence.raw_json()?.as_bytes()).await?;
    crate::atomic_write(
        &dir.join("normalized.json"),
        evidence.normalized_json()?.as_bytes(),
    )
    .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn present_false_is_distinct_from_empty_value() {
        let not_captured = RawChannelEvidence {
            channel: "chan-stdout".to_string(),
            operation: "op-1".to_string(),
            present: false,
            value: Value::Null,
        };
        let captured_empty = RawChannelEvidence {
            channel: "chan-stdout".to_string(),
            operation: "op-1".to_string(),
            present: true,
            value: Value::String(String::new()),
        };
        assert_ne!(
            not_captured, captured_empty,
            "not-captured must differ from captured-but-empty (FR-018)"
        );
    }

    #[test]
    fn overall_is_worst_channel_outcome() {
        let mk = |outcome| ChannelVerdict {
            channel: "chan-exit-code".to_string(),
            outcome,
            detail: None,
        };
        assert_eq!(
            CaseVerdict::compute_overall(&[mk(Outcome::Agree), mk(Outcome::Diverge)]),
            Outcome::Diverge
        );
        assert_eq!(
            CaseVerdict::compute_overall(&[
                mk(Outcome::AllowedDifference),
                mk(Outcome::NoReferenceForPlatform)
            ]),
            Outcome::NoReferenceForPlatform
        );
        assert_eq!(
            CaseVerdict::compute_overall(&[mk(Outcome::Diverge), mk(Outcome::Stale)]),
            Outcome::Stale
        );
        assert_eq!(CaseVerdict::compute_overall(&[]), Outcome::Agree);
    }

    #[test]
    fn case_evidence_keeps_raw_and_normalized_separate_and_retrievable() {
        let mut ev = CaseEvidence::new();
        let raw = RawChannelEvidence {
            channel: "chan-structured-output".to_string(),
            operation: "op-read".to_string(),
            present: true,
            value: serde_json::json!({ "root": "/tmp/ws-abc" }),
        };
        let normalized = NormalizedChannelEvidence {
            channel: "chan-structured-output".to_string(),
            operation: "op-read".to_string(),
            present: true,
            value: serde_json::json!({ "root": "<WORKSPACE>" }),
        };
        ev.push(raw.clone(), normalized.clone());

        // Independently retrievable, and NOT conflated (raw keeps the temp path).
        let got_raw = ev.raw_for("chan-structured-output", "op-read").unwrap();
        let got_norm = ev
            .normalized_for("chan-structured-output", "op-read")
            .unwrap();
        assert_eq!(got_raw.value["root"], serde_json::json!("/tmp/ws-abc"));
        assert_eq!(got_norm.value["root"], serde_json::json!("<WORKSPACE>"));
        assert_ne!(
            got_raw.value, got_norm.value,
            "raw and normalized must stay separate (FR-016)"
        );
        assert_eq!(ev.raw.len(), 1);
        assert_eq!(ev.normalized.len(), 1);
    }

    #[tokio::test]
    async fn write_snapshot_is_atomic_and_leaves_no_trailing_bytes() {
        use deacon_conformance::snapshot::Provenance;
        let dir = tempfile::tempdir().expect("tempdir");
        let mut prov = Provenance {
            oracle_version: "0.87.0".to_string(),
            source_revision: "113500f4".to_string(),
            case_hash: "aaaa".to_string(),
            fixture_hash: "bbbb".to_string(),
            argv: vec!["read-configuration".to_string()],
            platform: "linux".to_string(),
            arch: "x86_64".to_string(),
            node_version: "22.23.1".to_string(),
            docker_version: "29.6.2".to_string(),
            compose_version: "2.40.3".to_string(),
            image_digests: Default::default(),
            normalizer_version: "2".to_string(),
            captured_at: "2026-07-24T00:00:00Z".to_string(),
        };

        // First write: a LONG raw evidence array.
        let long_evidence = CaseEvidence {
            raw: vec![RawChannelEvidence {
                channel: "chan-stdout".to_string(),
                operation: "op".to_string(),
                present: true,
                value: Value::String("a-very-long-first-payload-".repeat(20)),
            }],
            normalized: Vec::new(),
        };
        write_snapshot(dir.path(), &prov, &long_evidence)
            .await
            .expect("first write");

        // Second write: a SHORTER raw evidence array over the same file.
        prov.case_hash = "cccc".to_string();
        let short_evidence = CaseEvidence {
            raw: vec![RawChannelEvidence {
                channel: "chan-exit-code".to_string(),
                operation: "op".to_string(),
                present: true,
                value: Value::from(0),
            }],
            normalized: Vec::new(),
        };
        write_snapshot(dir.path(), &prov, &short_evidence)
            .await
            .expect("second write");

        // The shorter payload must be intact — no trailing bytes from the longer one.
        let raw_back = std::fs::read_to_string(dir.path().join("raw.json")).unwrap();
        let parsed: Value = serde_json::from_str(&raw_back).expect("raw.json parses clean");
        assert_eq!(parsed[0]["channel"], "chan-exit-code");
        assert_eq!(parsed[0]["value"], 0);
        assert!(
            !raw_back.contains("a-very-long"),
            "no trailing bytes remain"
        );

        // Provenance + normalized files exist and parse.
        let prov_back = std::fs::read_to_string(dir.path().join("provenance.json")).unwrap();
        let prov_parsed: Provenance = serde_json::from_str(&prov_back).unwrap();
        assert_eq!(prov_parsed.case_hash, "cccc");
        assert!(dir.path().join("normalized.json").is_file());

        // No temp files survive a successful write.
        let leftovers: Vec<_> = std::fs::read_dir(dir.path())
            .unwrap()
            .filter_map(Result::ok)
            .filter(|e| e.file_name().to_string_lossy().starts_with(".tmp-"))
            .collect();
        assert!(leftovers.is_empty(), "temp files must be renamed away");
    }

    #[test]
    fn channel_verdict_serializes_null_detail() {
        let v = ChannelVerdict {
            channel: "chan-exit-code".to_string(),
            outcome: Outcome::Agree,
            detail: None,
        };
        let json = serde_json::to_string(&v).unwrap();
        assert!(
            json.contains("\"detail\":null"),
            "detail must serialize even when null for byte-determinism, got {json}"
        );
        assert!(json.contains("\"outcome\":\"agree\""));
    }
}
