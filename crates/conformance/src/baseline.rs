//! The frozen, mechanically enumerated pre-migration coverage baseline —
//! `conformance/migration/baseline.json` (data-model.md §1,
//! contracts/baseline-inventory.md).
//!
//! The baseline is the *subject* of the sentence "no coverage was lost". If it is
//! wrong, or editable by hand, the conservation claim is unfalsifiable (FR-045). So it
//! is machine-owned: `baseline generate` is its only writer, `baseline check` recomputes
//! and byte-compares, and drift is **V25**.
//!
//! # What counts as one unit (FR-049)
//!
//! A baseline unit is *the finest granularity for which the pre-migration system reports
//! an independent outcome* — each per-case result a comparison program emits — **plus
//! each test function that emits no per-case result**, counting as one unit. No
//! grouping, no splitting: an enumeration that merges two independently reported
//! outcomes, or splits one, is a defect.
//!
//! # Where each field comes from
//!
//! `id`, `program`, `category` and the *membership* of the inventory are **derived**:
//!
//! - corpus units come from the **production** discovery functions
//!   ([`crate::parity_corpus::discover_tier1_cases`] /
//!   [`crate::parity_corpus::discover_error_cases`]), never an independent directory
//!   walk — re-walking is exactly how 24 Tier-1 cases were once counted as 25
//!   (research D1);
//! - guard and internal-consistency units come from scanning the real
//!   `#[test]`/`#[tokio::test]` functions in the program's source, so adding a test
//!   function drifts the baseline and must be acknowledged;
//! - the declarative runner's units come from the registry's declarative cases, with
//!   their channels, fixtures and difference classes derived from the case record;
//! - the external manifest's units come from `fetch_realworld_corpus.py`'s pinned
//!   entries (research D8).
//!
//! `assertion`, and the per-case `channels` / `errorPath` / `fixtures` / `diffClasses`
//! for the scenario and guard programs, are **authored once, here, at freeze**. They
//! live in source rather than in the emitted file precisely because the emitted file
//! must regenerate byte-identically: a hand edit to `baseline.json` would be drift,
//! while a considered change here is a reviewable source diff. Per the freeze semantics
//! the `assertion` text is **immutable** after the freeze commit — it records what the
//! unit asserted *before* migration, and rewriting it post hoc would let the coverage
//! proof be satisfied by lowering the bar.
//!
//! Every scenario case id declared below is verified to occur as a string literal in its
//! program's source at generate time, and every live binary in
//! `fixtures/parity-corpus/registry.json` must be covered by one of the enumeration
//! strategies — an unrecognized program is a hard error, so a newly registered live
//! binary can never be silently omitted from the baseline.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::load::LoadError;
use crate::model::{CaseKind, OracleType, TestCase};
use crate::parity_corpus::{
    LiveKind, ParityCorpusError, ParityRegistry, discover_error_cases, discover_tier1_cases,
};

/// Schema version of the baseline file format.
pub const BASELINE_SCHEMA_VERSION: u32 = 1;

/// The freeze sentinel recorded when no freeze commit has been supplied. A committed
/// baseline still carrying it is not frozen, and is therefore **V25**: an unfrozen
/// baseline can be regenerated away at any time, which is exactly what makes the
/// conservation claim unfalsifiable (FR-045).
pub const UNFROZEN_REVISION: &str = "unfrozen";

/// Repo-relative location of the parity registry the enumeration reads.
const PARITY_REGISTRY_REL: &str = "fixtures/parity-corpus/registry.json";

/// Repo-relative location of the pinned external real-world corpus manifest (D8).
const REALWORLD_MANIFEST_REL: &str = "fixtures/parity-corpus/fetch_realworld_corpus.py";

/// Repo-relative directory holding the parity test binaries' sources.
const TESTS_DIR_REL: &str = "crates/deacon/tests";

/// The hermetic guard programs: they emit no per-case result, so each `#[test]` /
/// `#[tokio::test]` function is one unit (contracts/baseline-inventory.md rule 2).
/// Mirrors `parity_harness::registry::META_TEST_BINARIES`, which is the *checking*
/// side of the same fact.
const GUARD_PROGRAMS: &[&str] = &["parity_harness_faults", "parity_registry_check"];

// ---------------------------------------------------------------------------
// Record model (data-model.md §1)
// ---------------------------------------------------------------------------

/// How a baseline unit is carried today.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum UnitCategory {
    /// One per-case result emitted by a live (oracle-comparing) comparison program.
    LivePerCase,
    /// One `#[test]`/`#[tokio::test]` function of a program that emits no per-case
    /// result.
    HermeticGuard,
    /// One test function of an internal-consistency (deacon-vs-deacon) program.
    InternalConsistency,
    /// One entry of the pinned external real-world corpus manifest: inventoried, never
    /// executed, never counted as migrated (research D8).
    ExternalCorpusEntry,
}

impl UnitCategory {
    /// The wire spelling, for diagnostics that name the offending category.
    pub fn as_str(self) -> &'static str {
        match self {
            UnitCategory::LivePerCase => "live-per-case",
            UnitCategory::HermeticGuard => "hermetic-guard",
            UnitCategory::InternalConsistency => "internal-consistency",
            UnitCategory::ExternalCorpusEntry => "external-corpus-entry",
        }
    }
}

/// One baseline unit (data-model.md §1). Field order here is the emitted JSON field
/// order, matching contracts/baseline-inventory.md.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BaselineUnit {
    /// Derived, never authored: `<program>::<case-id>` for per-case programs,
    /// `<program>::<test-fn>` for guard programs, `realworld::<name>` for manifest
    /// entries. Unique across the file.
    pub id: String,
    /// The carrier that reports this unit today.
    pub program: String,
    /// How the unit is carried.
    pub category: UnitCategory,
    /// Whether running this unit requires a Docker daemon.
    pub docker_required: bool,
    /// What the unit asserts, in one sentence. Authored once at freeze; immutable.
    pub assertion: String,
    /// Observable channels the unit inspects today (resolvable `chan-*` ids).
    pub channels: Vec<String>,
    /// True when the unit's expectation is a rejection, a diagnostic, or a non-zero
    /// exit. Drives the FR-042 direction check.
    pub error_path: bool,
    /// Repo-relative fixture dirs consumed, or `inline:<fn>` for code-authored fixtures.
    pub fixtures: Vec<String>,
    /// Difference/result classes this unit can currently report, drawn from the
    /// enumerated vocabularies in research §1f.
    pub diff_classes: Vec<String>,
}

/// Provenance of an enumeration run — the inputs a reader needs to reproduce it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GeneratedFrom {
    /// Repo-relative path of the parity registry that was read.
    pub parity_registry: String,
    /// The production discovery functions that produced the corpus units.
    pub discovery: Vec<String>,
}

/// The committed baseline envelope (contracts/baseline-inventory.md).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BaselineFile {
    /// Schema version of the baseline format.
    pub schema_version: u32,
    /// The freeze commit. Participates in **V25**.
    pub revision: String,
    /// Provenance of the enumeration.
    pub generated_from: GeneratedFrom,
    /// Baseline units, sorted by `id`.
    pub records: Vec<BaselineUnit>,
}

impl BaselineFile {
    /// Look up a unit by id.
    pub fn unit(&self, id: &str) -> Option<&BaselineUnit> {
        self.records.iter().find(|u| u.id == id)
    }

    /// Count the units in one category.
    pub fn count(&self, category: UnitCategory) -> usize {
        self.records
            .iter()
            .filter(|u| u.category == category)
            .count()
    }

    /// The **executable** unit count — everything except the recorded-only external
    /// corpus entries. This is the denominator for the conservation accounting
    /// (contracts/baseline-inventory.md): the 33 manifest entries are inventoried but
    /// never counted as migrated (research D8).
    pub fn executable_count(&self) -> usize {
        self.records.len() - self.count(UnitCategory::ExternalCorpusEntry)
    }
}

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

/// Why an enumeration failed. Every variant names the specific input at fault —
/// enumeration never degrades to a partial inventory (constitution IV).
#[derive(Debug, thiserror::Error)]
pub enum BaselineError {
    #[error("could not read {path:?}: {cause}")]
    Io { path: PathBuf, cause: String },

    #[error("malformed parity registry {path:?}: {cause}")]
    ParityRegistry { path: PathBuf, cause: String },

    #[error(transparent)]
    Discovery(#[from] ParityCorpusError),

    #[error("could not load the conformance registry at {path:?}: {cause}")]
    Registry { path: PathBuf, cause: String },

    #[error(
        "live binary `{program}` is registered in {registry:?} but the baseline \
         enumerator has no strategy for it. Remedy: add its case ids (or its discovery \
         rule) to `crates/conformance/src/baseline.rs` — a live binary can never be \
         silently absent from the baseline."
    )]
    UnknownProgram { program: String, registry: PathBuf },

    #[error(
        "declared case id `{case}` for `{program}` does not occur as a string literal in \
         {source_file:?}. Remedy: the baseline's case ids must be sourced from the program \
         itself — fix the declared id or the program."
    )]
    CaseIdNotFound {
        program: String,
        case: String,
        source_file: PathBuf,
    },

    #[error(
        "declared declarative case `{case}` is not a declarative case in the conformance \
         registry. Remedy: the runner's baseline units are the declarative cases it drove \
         at the freeze commit."
    )]
    DeclarativeCaseMissing { case: String },

    #[error(
        "baseline unit `{unit}` has no authored metadata (assertion / channels / \
         errorPath / fixtures / diffClasses). Remedy: author it in \
         `crates/conformance/src/baseline.rs`; a unit with no recorded assertion cannot \
         be proven conserved."
    )]
    MissingAuthoredMetadata { unit: String },

    #[error(
        "found no `#[test]`/`#[tokio::test]` functions in {source_file:?}. Remedy: a guard \
         program with no test functions contributes no units, which is never intentional."
    )]
    NoTestFunctions { source_file: PathBuf },

    #[error(
        "found no pinned entries in the external corpus manifest {source_file:?}. Remedy: \
         the manifest's `CorpusEntry(name=…)` entries are the baseline's \
         `external-corpus-entry` units (research D8)."
    )]
    NoManifestEntries { source_file: PathBuf },

    #[error("duplicate baseline unit id `{id}` — enumeration must produce unique ids")]
    DuplicateUnit { id: String },
}

impl From<LoadError> for BaselineError {
    fn from(e: LoadError) -> Self {
        BaselineError::Registry {
            path: PathBuf::from("conformance/registry"),
            cause: e.to_string(),
        }
    }
}

// ---------------------------------------------------------------------------
// Authored metadata (frozen at the freeze commit)
// ---------------------------------------------------------------------------

/// The per-unit facts a machine cannot derive: what the unit asserts, which channels it
/// inspects, whether its expectation is a rejection, which fixtures it consumes, and
/// which difference classes it can report.
#[derive(Debug, Clone, Copy)]
struct Authored {
    assertion: &'static str,
    channels: &'static [&'static str],
    error_path: bool,
    fixtures: &'static [&'static str],
    diff_classes: &'static [&'static str],
}

// -- shared channel sets ----------------------------------------------------

/// A configuration differential: the exit class, then the parsed result document.
const CH_CONFIG: &[&str] = &["chan-exit-code", "chan-structured-output"];
/// A command executed inside a container.
const CH_EXEC: &[&str] = &["chan-exit-code", "chan-stdout", "chan-injected-process"];
/// A build: exit class, the JSON result document, and the produced image.
const CH_BUILD: &[&str] = &["chan-exit-code", "chan-structured-output", "chan-image"];
/// Inspected container/compose state.
const CH_STATE: &[&str] = &["chan-container-state", "chan-process-graph"];
/// Nothing observable through a devcontainer channel — the unit inspects harness or
/// repository structure.
const CH_NONE: &[&str] = &[];

// -- shared difference-class sets (research §1f) ----------------------------

/// A normalized config differential reports the three `DiffKind`s plus the process
/// causes that can abort it. Matches contracts/baseline-inventory.md's record example.
const DIFF_CONFIG: &[&str] = &[
    "ref-only",
    "value",
    "deacon-only",
    "oracle-failure",
    "normalization",
];
/// The error corpus compares an accept/reject *decision* against a recorded
/// expectation; a mismatch is reported as a divergence.
const DIFF_DECISION: &[&str] = &["divergence"];
/// A Docker-backed runtime differential.
const DIFF_RUNTIME: &[&str] = &[
    "ref-only",
    "value",
    "deacon-only",
    "divergence",
    "oracle-failure",
    "docker-missing",
];
/// A unit that reports a plain value disagreement between two deacon invocations.
const DIFF_VALUE: &[&str] = &["value"];
/// A unit that reports no difference class (it asserts structure, not a comparison).
const DIFF_NONE: &[&str] = &[];

// -- shared fixture sets ----------------------------------------------------

const FX_NONE: &[&str] = &[];
const FX_STUB: &[&str] = &["inline:write_stub"];

/// Per-program, per-case authored metadata for the scenario comparison programs.
/// Case ids are verified against each program's source at generate time.
///
/// An `inline:` fixture id names the code-authored workspace a unit consumes. Where one
/// test function authors SEVERAL distinct workspaces (`parity_exec`, `parity_build`), the
/// id is qualified per case (`inline:<program>#<case>`) — a per-function id would make two
/// units claim the same fixture, and a fixture correspondence that is not one-to-one is a
/// silent merge (V22).
const SCENARIO_UNITS: &[(&str, &[(&str, Authored)])] = &[
    (
        "parity_read_configuration",
        &[
            (
                "basic",
                Authored {
                    assertion: "deacon and the pinned reference resolve the same configuration document for the basic devcontainer.jsonc fixture",
                    channels: CH_CONFIG,
                    error_path: false,
                    fixtures: &["fixtures/config/basic"],
                    diff_classes: DIFF_CONFIG,
                },
            ),
            (
                "with-variables",
                Authored {
                    assertion: "deacon and the pinned reference resolve the same configuration document, including variable substitution, for the with-variables devcontainer.jsonc fixture",
                    channels: CH_CONFIG,
                    error_path: false,
                    fixtures: &["fixtures/config/with-variables"],
                    diff_classes: DIFF_CONFIG,
                },
            ),
        ],
    ),
    (
        "parity_exec",
        &[
            (
                "working-directory",
                Authored {
                    assertion: "deacon `exec` and the reference `exec` run the command in the same container working directory",
                    channels: CH_EXEC,
                    error_path: false,
                    fixtures: &["inline:parity_exec#working-directory"],
                    diff_classes: DIFF_RUNTIME,
                },
            ),
            (
                "user",
                Authored {
                    assertion: "deacon `exec` and the reference `exec` run the command as the same container user",
                    channels: CH_EXEC,
                    error_path: false,
                    fixtures: &["inline:parity_exec#user"],
                    diff_classes: DIFF_RUNTIME,
                },
            ),
            (
                "tty",
                Authored {
                    assertion: "deacon `exec` and the reference `exec` present the same TTY allocation to the executed command",
                    channels: CH_EXEC,
                    error_path: false,
                    fixtures: &["inline:parity_exec#tty"],
                    diff_classes: DIFF_RUNTIME,
                },
            ),
            (
                "env-propagation",
                Authored {
                    assertion: "deacon's `--remote-env FOO=BAR` and the reference's `--env FOO=BAR` propagate the same environment variable into the executed command",
                    channels: CH_EXEC,
                    error_path: false,
                    fixtures: &["inline:parity_exec#env-propagation"],
                    diff_classes: DIFF_RUNTIME,
                },
            ),
        ],
    ),
    (
        "parity_build",
        &[
            (
                "creates-discoverable-image",
                Authored {
                    assertion: "both CLIs' `build` create an image discoverable by the same unique parity.token label",
                    channels: CH_BUILD,
                    error_path: false,
                    fixtures: &["inline:parity_build#creates-discoverable-image"],
                    diff_classes: DIFF_RUNTIME,
                },
            ),
            (
                "with-build-args",
                Authored {
                    assertion: "both CLIs' `build` honor build args, producing images that carry the build-arg-derived label",
                    channels: CH_BUILD,
                    error_path: false,
                    fixtures: &["inline:parity_build#with-build-args"],
                    diff_classes: DIFF_RUNTIME,
                },
            ),
            (
                "push-json-output",
                Authored {
                    assertion: "deacon's `build --push --output-format json` emits the documented JSON result shape whether the push succeeds or fails",
                    channels: CH_BUILD,
                    error_path: false,
                    fixtures: &["inline:parity_build#push-json-output"],
                    diff_classes: DIFF_RUNTIME,
                },
            ),
            (
                "output-json-format",
                Authored {
                    assertion: "deacon's `build --output --output-format json` emits the documented JSON result shape whether the build succeeds or fails",
                    channels: CH_BUILD,
                    error_path: false,
                    fixtures: &["inline:parity_build#output-json-format"],
                    diff_classes: DIFF_RUNTIME,
                },
            ),
            (
                "buildkit-only-features",
                Authored {
                    assertion: "deacon's `build` fails gracefully with a diagnostic when BuildKit-only flags are requested without BuildKit, rather than producing a partial image",
                    channels: CH_BUILD,
                    error_path: true,
                    fixtures: &["inline:parity_build#buildkit-only-features"],
                    diff_classes: DIFF_RUNTIME,
                },
            ),
            (
                "image-reference",
                Authored {
                    assertion: "deacon's `build` from an image reference applies features and custom tags, emitting valid JSON with the custom tag and creating a labeled image",
                    channels: CH_BUILD,
                    error_path: false,
                    fixtures: &["inline:parity_build#image-reference"],
                    diff_classes: DIFF_RUNTIME,
                },
            ),
        ],
    ),
    (
        "parity_up_exec",
        &[(
            "traditional",
            Authored {
                assertion: "after `up` on a traditional (non-compose) workspace, both CLIs' `exec` reach that workspace's own container and observe the same environment marker",
                channels: &[
                    "chan-exit-code",
                    "chan-stdout",
                    "chan-container-state",
                    "chan-injected-process",
                ],
                error_path: false,
                fixtures: &["inline:parity_up_and_exec_traditional"],
                diff_classes: DIFF_RUNTIME,
            },
        )],
    ),
    (
        "parity_observable_state",
        &[
            (
                "lockfile-manifest-digest",
                Authored {
                    assertion: "a deacon-generated feature lockfile carries the OCI manifest digest (not the layer digest) and is consumable by the reference CLI's `features resolve-dependencies`",
                    channels: &[
                        "chan-exit-code",
                        "chan-structured-output",
                        "chan-file-content",
                    ],
                    error_path: false,
                    fixtures: &["inline:parity_lockfile_manifest_digest_resolves_dependencies"],
                    diff_classes: DIFF_RUNTIME,
                },
            ),
            (
                "compose-config-mounts",
                Authored {
                    assertion: "`devcontainer.json` mounts are applied by BOTH CLIs on the compose path",
                    channels: CH_STATE,
                    error_path: false,
                    fixtures: &["inline:parity_compose_config_mounts_applied_both_clis"],
                    diff_classes: DIFF_RUNTIME,
                },
            ),
            (
                "compose-project-name-isolated",
                Authored {
                    assertion: "deacon's compose project name is isolated from the reference CLI's, so the two CLIs' compose projects never collide",
                    channels: CH_STATE,
                    error_path: false,
                    fixtures: &["inline:parity_compose_project_name_isolated_from_reference"],
                    diff_classes: DIFF_RUNTIME,
                },
            ),
            (
                "container-and-image-labels",
                Authored {
                    assertion: "deacon stamps the reference-compatible discovery labels on its container and image while keeping its own isolation labels distinct",
                    channels: &["chan-container-state", "chan-image"],
                    error_path: false,
                    fixtures: &["inline:parity_container_and_image_labels_isolated"],
                    diff_classes: DIFF_RUNTIME,
                },
            ),
            (
                "rendered-compose-state",
                Authored {
                    assertion: "the primary compose service's rendered image, volumes, environment, and labels compare equal between the two CLIs on equivalent input",
                    channels: CH_STATE,
                    error_path: false,
                    fixtures: &["inline:parity_rendered_compose_state_comparable"],
                    diff_classes: DIFF_RUNTIME,
                },
            ),
            (
                "handoff-no-reuse",
                Authored {
                    assertion: "a deacon `up` after a reference `up` on the same workspace provisions its own distinct container instead of silently attaching to the reference's",
                    channels: &["chan-container-state", "chan-temporal"],
                    error_path: false,
                    fixtures: &["inline:parity_handoff_no_cross_cli_container_reuse"],
                    diff_classes: DIFF_RUNTIME,
                },
            ),
            (
                "merged-config-vs-runtime",
                Authored {
                    assertion: "for each CLI independently, `read-configuration --include-merged-configuration` agrees with what `docker inspect` shows on the running container",
                    channels: &["chan-structured-output", "chan-container-state"],
                    error_path: false,
                    fixtures: &["inline:parity_merged_config_matches_runtime_truth"],
                    diff_classes: DIFF_RUNTIME,
                },
            ),
        ],
    ),
    (
        "parity_state_diff",
        &[
            (
                "single-container-parity",
                Authored {
                    assertion: "deacon and the reference produce equivalent inspected container state for a single-container workspace",
                    channels: CH_STATE,
                    error_path: false,
                    fixtures: &["inline:state_diff_single_container_parity"],
                    diff_classes: DIFF_RUNTIME,
                },
            ),
            (
                "compose-parity-with-feature-mount-gap",
                Authored {
                    assertion: "deacon's compose path folds resolved feature mounts into the container exactly as the reference does",
                    channels: CH_STATE,
                    error_path: false,
                    fixtures: &["inline:state_diff_compose_parity_with_feature_mount_gap"],
                    diff_classes: DIFF_RUNTIME,
                },
            ),
            (
                "intra-deacon-single-vs-compose",
                Authored {
                    assertion: "deacon's own compose container state matches its single-container state for the same logical configuration, with no reference side involved",
                    channels: CH_STATE,
                    error_path: false,
                    fixtures: &["inline:state_diff_intra_deacon_single_vs_compose"],
                    diff_classes: DIFF_RUNTIME,
                },
            ),
            (
                "default-workspace-mount-target-parity",
                Authored {
                    assertion: "the characterized divergence between deacon's and the reference's default workspace-mount target remains exactly as recorded, so a change forces a deliberate decision",
                    channels: CH_STATE,
                    error_path: false,
                    fixtures: &["inline:state_diff_default_workspace_mount_target_parity"],
                    diff_classes: DIFF_RUNTIME,
                },
            ),
            (
                "dockerfile-build-and-nonroot-user",
                Authored {
                    assertion: "a Dockerfile-built workspace with a non-root containerUser/remoteUser yields equivalent container state in both CLIs",
                    channels: CH_STATE,
                    error_path: false,
                    fixtures: &["inline:state_diff_dockerfile_build_and_nonroot_user"],
                    diff_classes: DIFF_RUNTIME,
                },
            ),
            (
                "appport-published-ports",
                Authored {
                    assertion: "`appPort` publishes the same container ports in both CLIs",
                    channels: CH_STATE,
                    error_path: false,
                    fixtures: &["inline:state_diff_appport_published_ports"],
                    diff_classes: DIFF_RUNTIME,
                },
            ),
            (
                "mount-variety-readonly-and-tmpfs",
                Authored {
                    assertion: "read-only bind mounts and tmpfs mounts are applied identically by both CLIs",
                    channels: CH_STATE,
                    error_path: false,
                    fixtures: &["inline:state_diff_mount_variety_readonly_and_tmpfs"],
                    diff_classes: DIFF_RUNTIME,
                },
            ),
            (
                "compose-sidecar-and-named-volume",
                Authored {
                    assertion: "a compose sidecar service and a named volume are provisioned equivalently by both CLIs",
                    channels: CH_STATE,
                    error_path: false,
                    fixtures: &["inline:state_diff_compose_sidecar_and_named_volume"],
                    diff_classes: DIFF_RUNTIME,
                },
            ),
        ],
    ),
];

/// Authored metadata for the hermetic guard and internal-consistency test functions.
/// The *membership* of these lists is derived by scanning each program's source; this
/// table supplies what the scan cannot know.
const FUNCTION_UNITS: &[(&str, &[(&str, Authored)])] = &[
    (
        "parity_harness_faults",
        &[
            (
                "a_wrong_version_stub_reports_mismatch",
                Authored {
                    assertion: "an oracle whose reported version differs from the pin is reported as a version mismatch, never silently accepted",
                    channels: CH_NONE,
                    error_path: true,
                    fixtures: FX_STUB,
                    diff_classes: &["oracle-version-mismatch"],
                },
            ),
            (
                "b_nonexistent_override_reports_missing",
                Authored {
                    assertion: "an oracle override pointing at a nonexistent binary is reported as a missing oracle, never a skip",
                    channels: CH_NONE,
                    error_path: true,
                    fixtures: FX_STUB,
                    diff_classes: &["oracle-missing"],
                },
            ),
            (
                "c_failing_docker_stub_reports_docker_missing",
                Authored {
                    assertion: "a failing Docker probe is reported as docker-missing rather than allowing the check to pass",
                    channels: CH_NONE,
                    error_path: true,
                    fixtures: FX_STUB,
                    diff_classes: &["docker-missing"],
                },
            ),
            (
                "d_crash_stub_is_oracle_failure",
                Authored {
                    assertion: "an oracle that crashes is reported as an oracle failure naming its preserved stderr",
                    channels: CH_NONE,
                    error_path: true,
                    fixtures: FX_STUB,
                    diff_classes: &["oracle-failure"],
                },
            ),
            (
                "e_garbage_output_is_malformed",
                Authored {
                    assertion: "non-JSON output where structured output was required is reported as malformed output",
                    channels: CH_NONE,
                    error_path: true,
                    fixtures: FX_STUB,
                    diff_classes: &["malformed-output"],
                },
            ),
            (
                "f_hang_stub_times_out_with_partial_output",
                Authored {
                    assertion: "an oracle that hangs is terminated at its bound and reported as a timeout with its partial output preserved",
                    channels: CH_NONE,
                    error_path: true,
                    fixtures: FX_STUB,
                    diff_classes: &["oracle-timeout"],
                },
            ),
            (
                "g_injected_difference_is_unwaived_divergence",
                Authored {
                    assertion: "an injected difference with no matching waiver is reported as a divergence, never absorbed",
                    channels: CH_NONE,
                    error_path: true,
                    fixtures: FX_NONE,
                    diff_classes: &["divergence"],
                },
            ),
            (
                "h_matching_waiver_yields_pass_waived",
                Authored {
                    assertion: "a difference matching an active waiver is reported as a waived pass naming the waiver record",
                    channels: CH_NONE,
                    error_path: false,
                    fixtures: FX_NONE,
                    diff_classes: &["pass-waived"],
                },
            ),
            (
                "i_kept_waiver_without_difference_is_stale",
                Authored {
                    assertion: "a waiver whose characterized difference no longer reproduces is reported as stale rather than silently retained",
                    channels: CH_NONE,
                    error_path: true,
                    fixtures: FX_NONE,
                    diff_classes: &["stale-waiver"],
                },
            ),
            (
                "k_reference_only_difference_is_classified_as_ref_only",
                Authored {
                    assertion: "a key the reference emits and deacon does not is classified as its own `ref-only` difference class and named",
                    channels: CH_NONE,
                    error_path: true,
                    fixtures: FX_NONE,
                    diff_classes: &["ref-only"],
                },
            ),
            (
                "l_deacon_only_difference_is_classified_and_not_deprioritized",
                Authored {
                    assertion: "a key deacon emits and the reference does not is reported as its own `deacon-only` difference class and is not ranked below the others as noise",
                    channels: CH_NONE,
                    error_path: true,
                    fixtures: FX_NONE,
                    diff_classes: &["deacon-only"],
                },
            ),
            (
                "m_value_difference_is_classified_with_both_sides",
                Authored {
                    assertion: "a shared key with differing values is classified as a `value` difference carrying both sides",
                    channels: CH_NONE,
                    error_path: true,
                    fixtures: FX_NONE,
                    diff_classes: &["value"],
                },
            ),
            (
                "n_accept_vs_reject_difference_preserves_direction",
                Authored {
                    assertion: "an accept-versus-reject difference keeps its direction: a waiver characterizing one direction never waives the inverse, and an agreement expectation waives neither",
                    channels: CH_NONE,
                    error_path: true,
                    fixtures: FX_NONE,
                    diff_classes: &["deacon-stricter", "reference-stricter"],
                },
            ),
            (
                "o_allowed_difference_is_distinct_from_agree_and_names_its_backing_id",
                Authored {
                    assertion: "a divergence covered by a scoped tolerance verdicts `allowed-difference` naming its backing record, an uncovered path still verdicts `diverge`, and an unconsumed tolerance is reported stale",
                    channels: CH_NONE,
                    error_path: true,
                    fixtures: FX_NONE,
                    diff_classes: &["allowed-difference"],
                },
            ),
            (
                "p_no_reference_for_platform_is_its_own_outcome",
                Authored {
                    assertion: "a missing snapshot for the current platform verdicts `no-reference-for-platform`, distinct from both `agree` and `stale`, so an absent comparison is never reported as a pass",
                    channels: CH_NONE,
                    error_path: true,
                    fixtures: FX_NONE,
                    diff_classes: &["no-reference-for-platform"],
                },
            ),
            (
                "q_stale_snapshot_is_reported_naming_the_drifted_field",
                Authored {
                    assertion: "a snapshot whose evidence-determining provenance drifted is reported stale naming the first drifted field, while a drifted host tool version or capture time is not staleness",
                    channels: CH_NONE,
                    error_path: true,
                    fixtures: FX_NONE,
                    diff_classes: &["stale"],
                },
            ),
            (
                "j_normalization_failure_has_no_raw_fallback",
                Authored {
                    assertion: "a normalization failure fails the run; the harness never falls back to raw comparison",
                    channels: CH_NONE,
                    error_path: true,
                    fixtures: FX_NONE,
                    diff_classes: &["normalization"],
                },
            ),
        ],
    ),
    (
        "parity_registry_check",
        &[
            (
                "registry_matches_test_files_both_directions",
                Authored {
                    assertion: "every registered live and internal-consistency binary has a source file, and every parity source file is registered or a recognized meta-test binary",
                    channels: CH_NONE,
                    error_path: false,
                    fixtures: FX_NONE,
                    diff_classes: DIFF_NONE,
                },
            ),
            (
                "nextest_parity_profile_selects_exactly_live_binaries",
                Authored {
                    assertion: "the parity nextest profile selects exactly the registered live binaries and no other profile selects any of them",
                    channels: CH_NONE,
                    error_path: false,
                    fixtures: FX_NONE,
                    diff_classes: DIFF_NONE,
                },
            ),
            (
                "corpora_meet_registered_minimums",
                Authored {
                    assertion: "each registered corpus discovers at least its declared minimum number of cases",
                    channels: CH_NONE,
                    error_path: false,
                    fixtures: FX_NONE,
                    diff_classes: DIFF_NONE,
                },
            ),
            (
                "no_parity_source_uses_ignore_or_legacy_skip_idioms",
                Authored {
                    assertion: "no parity source uses an ignore attribute or a legacy skip idiom, so a lane can never go green by not running",
                    channels: CH_NONE,
                    error_path: false,
                    fixtures: FX_NONE,
                    diff_classes: DIFF_NONE,
                },
            ),
            (
                "waivers_live_in_conformance_registry_not_legacy_locations",
                Authored {
                    assertion: "characterized exceptions resolve only from the conformance registry, never from a reintroduced legacy location",
                    channels: CH_NONE,
                    error_path: false,
                    fixtures: FX_NONE,
                    diff_classes: DIFF_NONE,
                },
            ),
            (
                "no_surface_globs_a_removed_path",
                Authored {
                    assertion: "no Makefile, workflow or script reads a repository path the migration deleted, and the surviving image pre-pull still resolves to at least one fixture — a glob that matches nothing is not an error and would silently drop the protection it provides",
                    channels: CH_NONE,
                    error_path: false,
                    fixtures: FX_NONE,
                    diff_classes: DIFF_NONE,
                },
            ),
            (
                "no_surface_references_a_removed_binary",
                Authored {
                    assertion: "no Makefile, workflow, nextest, parity-registry or documentation reference points at a surface the migration removed; a doc may name one only while saying it is gone",
                    channels: CH_NONE,
                    error_path: false,
                    fixtures: FX_NONE,
                    diff_classes: DIFF_NONE,
                },
            ),
            (
                "the_surviving_set_is_mutually_consistent",
                Authored {
                    assertion: "after the cut-over the registry, the nextest profiles and the actual test sources agree on exactly the surviving live set, and no corpus remains declared",
                    channels: CH_NONE,
                    error_path: false,
                    fixtures: FX_NONE,
                    diff_classes: DIFF_NONE,
                },
            ),
            (
                "the_shipped_cli_gained_no_subcommand_from_this_feature",
                Authored {
                    assertion: "`deacon --help` exposes no subcommand introduced by the conformance/migration tooling, so contributor machinery never reaches the shipped consumer surface",
                    channels: CH_NONE,
                    error_path: false,
                    fixtures: FX_NONE,
                    diff_classes: DIFF_NONE,
                },
            ),
            (
                "tests_dir_anchor_is_valid",
                Authored {
                    assertion: "the tests-directory anchor the registry check walks actually resolves, so a silent no-op scan is impossible",
                    channels: CH_NONE,
                    error_path: false,
                    fixtures: FX_NONE,
                    diff_classes: DIFF_NONE,
                },
            ),
        ],
    ),
    (
        "consistency_env_probe_flag",
        &[
            (
                "exec_honors_default_user_env_probe_login_shell",
                Authored {
                    assertion: "`exec` honors the default login-interactive-shell user environment probe mode",
                    channels: &["chan-injected-process"],
                    error_path: false,
                    fixtures: FX_NONE,
                    diff_classes: DIFF_VALUE,
                },
            ),
            (
                "up_shared_probe_helper_uses_login_shell",
                Authored {
                    assertion: "`up`'s shared environment-probe helper uses the same login shell as `exec`, so the two subcommands cannot drift apart",
                    channels: &["chan-injected-process"],
                    error_path: false,
                    fixtures: FX_NONE,
                    diff_classes: DIFF_VALUE,
                },
            ),
        ],
    ),
    (
        "consistency_remote_env_flags",
        &[
            (
                "remote_env_validation_message_matches_for_up_and_exec",
                Authored {
                    assertion: "`up` and `exec` reject a malformed remote-env value with the same diagnostic",
                    channels: &["chan-exit-code", "chan-stderr"],
                    error_path: true,
                    fixtures: FX_NONE,
                    diff_classes: DIFF_VALUE,
                },
            ),
            (
                "remote_env_accepts_empty_values_for_up_and_exec",
                Authored {
                    assertion: "`up` and `exec` both accept an empty remote-env value",
                    channels: &["chan-exit-code", "chan-stderr"],
                    error_path: false,
                    fixtures: FX_NONE,
                    diff_classes: DIFF_VALUE,
                },
            ),
        ],
    ),
];

/// The error-corpus cases' authored assertion and rejection direction. The direction
/// mirrors each case's `corpus_case` waiver expectation in the conformance registry
/// (`both-accept` is the only non-error path); it is recorded here rather than derived
/// so the frozen baseline does not shift when a waiver is later re-homed.
const ERROR_CORPUS_UNITS: &[(&str, &str, bool)] = &[
    (
        "bad-config-path",
        "both CLIs reject a --config path that does not exist",
        true,
    ),
    (
        "duplicate-keys",
        "both CLIs accept a configuration containing duplicate JSON keys and resolve it to the same document",
        false,
    ),
    (
        "extends-cycle",
        "deacon rejects a cyclic extends chain where the reference leniently accepts it",
        true,
    ),
    (
        "extends-missing",
        "deacon rejects an extends target that does not exist where the reference leniently accepts it",
        true,
    ),
    (
        "malformed-json",
        "deacon rejects malformed JSONC where the reference leniently accepts it",
        true,
    ),
    (
        "missing-config",
        "both CLIs reject a workspace that has no devcontainer configuration",
        true,
    ),
    (
        "unknown-field-preserved",
        "both CLIs accept an unknown top-level field and preserve it in the resolved configuration",
        false,
    ),
    (
        "wrong-type-features",
        "deacon rejects a wrongly-typed features value where the reference leniently accepts it",
        true,
    ),
    (
        "wrong-type-forwardports",
        "deacon rejects a wrongly-typed forwardPorts value where the reference leniently accepts it",
        true,
    ),
];

/// The declarative cases the conformance runner drove at the freeze commit, with their
/// authored assertions. Their channels, fixtures, and difference classes are derived
/// from the case records themselves. Frozen: later phases add declarative cases as the
/// *destination* of migrated units, and a migrated case is not a pre-migration baseline
/// unit.
const RUNNER_UNITS: &[(&str, &str)] = &[
    (
        "case-readconfig-parity-exit",
        "deacon and the pinned reference both exit successfully for a well-formed configuration",
    ),
    (
        "case-readconfig-snapshot",
        "`read-configuration`'s normalized observables match the committed, provenance-checked reference snapshot for this platform",
    ),
    (
        "case-readconfig-unknown-field-echo",
        "an unknown top-level key round-trips through `read-configuration` instead of being dropped",
    ),
    (
        "case-readconfig-workspace-file-present",
        "`read-configuration` leaves the workspace's devcontainer configuration file in place",
    ),
    (
        "case-up-docker-channels",
        "`up` stamps deacon's image label, injects the configured container environment, and leaves a running container with the expected process graph",
    ),
    (
        "case-up-idempotent",
        "a second `up` on an already-provisioned workspace reuses the running container rather than recreating it",
    ),
];

/// The authored assertion for one external real-world corpus entry (research D8). These
/// entries are a coverage *source*: they were selected as representative third-party
/// workspaces, but the manifest has never run in CI and asserts nothing today.
/// Recording them preserves that selection; they never count as migrated.
fn realworld_assertion(name: &str) -> String {
    format!(
        "the pinned third-party workspace `{name}` is recorded as a representative \
         real-world configuration source; the manifest fetches it on demand and asserts \
         nothing in CI"
    )
}

// ---------------------------------------------------------------------------
// Enumeration
// ---------------------------------------------------------------------------

/// Enumerate the pre-migration baseline from the repository tree at `repo_root`,
/// recording `revision` as the freeze commit.
///
/// Deterministic: the result is sorted by `id` and contains no timestamps, absolute
/// paths, or machine-specific values.
pub fn generate_baseline(repo_root: &Path, revision: &str) -> Result<BaselineFile, BaselineError> {
    let parity_path = repo_root.join(PARITY_REGISTRY_REL);
    let parity_raw = read_to_string(&parity_path)?;
    let parity =
        ParityRegistry::parse(&parity_raw).map_err(|cause| BaselineError::ParityRegistry {
            path: parity_path.clone(),
            cause,
        })?;

    let tests_dir = repo_root.join(TESTS_DIR_REL);
    let registry_dir = repo_root.join("conformance").join("registry");
    let registry =
        crate::load::Registry::load(&registry_dir).map_err(|e| BaselineError::Registry {
            path: registry_dir.clone(),
            cause: e.to_string(),
        })?;

    let mut records: Vec<BaselineUnit> = Vec::new();

    // 1. Live comparison programs — every registered live binary must be recognized.
    for binary in &parity.live_binaries {
        match binary.kind {
            LiveKind::Corpus => {
                let corpus_id = binary.corpus.as_deref().unwrap_or_default();
                let corpus =
                    parity
                        .corpus(corpus_id)
                        .ok_or_else(|| BaselineError::ParityRegistry {
                            path: parity_path.clone(),
                            cause: format!(
                                "corpus binary `{}` references undeclared corpus `{corpus_id}`",
                                binary.name
                            ),
                        })?;
                let corpus_root = repo_root.join(&corpus.path);
                records.extend(corpus_units(
                    &binary.name,
                    corpus_id,
                    &corpus.path,
                    &corpus_root,
                    binary.docker_required,
                )?);
            }
            LiveKind::Scenario if binary.name == "parity_conformance_runner" => {
                records.extend(runner_units(&binary.name, &registry.cases)?);
            }
            LiveKind::Scenario => {
                records.extend(scenario_units(
                    &binary.name,
                    &tests_dir,
                    binary.docker_required,
                    &parity_path,
                )?);
            }
        }
    }

    // 2. Hermetic guard programs — one unit per test function.
    for program in GUARD_PROGRAMS {
        records.extend(function_units(
            program,
            &tests_dir,
            UnitCategory::HermeticGuard,
        )?);
    }

    // 3. Internal-consistency programs — one unit per test function.
    for program in &parity.internal_consistency_binaries {
        records.extend(function_units(
            program,
            &tests_dir,
            UnitCategory::InternalConsistency,
        )?);
    }

    // 4. The pinned external real-world corpus manifest (research D8).
    records.extend(realworld_units(&repo_root.join(REALWORLD_MANIFEST_REL))?);

    records.sort_by(|a, b| a.id.cmp(&b.id));
    let mut seen: BTreeSet<&str> = BTreeSet::new();
    for record in &records {
        if !seen.insert(record.id.as_str()) {
            return Err(BaselineError::DuplicateUnit {
                id: record.id.clone(),
            });
        }
    }

    Ok(BaselineFile {
        schema_version: BASELINE_SCHEMA_VERSION,
        revision: revision.to_string(),
        generated_from: GeneratedFrom {
            parity_registry: PARITY_REGISTRY_REL.to_string(),
            discovery: vec![
                "discover_tier1_cases".to_string(),
                "discover_error_cases".to_string(),
            ],
        },
        records,
    })
}

/// Units for a corpus-driving binary: one per directory the **production** discovery
/// function selects (research D1 — never a re-implemented walk).
fn corpus_units(
    program: &str,
    corpus_id: &str,
    corpus_rel: &str,
    corpus_root: &Path,
    docker_required: bool,
) -> Result<Vec<BaselineUnit>, BaselineError> {
    let dirs = match corpus_id {
        "errors" => discover_error_cases(corpus_root)?,
        _ => discover_tier1_cases(corpus_root)?,
    };

    let mut out = Vec::new();
    for dir in dirs {
        let case = dir
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or_default()
            .to_string();
        let fixture = format!("{corpus_rel}/{case}");

        let (assertion, error_path, channels, diff_classes) = if corpus_id == "errors" {
            let (_, assertion, error_path) = ERROR_CORPUS_UNITS
                .iter()
                .find(|(name, _, _)| *name == case)
                .ok_or_else(|| BaselineError::MissingAuthoredMetadata {
                    unit: format!("{program}::{case}"),
                })?;
            (
                (*assertion).to_string(),
                *error_path,
                CH_CONFIG,
                DIFF_DECISION,
            )
        } else if program == "parity_corpus_merged" {
            // The merged-mode variant of the same corpus directory. `extends-child` is
            // the one case whose recorded expectation is a reference *rejection* (the
            // `reference-stricter` characterization); in plain mode the two CLIs agree.
            (
                format!(
                    "deacon and the pinned reference resolve the same merged configuration \
                     (--include-merged-configuration) for the {case} workspace"
                ),
                case == "extends-child",
                CH_CONFIG,
                DIFF_CONFIG,
            )
        } else {
            (
                format!(
                    "deacon and the pinned reference resolve the same configuration for the \
                     {case} workspace"
                ),
                false,
                CH_CONFIG,
                DIFF_CONFIG,
            )
        };

        out.push(BaselineUnit {
            id: format!("{program}::{case}"),
            program: program.to_string(),
            category: UnitCategory::LivePerCase,
            docker_required,
            assertion,
            channels: to_strings(channels),
            error_path,
            fixtures: vec![fixture],
            diff_classes: to_strings(diff_classes),
        });
    }
    Ok(out)
}

/// Units for a scenario comparison program: one per declared case id, each verified to
/// occur as a string literal in the program's own source.
fn scenario_units(
    program: &str,
    tests_dir: &Path,
    docker_required: bool,
    parity_path: &Path,
) -> Result<Vec<BaselineUnit>, BaselineError> {
    let cases = SCENARIO_UNITS
        .iter()
        .find(|(name, _)| *name == program)
        .map(|(_, cases)| *cases)
        .ok_or_else(|| BaselineError::UnknownProgram {
            program: program.to_string(),
            registry: parity_path.to_path_buf(),
        })?;

    let source_path = tests_dir.join(format!("{program}.rs"));
    let source = read_to_string(&source_path)?;

    let mut out = Vec::new();
    for (case, authored) in cases {
        if !source.contains(&format!("\"{case}\"")) {
            return Err(BaselineError::CaseIdNotFound {
                program: program.to_string(),
                case: (*case).to_string(),
                source_file: source_path.clone(),
            });
        }
        out.push(BaselineUnit {
            id: format!("{program}::{case}"),
            program: program.to_string(),
            category: UnitCategory::LivePerCase,
            docker_required,
            assertion: authored.assertion.to_string(),
            channels: to_strings(authored.channels),
            error_path: authored.error_path,
            fixtures: to_strings(authored.fixtures),
            diff_classes: to_strings(authored.diff_classes),
        });
    }
    Ok(out)
}

/// Units for the declarative conformance runner: one per declarative case it drove at
/// the freeze commit. Channels, fixtures and difference classes come from the case
/// record itself; only the assertion sentence is authored.
fn runner_units(program: &str, cases: &[TestCase]) -> Result<Vec<BaselineUnit>, BaselineError> {
    let declarative: BTreeMap<&str, &TestCase> = cases
        .iter()
        .filter(|c| matches!(c.classify(), Ok(CaseKind::Declarative)))
        .map(|c| (c.id.as_str(), c))
        .collect();

    let mut out = Vec::new();
    for (case_id, assertion) in RUNNER_UNITS {
        let case = declarative.get(case_id).copied().ok_or_else(|| {
            BaselineError::DeclarativeCaseMissing {
                case: (*case_id).to_string(),
            }
        })?;

        // Channels: the declared expectation channels, de-duplicated and ordered.
        let channels: BTreeSet<String> = case.expected.iter().map(|e| e.channel.clone()).collect();
        // Fixtures: each operation's fixture ids, resolved to their committed directory.
        let fixtures: BTreeSet<String> = case
            .operations
            .iter()
            .flat_map(|op| op.fixtures.iter())
            .map(|id| format!("conformance/fixtures/{id}"))
            .collect();

        out.push(BaselineUnit {
            id: format!("{program}::{case_id}"),
            program: program.to_string(),
            category: UnitCategory::LivePerCase,
            // The runner's registry entry is `docker_required: false` because most of
            // its cases are hermetic; docker-ness is a per-CASE fact here, carried by
            // the case's nextest resource group. Recording the binary-level flag would
            // misreport the Docker-backed cases.
            docker_required: case.resource_group.is_some(),
            assertion: (*assertion).to_string(),
            channels: channels.into_iter().collect(),
            error_path: false,
            fixtures: fixtures.into_iter().collect(),
            diff_classes: to_strings(runner_diff_classes(case.oracle_type)),
        });
    }
    Ok(out)
}

/// The declarative verdict vocabulary (`evidence::Outcome`) a case can currently
/// produce, narrowed by its oracle type: only a snapshot oracle can be stale or lack a
/// reference for the current platform.
fn runner_diff_classes(oracle_type: Option<OracleType>) -> &'static [&'static str] {
    match oracle_type {
        Some(OracleType::Snapshot) => &[
            "agree",
            "diverge",
            "allowed-difference",
            "no-reference-for-platform",
            "stale",
            "error",
        ],
        _ => &["agree", "diverge", "allowed-difference", "error"],
    }
}

/// Units for a program that emits no per-case result: one per `#[test]` /
/// `#[tokio::test]` function found in its source (contracts/baseline-inventory.md
/// rule 2).
fn function_units(
    program: &str,
    tests_dir: &Path,
    category: UnitCategory,
) -> Result<Vec<BaselineUnit>, BaselineError> {
    let source_path = tests_dir.join(format!("{program}.rs"));
    let source = read_to_string(&source_path)?;
    let functions = scan_test_functions(&source);
    if functions.is_empty() {
        return Err(BaselineError::NoTestFunctions {
            source_file: source_path,
        });
    }

    let authored = FUNCTION_UNITS
        .iter()
        .find(|(name, _)| *name == program)
        .map(|(_, units)| *units)
        .unwrap_or(&[]);

    let mut out = Vec::new();
    for function in functions {
        let meta = authored
            .iter()
            .find(|(name, _)| *name == function)
            .map(|(_, a)| *a)
            .ok_or_else(|| BaselineError::MissingAuthoredMetadata {
                unit: format!("{program}::{function}"),
            })?;
        out.push(BaselineUnit {
            id: format!("{program}::{function}"),
            program: program.to_string(),
            category,
            docker_required: false,
            assertion: meta.assertion.to_string(),
            channels: to_strings(meta.channels),
            error_path: meta.error_path,
            fixtures: to_strings(meta.fixtures),
            diff_classes: to_strings(meta.diff_classes),
        });
    }
    Ok(out)
}

/// Units for the pinned external real-world corpus manifest (research D8): one record
/// per `CorpusEntry(name=…)` entry, `program: realworld`.
fn realworld_units(manifest_path: &Path) -> Result<Vec<BaselineUnit>, BaselineError> {
    let source = read_to_string(manifest_path)?;
    let names = scan_manifest_entry_names(&source);
    if names.is_empty() {
        return Err(BaselineError::NoManifestEntries {
            source_file: manifest_path.to_path_buf(),
        });
    }
    Ok(names
        .into_iter()
        .map(|name| BaselineUnit {
            id: format!("realworld::{name}"),
            program: "realworld".to_string(),
            category: UnitCategory::ExternalCorpusEntry,
            docker_required: false,
            assertion: realworld_assertion(&name),
            channels: Vec::new(),
            error_path: false,
            fixtures: Vec::new(),
            diff_classes: Vec::new(),
        })
        .collect())
}

/// Scan Rust source for top-level `#[test]` / `#[tokio::test]` function names, in
/// declaration order.
///
/// Only functions declared at column 0 are accepted, which excludes helpers inside a
/// `#[cfg(test)] mod tests { … }` block — those are unit tests of the file's own
/// helpers, not independently reported outcomes of the program.
fn scan_test_functions(source: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut armed = false;
    for line in source.lines() {
        let trimmed = line.trim();
        if trimmed == "#[test]" || trimmed == "#[tokio::test]" {
            armed = true;
            continue;
        }
        if !armed {
            continue;
        }
        // Additional attributes between the test attribute and the fn keep it armed.
        if trimmed.starts_with("#[") {
            continue;
        }
        let decl = line
            .strip_prefix("async fn ")
            .or_else(|| line.strip_prefix("fn "));
        if let Some(rest) = decl {
            if let Some(name) = rest.split(['(', '<']).next() {
                out.push(name.trim().to_string());
            }
        }
        armed = false;
    }
    out
}

/// Scan the external corpus manifest for its pinned entry names — the `name="…"`
/// field of each `CorpusEntry(…)`, in declaration order.
fn scan_manifest_entry_names(source: &str) -> Vec<String> {
    let mut out = Vec::new();
    for line in source.lines() {
        let trimmed = line.trim();
        let Some(rest) = trimmed.strip_prefix("name=\"") else {
            continue;
        };
        let Some(name) = rest.split('"').next() else {
            continue;
        };
        if !name.is_empty() {
            out.push(name.to_string());
        }
    }
    out
}

fn to_strings(values: &[&str]) -> Vec<String> {
    values.iter().map(|s| (*s).to_string()).collect()
}

fn read_to_string(path: &Path) -> Result<String, BaselineError> {
    std::fs::read_to_string(path).map_err(|e| BaselineError::Io {
        path: path.to_path_buf(),
        cause: e.to_string(),
    })
}

// ---------------------------------------------------------------------------
// Serialization + atomic write
// ---------------------------------------------------------------------------

/// Render the baseline to its canonical string form: 2-space indent, LF endings,
/// trailing newline, no timestamps, no absolute paths. Identical inputs render
/// byte-identically on every platform.
pub fn render(baseline: &BaselineFile) -> String {
    let mut out = serde_json::to_string_pretty(baseline)
        .unwrap_or_else(|e| unreachable!("baseline serialization is infallible: {e}"));
    out.push('\n');
    out
}

/// Atomically write the rendered baseline to `path`, delegating to the single
/// [`crate::atomic_write`] primitive (temp file + rename). Never leaves a partial file.
pub fn write_baseline(path: &Path, baseline: &BaselineFile) -> std::io::Result<()> {
    crate::atomic_write(path, &render(baseline))
}

/// Load a committed baseline. A missing file yields `Ok(None)` — the baseline is
/// generated once, and loading before that is not an error. A present-but-malformed
/// file fails loud (constitution IV).
pub fn load_baseline(path: &Path) -> Result<Option<BaselineFile>, BaselineError> {
    if !path.exists() {
        return Ok(None);
    }
    let raw = read_to_string(path)?;
    let baseline: BaselineFile = serde_json::from_str(&raw).map_err(|e| BaselineError::Io {
        path: path.to_path_buf(),
        cause: format!("malformed baseline: {e}"),
    })?;
    Ok(Some(baseline))
}

// ---------------------------------------------------------------------------
// Drift comparison (`baseline check`)
// ---------------------------------------------------------------------------

/// A compact drift summary between a committed baseline and a fresh regeneration — the
/// `baseline check` mismatch report (contracts/cli-commands.md). Every entry names the
/// specific unit and whether it was added, removed, or changed (FR-004).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BaselineDrift {
    /// Unit ids present in the regeneration but not in the committed file.
    pub added: Vec<String>,
    /// Unit ids present in the committed file but not in the regeneration.
    pub removed: Vec<String>,
    /// Unit ids present in both whose record content differs, each with the fields that
    /// differ.
    pub changed: Vec<(String, Vec<String>)>,
    /// Set when the envelope's freeze `revision` differs.
    pub revision: Option<(String, String)>,
}

impl BaselineDrift {
    /// Whether the committed baseline matches the regeneration exactly.
    pub fn is_empty(&self) -> bool {
        self.added.is_empty()
            && self.removed.is_empty()
            && self.changed.is_empty()
            && self.revision.is_none()
    }

    /// One line per drifted item, each naming the item and its change kind.
    pub fn lines(&self) -> Vec<String> {
        let mut out = Vec::new();
        if let Some((committed, regenerated)) = &self.revision {
            out.push(format!(
                "revision: committed `{committed}` != regenerated `{regenerated}`"
            ));
        }
        for id in &self.added {
            out.push(format!("added: {id}"));
        }
        for id in &self.removed {
            out.push(format!("removed: {id}"));
        }
        for (id, fields) in &self.changed {
            out.push(format!("changed: {id} ({})", fields.join(", ")));
        }
        out
    }
}

/// Compare a committed baseline against a fresh regeneration, naming each added,
/// removed, or changed unit (FR-004). Pure: never writes.
pub fn compare(committed: &BaselineFile, regenerated: &BaselineFile) -> BaselineDrift {
    let mut drift = BaselineDrift::default();

    if committed.revision != regenerated.revision {
        drift.revision = Some((committed.revision.clone(), regenerated.revision.clone()));
    }

    let committed_by_id: BTreeMap<&str, &BaselineUnit> = committed
        .records
        .iter()
        .map(|u| (u.id.as_str(), u))
        .collect();
    let regenerated_by_id: BTreeMap<&str, &BaselineUnit> = regenerated
        .records
        .iter()
        .map(|u| (u.id.as_str(), u))
        .collect();

    for (id, unit) in &regenerated_by_id {
        match committed_by_id.get(id) {
            None => drift.added.push((*id).to_string()),
            Some(existing) => {
                let fields = changed_fields(existing, unit);
                if !fields.is_empty() {
                    drift.changed.push(((*id).to_string(), fields));
                }
            }
        }
    }
    for id in committed_by_id.keys() {
        if !regenerated_by_id.contains_key(id) {
            drift.removed.push((*id).to_string());
        }
    }

    drift.added.sort();
    drift.removed.sort();
    drift.changed.sort();
    drift
}

/// The field names on which two records for the same unit differ.
fn changed_fields(committed: &BaselineUnit, regenerated: &BaselineUnit) -> Vec<String> {
    let mut out = Vec::new();
    let mut push = |name: &str, differs: bool| {
        if differs {
            out.push(name.to_string());
        }
    };
    push("program", committed.program != regenerated.program);
    push("category", committed.category != regenerated.category);
    push(
        "dockerRequired",
        committed.docker_required != regenerated.docker_required,
    );
    push("assertion", committed.assertion != regenerated.assertion);
    push("channels", committed.channels != regenerated.channels);
    push("errorPath", committed.error_path != regenerated.error_path);
    push("fixtures", committed.fixtures != regenerated.fixtures);
    push(
        "diffClasses",
        committed.diff_classes != regenerated.diff_classes,
    );
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unit(id: &str) -> BaselineUnit {
        BaselineUnit {
            id: id.to_string(),
            program: "p".to_string(),
            category: UnitCategory::LivePerCase,
            docker_required: false,
            assertion: "asserts something".to_string(),
            channels: vec!["chan-exit-code".to_string()],
            error_path: false,
            fixtures: Vec::new(),
            diff_classes: vec!["value".to_string()],
        }
    }

    fn file(records: Vec<BaselineUnit>) -> BaselineFile {
        BaselineFile {
            schema_version: BASELINE_SCHEMA_VERSION,
            revision: "abc1234".to_string(),
            generated_from: GeneratedFrom {
                parity_registry: PARITY_REGISTRY_REL.to_string(),
                discovery: vec!["discover_tier1_cases".to_string()],
            },
            records,
        }
    }

    #[test]
    fn scan_test_functions_finds_top_level_tests_only() {
        let src = r#"
#[tokio::test]
async fn a_first() {
}

#[test]
fn b_second() {
}

fn helper() {}

#[cfg(test)]
mod tests {
    #[test]
    fn inner_helper_test() {}
}
"#;
        assert_eq!(scan_test_functions(src), vec!["a_first", "b_second"]);
    }

    #[test]
    fn scan_test_functions_tolerates_extra_attributes() {
        let src = "#[test]\n#[allow(clippy::needless_return)]\nfn c_third() {}\n";
        assert_eq!(scan_test_functions(src), vec!["c_third"]);
    }

    #[test]
    fn scan_manifest_entry_names_reads_pinned_entries() {
        let src = r#"
ENTRIES = (
    CorpusEntry(
        name="images-python",
        repo="devcontainers/images",
    ),
    CorpusEntry(
        name="try-node",
    ),
)
"#;
        assert_eq!(
            scan_manifest_entry_names(src),
            vec!["images-python", "try-node"]
        );
    }

    #[test]
    fn compare_names_added_removed_and_changed_units() {
        let committed = file(vec![unit("p::a"), unit("p::b")]);
        let mut regenerated_records = vec![unit("p::a"), unit("p::c")];
        regenerated_records[0].assertion = "asserts something else".to_string();
        let regenerated = file(regenerated_records);

        let drift = compare(&committed, &regenerated);
        assert!(!drift.is_empty());
        assert_eq!(drift.added, vec!["p::c"]);
        assert_eq!(drift.removed, vec!["p::b"]);
        assert_eq!(
            drift.changed,
            vec![("p::a".to_string(), vec!["assertion".to_string()])]
        );

        let lines = drift.lines();
        assert!(lines.iter().any(|l| l == "added: p::c"));
        assert!(lines.iter().any(|l| l == "removed: p::b"));
        assert!(lines.iter().any(|l| l.starts_with("changed: p::a")));
    }

    #[test]
    fn compare_is_clean_for_identical_files() {
        let a = file(vec![unit("p::a")]);
        let b = file(vec![unit("p::a")]);
        assert!(compare(&a, &b).is_empty());
    }

    #[test]
    fn compare_flags_a_revision_mismatch() {
        let a = file(vec![unit("p::a")]);
        let mut b = file(vec![unit("p::a")]);
        b.revision = "deadbee".to_string();
        let drift = compare(&a, &b);
        assert!(!drift.is_empty());
        assert!(drift.lines()[0].contains("revision"));
    }

    #[test]
    fn render_is_newline_terminated_and_stable() {
        let f = file(vec![unit("p::a")]);
        let once = render(&f);
        assert_eq!(once, render(&f));
        assert!(once.ends_with('\n'));
        assert!(!once.contains('\r'));
    }

    #[test]
    fn every_authored_table_entry_is_unique() {
        for (program, cases) in SCENARIO_UNITS {
            let mut seen = BTreeSet::new();
            for (case, _) in *cases {
                assert!(seen.insert(*case), "duplicate case `{case}` in {program}");
            }
        }
        for (program, functions) in FUNCTION_UNITS {
            let mut seen = BTreeSet::new();
            for (function, _) in *functions {
                assert!(
                    seen.insert(*function),
                    "duplicate function `{function}` in {program}"
                );
            }
        }
    }

    #[test]
    fn runner_diff_classes_narrow_by_oracle_type() {
        let snapshot = runner_diff_classes(Some(OracleType::Snapshot));
        assert!(snapshot.contains(&"stale"));
        assert!(snapshot.contains(&"no-reference-for-platform"));

        let spec = runner_diff_classes(Some(OracleType::SpecExpectation));
        assert!(!spec.contains(&"stale"));
        assert!(spec.contains(&"agree"));
    }

    #[test]
    fn executable_count_excludes_recorded_only_entries() {
        let mut external = unit("realworld::x");
        external.category = UnitCategory::ExternalCorpusEntry;
        let f = file(vec![unit("p::a"), external]);
        assert_eq!(f.records.len(), 2);
        assert_eq!(f.executable_count(), 1);
        assert_eq!(f.count(UnitCategory::ExternalCorpusEntry), 1);
    }
}
