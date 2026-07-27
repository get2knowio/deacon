//! Exit-status guard for the `discovery` command group
//! (025-exploratory-parity-discovery, contracts/discovery-cli.md).
//!
//! Hermetic: builds a scratch `conformance/` tree pointing at the real registry and
//! drives the `conformance` binary against it. No Docker, no network, no oracle — so it
//! carries no nextest override and runs in every profile, like every other conformance
//! guard.
//!
//! ## Why the exit statuses need their own test
//!
//! The contract's first rule is that a discovery command's status reflects **whether it
//! ran**, never **what it found**. That rule is the entire reason the discovery lane can
//! be scheduled in CI at all: the moment a status depends on findings, the lane becomes a
//! stochastic gate and green stops being reproducible.
//!
//! Nothing else asserts it. The library tests exercise the model and the wiring test
//! parses `.config/nextest.toml`, but the *process contract* — `report` exits `0` on a
//! queue full of untriaged findings, `check` exits `1` on a violation, `scaffold` refuses
//! a non-promotable finding — is only observable from outside the process.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

/// The workspace root, derived from this crate's manifest directory.
fn workspace_root() -> PathBuf {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest
        .parent()
        .and_then(Path::parent)
        .map(Path::to_path_buf)
        .unwrap_or(manifest)
}

/// A scratch `conformance/`-shaped tree: the real registry (so channels and revisions
/// resolve) beside a writable discovery root the test owns.
struct Scratch {
    dir: tempfile::TempDir,
}

impl Scratch {
    fn new() -> Scratch {
        let dir = tempfile::tempdir().expect("tempdir");
        let registry = dir.path().join("registry");
        // A symlink rather than a copy: the registry is large, read-only for every
        // command here, and copying it would make this guard a slow test for no gain.
        #[cfg(unix)]
        std::os::unix::fs::symlink(
            workspace_root().join("conformance").join("registry"),
            &registry,
        )
        .expect("link the real registry");
        #[cfg(windows)]
        std::os::windows::fs::symlink_dir(
            workspace_root().join("conformance").join("registry"),
            &registry,
        )
        .expect("link the real registry");

        std::fs::create_dir_all(dir.path().join("discovery")).expect("discovery root");
        let scratch = Scratch { dir };
        scratch.write("findings.json", EMPTY_COLLECTION);
        scratch.write("campaigns.json", EMPTY_COLLECTION);
        scratch.write("corpus.json", EMPTY_COLLECTION);
        scratch
    }

    fn registry(&self) -> PathBuf {
        self.dir.path().join("registry")
    }

    fn write(&self, name: &str, contents: &str) {
        std::fs::write(self.dir.path().join("discovery").join(name), contents)
            .unwrap_or_else(|e| panic!("write {name}: {e}"));
    }

    fn run(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_conformance"))
            .arg("--registry")
            .arg(self.registry())
            .args(args)
            .output()
            .expect("the conformance binary runs")
    }
}

const EMPTY_COLLECTION: &str = "{\n  \"schemaVersion\": 1,\n  \"records\": []\n}\n";

/// A queue holding one untriaged finding and the campaign that admitted it. Ids are the
/// real derived ones, so the fixture passes `check` — the point of several tests below is
/// that a *populated* queue still exits `0` from `report`.
fn populated_queue() -> (String, String, String) {
    use deacon_conformance::discovery::queue::{
        Campaign, CampaignLane, CampaignTier, PinnedInputSet, Witness,
    };
    use deacon_conformance::discovery::signature::{Divergence, DivergenceKind, Signature};

    let deacon = serde_json::json!("vscode");
    let reference = serde_json::json!("root");
    let signature = Signature::derive(
        "chan-structured-output",
        &Divergence {
            kind: DivergenceKind::Value,
            path: "configuration.remoteUser",
            deacon: Some(&deacon),
            reference: Some(&reference),
        },
    );
    let finding_id = signature.finding_id();

    // The campaign id is DERIVED, not chosen: `check` recomputes it from the record's own
    // substance, so a hand-picked id would (correctly) fail the D1 identity clause and
    // this fixture would stop being the clean queue the tests below need.
    let seed = "0x5eed1234";
    let profile = "prof-linux-amd64-docker-0870";
    let lane = CampaignLane::Scheduled;
    let tier = CampaignTier::ConfigDifferential;
    let pins = PinnedInputSet {
        schema_pin: deacon_conformance::CURRENT_SCHEMA_PIN.to_string(),
        prose_pin: deacon_conformance::CURRENT_SPEC_PIN.to_string(),
        oracle_version: "0.87.0".to_string(),
        normalizer_version: "6".to_string(),
        grammar_version: "rev-schema-113500f4".to_string(),
        mutation_catalog_version: "v1".to_string(),
        generator_version: "splitmix64-seed+xoshiro256starstar/v1".to_string(),
    };
    let campaign_id = Campaign::derive_id(seed, &pins, lane, profile, tier);
    let witness_id = Witness::derived_id(&campaign_id, "cnd-11111111");

    let findings = serde_json::json!({
        "schemaVersion": 1,
        "records": [{
            "id": finding_id,
            "signature": signature,
            "witnesses": [{
                "id": witness_id,
                "campaignId": campaign_id,
                "candidateId": "cnd-11111111",
                "minimalInput": { "image": "alpine:3.18" },
                "isMinimal": true,
                "reductionSteps": ["drop-optional-key"],
                "observedValues": { "deacon": "vscode", "reference": "root" },
                "mutationOperators": ["mop-wrong-type"]
            }],
            "classification": null,
            "state": "untriaged",
            "firstObserved": campaign_id,
            "lastObserved": campaign_id,
            "promotedTo": null,
            "splitFrom": null,
            "notes": ""
        }]
    });
    let campaigns = serde_json::json!({
        "schemaVersion": 1,
        "records": [{
            "id": campaign_id,
            "seed": seed,
            "lane": lane.as_str(),
            "tier": tier.as_str(),
            "profile": profile,
            "pinnedInputSet": pins,
            "budget": {
                "wallClockSeconds": 1800,
                "perCandidateSeconds": 60,
                "shrinkStepsPerFinding": 64,
                "admissionCap": 25
            },
            "outcome": {
                "candidatesGenerated": 4820,
                "candidatesExecuted": 4629,
                "candidatesDiscardedUnsafe": 0,
                "parseStageFailures": 191,
                "budgetExhausted": false,
                "spaceCoveredFraction": 0.0,
                "mutationApplications": { "unknown-field": 512 },
                "signaturesObserved": 1,
                "signaturesAdmitted": 1,
                "signaturesSuppressed": 0
            }
        }]
    });
    (
        finding_id,
        serde_json::to_string_pretty(&findings).expect("render findings"),
        serde_json::to_string_pretty(&campaigns).expect("render campaigns"),
    )
}

fn code(output: &Output) -> i32 {
    output.status.code().unwrap_or(-1)
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

#[test]
fn check_exits_zero_on_a_clean_data_root() {
    let scratch = Scratch::new();
    let out = scratch.run(&["discovery", "check"]);
    assert_eq!(code(&out), 0, "stderr: {}", stderr(&out));
}

#[test]
fn check_exits_one_and_reports_every_violation_in_one_pass() {
    let scratch = Scratch::new();
    let (_, findings, campaigns) = populated_queue();
    // Two independent violations: an undeclared channel (D1) and an unrecorded oracle
    // pin (D5). A checker that stopped at the first would make fixing a batch an
    // iterative guessing game.
    scratch.write(
        "findings.json",
        &findings.replace("chan-structured-output", "chan-invented"),
    );
    scratch.write("campaigns.json", &campaigns.replace("0.87.0", "9.9.9"));

    let out = scratch.run(&["discovery", "check"]);
    assert_eq!(code(&out), 1, "stderr: {}", stderr(&out));
    let report = stdout(&out);
    assert!(report.contains("D1 "), "violations go to stdout: {report}");
    assert!(report.contains("D5 "), "both classes reported: {report}");
}

#[test]
fn check_json_always_emits_a_document_on_stdout() {
    // Including when the data root itself will not load. A consumer doing
    // `discovery check --json | jq .ok` must get `false`, never a parse error — that is
    // the difference between "the check says no" and "the check crashed".
    let scratch = Scratch::new();

    let clean = scratch.run(&["discovery", "check", "--json"]);
    assert_eq!(code(&clean), 0);
    let doc: serde_json::Value = serde_json::from_str(&stdout(&clean)).expect("clean run is JSON");
    assert_eq!(doc["ok"], serde_json::json!(true));

    scratch.write("findings.json", "{ this is not json");
    let broken = scratch.run(&["discovery", "check", "--json"]);
    assert_eq!(code(&broken), 1, "stderr: {}", stderr(&broken));
    let doc: serde_json::Value =
        serde_json::from_str(&stdout(&broken)).expect("a malformed root still emits JSON");
    assert_eq!(doc["ok"], serde_json::json!(false));
    assert!(
        !doc["violations"]
            .as_array()
            .expect("violations array")
            .is_empty(),
        "the document must name what failed: {doc}"
    );
}

#[test]
fn check_refuses_a_truncated_file_rather_than_reading_it_as_empty() {
    let scratch = Scratch::new();
    scratch.write("findings.json", "{\n  \"schemaVersion\": 1\n}\n");
    let out = scratch.run(&["discovery", "check"]);
    assert_eq!(
        code(&out),
        1,
        "a records-less file is damage, not an empty queue; stderr: {}",
        stderr(&out)
    );
}

#[test]
fn report_exits_zero_whether_the_queue_is_empty_or_full() {
    // The never-gates rule (FR-058). Both of these are `0`, and that is the entire
    // reason the discovery lane is safe to schedule.
    let scratch = Scratch::new();
    let out_dir = scratch.dir.path().join("out");

    let empty = scratch.run(&[
        "discovery",
        "report",
        "--out-dir",
        &out_dir.to_string_lossy(),
    ]);
    assert_eq!(code(&empty), 0, "stderr: {}", stderr(&empty));
    assert!(out_dir.join("queue.json").is_file());
    assert!(out_dir.join("queue.md").is_file());

    let (_, findings, campaigns) = populated_queue();
    scratch.write("findings.json", &findings);
    scratch.write("campaigns.json", &campaigns);
    let full = scratch.run(&[
        "discovery",
        "report",
        "--out-dir",
        &out_dir.to_string_lossy(),
    ]);
    assert_eq!(
        code(&full),
        0,
        "a queue holding findings must still exit 0 — status reflects whether the command \
         ran, never what it found; stderr: {}",
        stderr(&full)
    );
    let rendered = std::fs::read_to_string(out_dir.join("queue.md")).expect("queue.md");
    assert!(rendered.contains("| untriaged | 1 |"), "{rendered}");
}

#[test]
fn report_exits_one_when_it_cannot_write() {
    let scratch = Scratch::new();
    // A path whose parent is a regular file cannot be created as a directory.
    let blocker = scratch.dir.path().join("blocker");
    std::fs::write(&blocker, "not a directory").expect("write");
    let out = scratch.run(&[
        "discovery",
        "report",
        "--out-dir",
        &blocker.join("nested").to_string_lossy(),
    ]);
    assert_eq!(code(&out), 1, "stderr: {}", stderr(&out));
}

#[test]
fn triage_records_a_classification_and_rejects_an_invalid_one() {
    let scratch = Scratch::new();
    let (finding_id, findings, campaigns) = populated_queue();
    scratch.write("findings.json", &findings);
    scratch.write("campaigns.json", &campaigns);

    let bad = scratch.run(&[
        "discovery",
        "triage",
        &finding_id,
        "--classification",
        "probably-fine",
    ]);
    assert_eq!(code(&bad), 1);
    assert!(
        stderr(&bad).contains("deacon-regression"),
        "the diagnosis must list the closed set: {}",
        stderr(&bad)
    );

    let unknown = scratch.run(&[
        "discovery",
        "triage",
        "fnd-nowhere",
        "--classification",
        "deacon-regression",
    ]);
    assert_eq!(code(&unknown), 1);

    let ok = scratch.run(&[
        "discovery",
        "triage",
        &finding_id,
        "--classification",
        "deacon-regression",
        "--notes",
        "extends child overrides parent remoteUser",
    ]);
    assert_eq!(code(&ok), 0, "stderr: {}", stderr(&ok));

    // The write landed and still validates.
    assert_eq!(code(&scratch.run(&["discovery", "check"])), 0);
    let written =
        std::fs::read_to_string(scratch.dir.path().join("discovery").join("findings.json"))
            .expect("read back");
    assert!(written.contains("\"classification\": \"deacon-regression\""));
    assert!(written.contains("\"state\": \"triaged\""));
}

#[test]
fn triage_does_not_revive_a_no_longer_reproducing_finding() {
    // Only a campaign that actually reproduces it may move it back to `triaged`
    // (contracts/findings-queue.md, "Reproduction lifecycle"). Setting the state here
    // would assert an observation nothing made and silently empty the FR-033 bucket.
    let scratch = Scratch::new();
    let (finding_id, findings, campaigns) = populated_queue();
    scratch.write(
        "findings.json",
        &findings.replace(
            "\"state\": \"untriaged\"",
            "\"state\": \"no-longer-reproducing\"",
        ),
    );
    scratch.write("campaigns.json", &campaigns);

    let out = scratch.run(&[
        "discovery",
        "triage",
        &finding_id,
        "--classification",
        "deacon-regression",
    ]);
    assert_eq!(code(&out), 0, "stderr: {}", stderr(&out));
    let written =
        std::fs::read_to_string(scratch.dir.path().join("discovery").join("findings.json"))
            .expect("read back");
    assert!(
        written.contains("\"state\": \"no-longer-reproducing\""),
        "the state must survive triage: {written}"
    );
    assert!(written.contains("\"classification\": \"deacon-regression\""));
}

#[test]
fn scaffold_writes_nothing_and_refuses_a_non_promotable_finding() {
    let scratch = Scratch::new();
    let (finding_id, findings, campaigns) = populated_queue();
    scratch.write("findings.json", &findings);
    scratch.write("campaigns.json", &campaigns);

    let before =
        std::fs::read_to_string(scratch.dir.path().join("discovery").join("findings.json"))
            .expect("read");
    let out = scratch.run(&["discovery", "scaffold", &finding_id]);
    assert_eq!(code(&out), 0, "stderr: {}", stderr(&out));
    let document: serde_json::Value =
        serde_json::from_str(&stdout(&out)).expect("the skeleton is JSON on stdout");
    assert_eq!(
        document["behavior"]["id"],
        serde_json::json!("bhv-UNREVIEWED")
    );
    assert_eq!(
        std::fs::read_to_string(scratch.dir.path().join("discovery").join("findings.json"))
            .expect("read"),
        before,
        "scaffold writes NOTHING — there is no code path from a finding to a registry write"
    );

    // `normalizer-defect` describes a defect in the discovery machinery, not a behavior
    // of either implementation, so it can never be promoted (FR-035).
    scratch.write(
        "findings.json",
        &findings
            .replace(
                "\"classification\": null",
                "\"classification\": \"normalizer-defect\"",
            )
            .replace("\"state\": \"untriaged\"", "\"state\": \"triaged\""),
    );
    let refused = scratch.run(&["discovery", "scaffold", &finding_id]);
    assert_eq!(code(&refused), 1);
    assert!(
        stderr(&refused).contains("not promotable"),
        "stderr: {}",
        stderr(&refused)
    );
}

#[test]
fn split_requires_something_to_separate() {
    let scratch = Scratch::new();
    let (finding_id, findings, campaigns) = populated_queue();
    scratch.write("findings.json", &findings);
    scratch.write("campaigns.json", &campaigns);

    // One witness: a split separates witnesses with different causes, so there is
    // nothing to separate.
    let out = scratch.run(&["discovery", "split", &finding_id]);
    assert_eq!(code(&out), 1, "stderr: {}", stderr(&out));
    assert!(
        stderr(&out).contains("nothing to separate"),
        "{}",
        stderr(&out)
    );

    let unknown = scratch.run(&["discovery", "split", "fnd-nowhere"]);
    assert_eq!(code(&unknown), 1);
}

#[test]
fn the_discovery_group_never_writes_into_the_registry() {
    // FR-036 as an observable property: every discovery command runs, and the registry
    // tree is byte-identical afterwards. The symlinked registry means a write would land
    // in the real one, so this also protects the repository from the test itself.
    let scratch = Scratch::new();
    let (finding_id, findings, campaigns) = populated_queue();
    scratch.write("findings.json", &findings);
    scratch.write("campaigns.json", &campaigns);

    let registry_dir = workspace_root().join("conformance").join("registry");
    let fingerprint = |dir: &Path| -> Vec<(PathBuf, u64, String)> {
        let mut out = Vec::new();
        let mut stack = vec![dir.to_path_buf()];
        while let Some(next) = stack.pop() {
            for entry in std::fs::read_dir(&next)
                .expect("read registry dir")
                .flatten()
            {
                let path = entry.path();
                let meta = entry.metadata().expect("metadata");
                if meta.is_dir() {
                    stack.push(path);
                } else {
                    let contents = std::fs::read_to_string(&path).unwrap_or_default();
                    out.push((path, meta.len(), contents));
                }
            }
        }
        out.sort();
        out
    };

    let before = fingerprint(&registry_dir);
    for args in [
        vec!["discovery", "check"],
        vec!["discovery", "scaffold", finding_id.as_str()],
        vec![
            "discovery",
            "triage",
            finding_id.as_str(),
            "--classification",
            "deacon-regression",
        ],
    ] {
        scratch.run(&args);
    }
    assert_eq!(
        before,
        fingerprint(&registry_dir),
        "no discovery command may alter the conformance registry (FR-036)"
    );
}
