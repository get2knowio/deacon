//! Parity: deacon vs the pinned `@devcontainers/cli` oracle for `build`.
//!
//! Runs ONLY under `cargo nextest run --profile parity`. There is no opt-in env
//! gate and no silent skip: a missing/mismatched oracle or an unavailable Docker
//! FAILS the test with a cause-specific message (018-harden-parity-harness). Both
//! CLIs' raw output is preserved under `target/parity/raw/` and a run-report
//! fragment is written to `target/parity/report/parity_build.json`.
//!
//! The six historical `build` tests are consolidated into ONE test running each
//! as a sequential case (one report fragment per binary is the design). Two cases
//! compare deacon against the oracle (`creates-discoverable-image`,
//! `with-build-args`); the remaining four exercise deacon-only build surface
//! (`--push`, `--output`, BuildKit-only flags, image-reference) that the oracle
//! does not model — but every parity binary still certifies against the pinned
//! oracle up front, even for deacon-only cases.

use std::fs;
use std::path::Path;

use parity_harness::HarnessError;
use parity_harness::exec::{ExecKind, Invocation, exec_deacon, exec_oracle};
use parity_harness::oracle::Oracle;
use parity_harness::prereq::require_docker;
use parity_harness::report::{
    CaseResult, Cause, OracleInfo, RawPaths, ReportFragment, now_rfc3339,
};
use tempfile::TempDir;

/// This binary's name — the fragment key and raw-artifact subdirectory.
const BINARY: &str = "parity_build";

/// Fail the test with the error's cause-specific `Display` message (never the
/// `Debug` form) so an oracle/prereq failure reads as its remedy.
fn ff<T>(r: Result<T, HarnessError>) -> T {
    r.unwrap_or_else(|e| panic!("{e}"))
}

/// The four preserved raw-output paths (report-relative) for a compared case
/// (deacon vs oracle).
fn raw_paths(deacon: &Invocation, oracle: &Invocation) -> RawPaths {
    RawPaths {
        deacon_stdout: deacon.stdout_rel.display().to_string(),
        deacon_stderr: deacon.stderr_rel.display().to_string(),
        oracle_stdout: oracle.stdout_rel.display().to_string(),
        oracle_stderr: oracle.stderr_rel.display().to_string(),
    }
}

/// Raw-output paths for a deacon-only case (the oracle does not model this
/// surface, so its two slots are empty).
fn raw_paths_deacon(deacon: &Invocation) -> RawPaths {
    RawPaths {
        deacon_stdout: deacon.stdout_rel.display().to_string(),
        deacon_stderr: deacon.stderr_rel.display().to_string(),
        oracle_stdout: String::new(),
        oracle_stderr: String::new(),
    }
}

/// `docker images [-a] --filter label=<label> --format {{.ID}}` → image ids.
/// Asserts the `docker images` invocation itself succeeded (a broken Docker CLI
/// is a hard failure, not an empty result).
fn docker_image_ids(label: &str, all: bool) -> Vec<String> {
    let filter = format!("label={label}");
    let mut args: Vec<&str> = vec!["images"];
    if all {
        args.push("-a");
    }
    args.extend_from_slice(&["--filter", &filter, "--format", "{{.ID}}"]);
    let out = std::process::Command::new("docker")
        .args(&args)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "docker images failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

/// `docker inspect -f '{{ json .Config.Labels }}' <id>` → the raw labels JSON.
fn docker_labels_json(id: &str) -> String {
    let out = std::process::Command::new("docker")
        .args(["inspect", "-f", "{{ json .Config.Labels }}", id])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "docker inspect failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).to_string()
}

/// Best-effort `docker rmi <id>` (cleanup; never fails the run).
fn docker_rmi(id: &str) {
    let _ = std::process::Command::new("docker")
        .args(["rmi", id])
        .output();
}

#[tokio::test]
async fn parity_build() {
    // Fail fast for ALL cases: every parity binary certifies against the pinned
    // oracle (even the deacon-only cases) and requires a working Docker.
    let oracle = ff(Oracle::acquire().await);
    ff(require_docker().await);
    let deacon_bin = Path::new(env!("CARGO_BIN_EXE_deacon"));

    let started = now_rfc3339();
    let mut cases: Vec<CaseResult> = Vec::new();
    let mut failures: Vec<String> = Vec::new();

    // ---------------------------------------------------------------------
    // Case: creates-discoverable-image
    // upstream build + deacon build; both must create an image discoverable by
    // a unique parity.token label.
    // ---------------------------------------------------------------------
    {
        let case = "creates-discoverable-image";
        let tmp = TempDir::new().unwrap();
        let ws = tmp.path();
        let ws_str = ws.to_string_lossy().into_owned();
        let unique_token = format!("parity-build-{}", std::process::id());

        fs::write(
            ws.join("Dockerfile"),
            format!(
                r#"FROM alpine:3.19
LABEL parity.token={}
"#,
                unique_token
            ),
        )
        .unwrap();
        fs::write(
            ws.join(".devcontainer.json"),
            r#"{
        "name": "ParityBuild",
        "dockerFile": "Dockerfile",
        "build": {
            "context": "."
        }
    }
    "#,
        )
        .unwrap();

        let args = ["build", "--workspace-folder", ws_str.as_str()];

        // upstream: build
        let oracle_inv =
            ff(exec_oracle(BINARY, case, ExecKind::Lifecycle, &oracle.path, &args, ws).await);
        ff(oracle_inv.require_success());

        // Discover upstream image by label (retry: daemon may not have flushed
        // image metadata yet).
        std::thread::sleep(std::time::Duration::from_millis(500));
        let label = format!("parity.token={}", unique_token);
        let mut upstream_ids: Vec<String> = Vec::new();
        for _ in 0..20 {
            upstream_ids = docker_image_ids(&label, true);
            if !upstream_ids.is_empty() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(500));
        }
        assert!(
            !upstream_ids.is_empty(),
            "upstream build should create an image with label parity.token={}",
            unique_token
        );
        for id in &upstream_ids {
            docker_rmi(id);
        }

        // deacon: build
        let deacon_inv =
            ff(exec_deacon(BINARY, case, ExecKind::Lifecycle, deacon_bin, &args, ws).await);
        ff(deacon_inv.require_success());

        let deacon_ids = docker_image_ids(&label, true);
        let raw = raw_paths(&deacon_inv, &oracle_inv);
        if deacon_ids.is_empty() {
            let msg = format!(
                "deacon build should create an image with label parity.token={}",
                unique_token
            );
            cases.push(CaseResult::fail(
                case,
                Cause::Divergence,
                Some(msg.clone()),
                raw,
            ));
            failures.push(format!("[{case}] {msg}"));
        } else {
            cases.push(CaseResult::pass(case, raw));
        }
        for id in &deacon_ids {
            docker_rmi(id);
        }
    }

    // ---------------------------------------------------------------------
    // Case: with-build-args
    // upstream + deacon build with build args; both images must carry the
    // build-arg-derived label.
    // ---------------------------------------------------------------------
    {
        let case = "with-build-args";
        let tmp = TempDir::new().unwrap();
        let ws = tmp.path();
        let ws_str = ws.to_string_lossy().into_owned();
        let unique_token = format!("parity-build-args-{}", std::process::id());

        fs::write(
            ws.join("Dockerfile"),
            format!(
                r#"FROM alpine:3.19
ARG BUILD_ARG_VALUE=default
ENV BUILD_ARG_VALUE=$BUILD_ARG_VALUE
LABEL parity.token={}
LABEL build.arg.value=$BUILD_ARG_VALUE
"#,
                unique_token
            ),
        )
        .unwrap();
        fs::write(
            ws.join(".devcontainer.json"),
            r#"{
        "name": "ParityBuildArgs",
        "dockerFile": "Dockerfile",
        "build": {
            "context": ".",
            "args": {
                "BUILD_ARG_VALUE": "parity-test"
            }
        }
    }
    "#,
        )
        .unwrap();

        let args = ["build", "--workspace-folder", ws_str.as_str()];
        let label = format!("parity.token={}", unique_token);

        // upstream: build
        let oracle_inv =
            ff(exec_oracle(BINARY, case, ExecKind::Lifecycle, &oracle.path, &args, ws).await);
        ff(oracle_inv.require_success());

        let upstream_ids = docker_image_ids(&label, false);
        assert!(
            !upstream_ids.is_empty(),
            "upstream build should produce at least one image with parity token label"
        );
        let upstream_has_arg = upstream_ids
            .iter()
            .any(|id| docker_labels_json(id).contains("\"build.arg.value\":\"parity-test\""));
        assert!(
            upstream_has_arg,
            "upstream image should carry build.arg.value=parity-test label"
        );
        for id in &upstream_ids {
            docker_rmi(id);
        }

        // deacon: build
        let deacon_inv =
            ff(exec_deacon(BINARY, case, ExecKind::Lifecycle, deacon_bin, &args, ws).await);
        ff(deacon_inv.require_success());

        let deacon_ids = docker_image_ids(&label, false);
        assert!(
            !deacon_ids.is_empty(),
            "deacon build should produce at least one image with parity token label"
        );
        let deacon_has_arg = deacon_ids
            .iter()
            .any(|id| docker_labels_json(id).contains("\"build.arg.value\":\"parity-test\""));

        let raw = raw_paths(&deacon_inv, &oracle_inv);
        if deacon_has_arg {
            cases.push(CaseResult::pass(case, raw));
        } else {
            let msg = "deacon image should carry build.arg.value=parity-test label".to_string();
            cases.push(CaseResult::fail(
                case,
                Cause::Divergence,
                Some(msg.clone()),
                raw,
            ));
            failures.push(format!("[{case}] {msg}"));
        }
        for id in &deacon_ids {
            docker_rmi(id);
        }
    }

    // ---------------------------------------------------------------------
    // Case: push-json-output (DEACON-ONLY)
    // Verifies --push JSON output shape whether the build succeeds or fails.
    // ---------------------------------------------------------------------
    {
        let case = "push-json-output";
        let tmp = TempDir::new().unwrap();
        let ws = tmp.path();
        let ws_str = ws.to_string_lossy().into_owned();

        fs::write(
            ws.join("Dockerfile"),
            r#"FROM alpine:3.19
LABEL test.push=true
"#,
        )
        .unwrap();
        fs::write(
            ws.join(".devcontainer.json"),
            r#"{
        "name": "ParityBuildPush",
        "dockerFile": "Dockerfile",
        "build": {
            "context": "."
        }
    }
    "#,
        )
        .unwrap();

        let args = [
            "build",
            "--workspace-folder",
            ws_str.as_str(),
            "--push",
            "--image-name",
            "localhost:5000/test-push:latest",
            "--output-format",
            "json",
        ];
        let inv = ff(exec_deacon(BINARY, case, ExecKind::Lifecycle, deacon_bin, &args, ws).await);

        let stdout = inv.stdout_string();
        let stderr = String::from_utf8_lossy(&inv.stderr);
        let raw = raw_paths_deacon(&inv);

        if inv.success {
            // If successful, verify JSON output contains pushed field.
            let parsed: serde_json::Value =
                serde_json::from_str(stdout.trim()).expect("stdout should be valid JSON");
            let mut problems: Vec<String> = Vec::new();
            if parsed["outcome"] != "success" {
                problems.push("Build should have success outcome".to_string());
            }
            if !parsed["pushed"].is_boolean() {
                problems.push("pushed field should be present and boolean".to_string());
            }
            // Clean up pushed image if any.
            docker_rmi("localhost:5000/test-push:latest");

            if problems.is_empty() {
                cases.push(CaseResult::pass(case, raw));
            } else {
                let msg = problems.join("; ");
                cases.push(CaseResult::fail(
                    case,
                    Cause::Divergence,
                    Some(msg.clone()),
                    raw,
                ));
                failures.push(format!("[{case}] {msg}"));
            }
        } else {
            // If failed, should have proper error output (BuildKit or Docker).
            let ok = stdout.contains("BuildKit is required")
                || stderr.contains("BuildKit is required")
                || stdout.contains("outcome")
                || stderr.contains("Docker");
            if ok {
                cases.push(CaseResult::pass(case, raw));
            } else {
                let msg = "Expected BuildKit or Docker error in failure case".to_string();
                cases.push(CaseResult::fail(
                    case,
                    Cause::Divergence,
                    Some(msg.clone()),
                    raw,
                ));
                failures.push(format!("[{case}] {msg}"));
            }
        }
    }

    // ---------------------------------------------------------------------
    // Case: output-json-format (DEACON-ONLY)
    // Verifies --output JSON output shape whether the build succeeds or fails.
    // ---------------------------------------------------------------------
    {
        let case = "output-json-format";
        let tmp = TempDir::new().unwrap();
        let ws = tmp.path();
        let ws_str = ws.to_string_lossy().into_owned();

        fs::write(
            ws.join("Dockerfile"),
            r#"FROM alpine:3.19
LABEL test.export=true
"#,
        )
        .unwrap();
        fs::write(
            ws.join(".devcontainer.json"),
            r#"{
        "name": "ParityBuildOutput",
        "dockerFile": "Dockerfile",
        "build": {
            "context": "."
        }
    }
    "#,
        )
        .unwrap();

        let export_path = tmp.path().join("export.tar");
        let output_spec = format!("type=docker,dest={}", export_path.display());
        let args = [
            "build",
            "--workspace-folder",
            ws_str.as_str(),
            "--output",
            output_spec.as_str(),
            "--output-format",
            "json",
        ];
        let inv = ff(exec_deacon(BINARY, case, ExecKind::Lifecycle, deacon_bin, &args, ws).await);

        let stdout = inv.stdout_string();
        let stderr = String::from_utf8_lossy(&inv.stderr);
        let raw = raw_paths_deacon(&inv);

        if inv.success {
            let parsed: serde_json::Value =
                serde_json::from_str(stdout.trim()).expect("stdout should be valid JSON");
            let mut problems: Vec<String> = Vec::new();
            if parsed["outcome"] != "success" {
                problems.push("Build should have success outcome".to_string());
            }
            if !parsed["exportPath"].is_string() {
                problems.push("exportPath field should be present and string".to_string());
            }
            // Clean up export file if created.
            let _ = std::fs::remove_file(&export_path);

            if problems.is_empty() {
                cases.push(CaseResult::pass(case, raw));
            } else {
                let msg = problems.join("; ");
                cases.push(CaseResult::fail(
                    case,
                    Cause::Divergence,
                    Some(msg.clone()),
                    raw,
                ));
                failures.push(format!("[{case}] {msg}"));
            }
        } else {
            let ok = stdout.contains("BuildKit is required")
                || stderr.contains("BuildKit is required")
                || stdout.contains("outcome")
                || stderr.contains("Docker");
            if ok {
                cases.push(CaseResult::pass(case, raw));
            } else {
                let msg = "Expected BuildKit or Docker error in failure case".to_string();
                cases.push(CaseResult::fail(
                    case,
                    Cause::Divergence,
                    Some(msg.clone()),
                    raw,
                ));
                failures.push(format!("[{case}] {msg}"));
            }
        }
    }

    // ---------------------------------------------------------------------
    // Case: buildkit-only-features (DEACON-ONLY)
    // Regression: BuildKit-only flags must fail gracefully without BuildKit.
    // ---------------------------------------------------------------------
    {
        let case = "buildkit-only-features";
        let tmp = TempDir::new().unwrap();
        let ws = tmp.path();
        let ws_str = ws.to_string_lossy().into_owned();

        fs::write(
            ws.join("Dockerfile"),
            r#"FROM alpine:3.19
LABEL test.buildkit=true
"#,
        )
        .unwrap();
        fs::write(
            ws.join(".devcontainer.json"),
            r#"{
        "name": "ParityBuildKitOnly",
        "dockerFile": "Dockerfile",
        "build": {
            "context": "."
        }
    }
    "#,
        )
        .unwrap();

        let buildkit_flags = [
            ("--platform", "linux/amd64"),
            ("--cache-to", "type=local,dest=/tmp/cache"),
        ];

        let mut last_inv: Option<Invocation> = None;
        let mut problems: Vec<String> = Vec::new();
        for (flag_name, flag_value) in buildkit_flags {
            let args = [
                "build",
                "--workspace-folder",
                ws_str.as_str(),
                flag_name,
                flag_value,
                "--output-format",
                "json",
            ];
            let inv =
                ff(exec_deacon(BINARY, case, ExecKind::Lifecycle, deacon_bin, &args, ws).await);
            let stdout = inv.stdout_string();
            let stderr = String::from_utf8_lossy(&inv.stderr);

            // If BuildKit is not available, should fail with proper error. If it
            // succeeded, BuildKit was available and the feature worked.
            if !inv.success {
                let ok = stdout.contains("BuildKit is required")
                    || stderr.contains("BuildKit is required")
                    || stdout.contains("outcome")
                    || stderr.contains("Docker");
                if !ok {
                    problems.push(format!(
                        "BuildKit-only flag {} should fail gracefully without BuildKit",
                        flag_name
                    ));
                }
            }
            drop(stdout);
            drop(stderr);
            last_inv = Some(inv);
        }

        let raw = last_inv.as_ref().map(raw_paths_deacon).unwrap_or(RawPaths {
            deacon_stdout: String::new(),
            deacon_stderr: String::new(),
            oracle_stdout: String::new(),
            oracle_stderr: String::new(),
        });
        if problems.is_empty() {
            cases.push(CaseResult::pass(case, raw));
        } else {
            let msg = problems.join("; ");
            cases.push(CaseResult::fail(
                case,
                Cause::Divergence,
                Some(msg.clone()),
                raw,
            ));
            failures.push(format!("[{case}] {msg}"));
        }
    }

    // ---------------------------------------------------------------------
    // Case: image-reference (DEACON-ONLY)
    // deacon builds from an image ref, applying features + custom tags; must
    // succeed, emit valid JSON with the custom tag, and create a labeled image.
    // ---------------------------------------------------------------------
    {
        let case = "image-reference";
        let tmp = TempDir::new().unwrap();
        let ws = tmp.path();
        let ws_str = ws.to_string_lossy().into_owned();
        let unique_token = format!("parity-image-ref-{}", std::process::id());

        fs::write(
            ws.join(".devcontainer.json"),
            r#"{
        "name": "ParityBuildImageRef",
        "image": "alpine:3.19"
    }
    "#,
        )
        .unwrap();

        let custom_tag = format!("test-image-ref:{}", unique_token);
        let label_arg = format!("parity.token={}", unique_token);
        let args = [
            "build",
            "--workspace-folder",
            ws_str.as_str(),
            "--image-name",
            custom_tag.as_str(),
            "--label",
            label_arg.as_str(),
            "--output-format",
            "json",
        ];
        let inv = ff(exec_deacon(BINARY, case, ExecKind::Lifecycle, deacon_bin, &args, ws).await);
        // This case hard-asserted success in the original — preserve that.
        ff(inv.require_success());

        let raw = raw_paths_deacon(&inv);
        let stdout = inv.stdout_string();
        let parsed: serde_json::Value =
            serde_json::from_str(stdout.trim()).expect("stdout should be valid JSON");
        assert_eq!(
            parsed["outcome"], "success",
            "Build should have success outcome"
        );

        // Verify imageName array contains the custom tag.
        let image_names = parsed["imageName"]
            .as_array()
            .expect("imageName should be an array");
        let tags: Vec<String> = image_names
            .iter()
            .filter_map(|v| v.as_str())
            .map(|s| s.to_string())
            .collect();
        let tag_ok = tags.iter().any(|t| t.contains(&unique_token));

        // Verify an image was created with the label.
        let image_ids = docker_image_ids(&label_arg, false);
        let image_ok = !image_ids.is_empty();

        if tag_ok && image_ok {
            cases.push(CaseResult::pass(case, raw));
        } else {
            let mut problems: Vec<String> = Vec::new();
            if !tag_ok {
                problems.push(format!(
                    "imageName should contain custom tag with unique token: {:?}",
                    tags
                ));
            }
            if !image_ok {
                problems.push(
                    "Image-reference build should create an image with parity token label"
                        .to_string(),
                );
            }
            let msg = problems.join("; ");
            cases.push(CaseResult::fail(
                case,
                Cause::Divergence,
                Some(msg.clone()),
                raw,
            ));
            failures.push(format!("[{case}] {msg}"));
        }

        // Clean up.
        for id in &image_ids {
            docker_rmi(id);
        }
        docker_rmi(&custom_tag);
    }

    let finished = now_rfc3339();
    let fragment = ReportFragment::new(
        BINARY,
        OracleInfo::from(&oracle),
        started,
        finished,
        cases,
        Vec::new(),
    );
    ff(fragment.write().await);

    assert!(
        failures.is_empty(),
        "build parity divergence(s) vs oracle {}:\n{}",
        oracle.version,
        failures.join("\n\n"),
    );
}
