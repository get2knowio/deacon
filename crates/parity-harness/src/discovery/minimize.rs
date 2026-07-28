//! The live reproduction predicate supplied to the hermetic shrinker
//! (025-exploratory-parity-discovery, T048, FR-019 – FR-023).
//!
//! [`deacon_conformance::discovery::shrink`] takes its predicate as a **parameter**
//! precisely so the reduction strategy can be unit-tested against a synthetic one; this
//! module is the real one — it re-runs both implementations over a reduced input and reports
//! whether the *signature under reduction* is still there (research D4/D5).
//!
//! ## Nothing here is a new mechanism
//!
//! A probe is one call to [`super::differential::compare`]: the same bounded execution, the
//! same single normalization definition, the same tolerance index, the same signature
//! derivation. Re-implementing any of it would give minimization a second opinion on what
//! differs, able to disagree with the comparison that produced the finding — at which point
//! a "reduced input that still reproduces" would be a claim about a comparison nobody ran.
//!
//! ## Two things held constant across every probe, and why
//!
//! - **The workspace shape.** A probe materializes the reduced document into the *same*
//!   tree shape [`super::campaign`] materializes a candidate into. A probe workspace whose
//!   Compose or Dockerfile scaffolding differed would be measuring the scaffold.
//! - **`deliberately_invalid`.** It is a fact about the *candidate's provenance* — a
//!   near-valid draw, or a mutated document — not about the document currently under the
//!   knife. Recomputing it as the reduction un-applies mutations would move the tolerance
//!   boundary mid-reduction, so "the signature was preserved" would silently start meaning
//!   something different halfway through.
//!
//! ## What each of the three answers means here
//!
//! | Probe answer | Live meaning |
//! |---|---|
//! | [`Preserved`](Reproduction::Preserved) | the target signature is among the observations |
//! | [`Drifted`](Reproduction::Drifted) | it is not, but other **new** differences are — captured as candidate findings (FR-023) |
//! | [`Absent`](Reproduction::Absent) | it is not, and nothing else is news either |
//!
//! An already-*characterized* observation never counts as drift. It is not a candidate
//! finding by definition (FR-017), and admitting it here would let minimization author
//! findings the differential itself refuses to raise.

use std::path::{Path, PathBuf};
use std::time::Duration;

use deacon_conformance::discovery::shrink::{Reproduction, ReproductionProbe};
use deacon_conformance::discovery::signature::Signature;
use serde_json::Value;

use crate::HarnessError;
use crate::oracle::VerifiedOracle;

use super::campaign::materialize_document;
use super::differential::{self, Characterization, DifferentialInput};

/// Everything a probe needs, resolved once by the caller.
///
/// Owned rather than borrowed for the two path fields so a probe can be constructed at the
/// call site without threading a lifetime through the campaign loop; the oracle and the
/// tolerance index stay borrowed because both are shared, immutable, and expensive to clone.
pub struct DifferentialProbe<'a> {
    /// The signature the reduction must preserve.
    target: Signature,
    /// The candidate the finding came from — the artifact-tree prefix, so a reviewer can
    /// find a probe's raw output beside the candidate's own.
    candidate_id: String,
    deacon: PathBuf,
    /// The **verified** pinned oracle. Taking the verified type rather than a path is what
    /// makes "never compare against an unverified reference" (FR-003) a type-level fact at
    /// this call site rather than a rule the caller has to remember.
    oracle: &'a VerifiedOracle,
    bound: Duration,
    report_root: PathBuf,
    characterization: &'a Characterization,
    /// Constant for the whole reduction — see the module docs.
    deliberately_invalid: bool,
    /// How many probes have been made, so each writes to its own artifact directory rather
    /// than overwriting the previous one's raw capture.
    probes: u64,
}

impl<'a> DifferentialProbe<'a> {
    /// Build a probe for one finding under reduction.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        target: Signature,
        candidate_id: &str,
        deacon: &Path,
        oracle: &'a VerifiedOracle,
        bound: Duration,
        report_root: &Path,
        characterization: &'a Characterization,
        deliberately_invalid: bool,
    ) -> DifferentialProbe<'a> {
        DifferentialProbe {
            target,
            candidate_id: candidate_id.to_string(),
            deacon: deacon.to_path_buf(),
            oracle,
            bound,
            report_root: report_root.to_path_buf(),
            characterization,
            deliberately_invalid,
            probes: 0,
        }
    }

    /// How many probes this instance made — the expensive unit, reported so a campaign can
    /// say what minimization cost it.
    pub fn probes(&self) -> u64 {
        self.probes
    }

    async fn run(&mut self, document: &Value) -> Result<Reproduction, HarnessError> {
        let workspace = materialize_document(document).map_err(|e| HarnessError::Report {
            cause: format!(
                "could not materialize a reduced workspace for `{}`: {e}",
                self.candidate_id
            ),
        })?;
        // A per-probe artifact name, so the raw capture of every probe survives. Reusing the
        // candidate's own name would leave only the LAST probe's output on disk, and the
        // reduction's most interesting artifact is usually the probe that failed.
        let case = format!("{}-shrink-{:04}", self.candidate_id, self.probes);
        self.probes += 1;

        let result = differential::compare(
            DifferentialInput {
                candidate_id: &case,
                workspace: workspace.path(),
                deacon: &self.deacon,
                oracle: self.oracle,
                bound: self.bound,
                report_root: &self.report_root,
                deliberately_invalid: self.deliberately_invalid,
            },
            self.characterization,
        )
        .await;

        // A per-candidate timeout during minimization is a fact about *this probe*, not a
        // failure of the campaign: the reduced input happened to be pathological. It is
        // reported as `Absent` — the reduction did not demonstrate reproduction — so the
        // step is rejected and the reduction continues, exactly as the campaign loop treats
        // a timed-out candidate rather than letting one input consume the whole budget.
        let result = match result {
            Ok(r) => r,
            Err(HarnessError::OracleTimeout { .. }) => {
                tracing::debug!(
                    candidate = %self.candidate_id,
                    probe = %case,
                    "a minimization probe exceeded its bound; the step is rejected"
                );
                return Ok(Reproduction::Absent);
            }
            Err(other) => return Err(other),
        };

        if result
            .observations
            .iter()
            .any(|o| o.signature.id == self.target.id)
        {
            return Ok(Reproduction::Preserved);
        }

        // FR-023: what appeared *instead*. Only genuinely new observations — an
        // already-characterized difference is not a candidate finding (FR-017), and
        // capturing one here would let minimization raise what the differential refuses to.
        let drifted: Vec<Signature> = result
            .new_observations()
            .map(|o| o.signature.clone())
            .collect();
        Ok(if drifted.is_empty() {
            Reproduction::Absent
        } else {
            Reproduction::Drifted(drifted)
        })
    }
}

impl ReproductionProbe for DifferentialProbe<'_> {
    type Error = HarnessError;

    async fn probe(&mut self, document: &Value) -> Result<Reproduction, HarnessError> {
        self.run(document).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use deacon_conformance::discovery::signature::{Divergence, DivergenceKind};
    use deacon_conformance::model::CHAN_STRUCTURED_OUTPUT;

    fn signature(path: &str) -> Signature {
        Signature::derive(
            CHAN_STRUCTURED_OUTPUT,
            &Divergence {
                kind: DivergenceKind::Value,
                path,
                deacon: None,
                reference: None,
            },
        )
    }

    /// The probe's own bookkeeping is asserted here; its *verdict* is asserted live, in
    /// `discovery_campaign`, because the verdict is exactly the thing that cannot be
    /// established without the reference.
    #[test]
    fn a_fresh_probe_has_spent_nothing() {
        let oracle = VerifiedOracle {
            path: PathBuf::from("/nonexistent/devcontainer"),
            source: crate::oracle::OracleSource::Override,
            version: "0.0.0".to_string(),
        };
        let characterization = Characterization::default();
        let probe = DifferentialProbe::new(
            signature("configuration.remoteUser"),
            "cnd-11111111",
            Path::new("/nonexistent/deacon"),
            &oracle,
            Duration::from_secs(1),
            Path::new("/tmp/does-not-matter"),
            &characterization,
            true,
        );
        assert_eq!(probe.probes(), 0);
        assert_eq!(probe.target.path, "configuration.remoteUser");
        assert!(
            probe.deliberately_invalid,
            "the provenance flag is held constant for the whole reduction, so it must be \
             what the caller passed rather than something recomputed per probe"
        );
    }
}
