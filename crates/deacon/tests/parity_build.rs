#![cfg(feature = "full")]
//! Parity tests comparing deacon vs upstream devcontainer CLI for `build` functionality.
//!
//! These tests verify that deacon's build command behaves functionally equivalent to
//! the upstream devcontainer CLI in terms of image creation and discoverability.

use std::fs;
use tempfile::TempDir;

mod parity_utils;

/// Test build succeeds and creates discoverable image
#[test]
fn parity_build_creates_discoverable_image() {
    if !parity_utils::parity_enabled() {
        eprintln!("Skipping parity test: {}", parity_utils::skip_reason());
        return;
    }
    if !parity_utils::docker_available() {
        eprintln!(
            "Skipping parity test (Docker unavailable): {}",
            parity_utils::skip_reason()
        );
        return;
    }
    if !parity_utils::upstream_available() {
        eprintln!("Skipping parity test: {}", parity_utils::skip_reason());
        return;
    }

    let tmp = TempDir::new().unwrap();
    let ws = tmp.path();
    let unique_token = format!("parity-build-{}", std::process::id());

    // Create Dockerfile at workspace root with unique label
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

    // Create root-level .devcontainer.json referencing Dockerfile at workspace root
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

    // upstream: build
    let st1 =
        parity_utils::run_upstream(ws, &["build", "--workspace-folder", &ws.to_string_lossy()])
            .unwrap();
    assert!(
        st1.status.success(),
        "upstream build failed (code {:?}): {}",
        st1.status.code(),
        String::from_utf8_lossy(&st1.stderr)
    );

    // Check if upstream created an image with our label (discover by ID with retry)
    // Small initial delay in case the daemon hasn't flushed image metadata yet
    std::thread::sleep(std::time::Duration::from_millis(500));
    let mut upstream_ids: Vec<String> = Vec::new();
    for _ in 0..20 {
        let images1 = std::process::Command::new("docker")
            .args([
                "images",
                "-a",
                "--filter",
                &format!("label=parity.token={}", unique_token),
                "--format",
                "{{.ID}}",
            ])
            .output()
            .unwrap();
        assert!(
            images1.status.success(),
            "docker images failed after upstream build: {}",
            String::from_utf8_lossy(&images1.stderr)
        );
        upstream_ids = String::from_utf8_lossy(&images1.stdout)
            .lines()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
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

    // Clean up the upstream image(s) to avoid conflicts
    if !upstream_ids.is_empty() {
        for id in &upstream_ids {
            let _ = std::process::Command::new("docker")
                .args(["rmi", id])
                .output();
        }
    }

    // deacon: build
    let st2 = parity_utils::run_deacon(ws, &["build", "--workspace-folder", &ws.to_string_lossy()])
        .unwrap();
    assert!(
        st2.status.success(),
        "deacon build failed (code {:?}): {}",
        st2.status.code(),
        String::from_utf8_lossy(&st2.stderr)
    );

    // Check if deacon created an image with our label (discover by ID for robustness)
    let images2 = std::process::Command::new("docker")
        .args([
            "images",
            "-a",
            "--filter",
            &format!("label=parity.token={}", unique_token),
            "--format",
            "{{.ID}}",
        ])
        .output()
        .unwrap();
    assert!(
        images2.status.success(),
        "docker images failed after deacon build: {}",
        String::from_utf8_lossy(&images2.stderr)
    );
    let deacon_ids: Vec<String> = String::from_utf8_lossy(&images2.stdout)
        .lines()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();
    assert!(
        !deacon_ids.is_empty(),
        "deacon build should create an image with label parity.token={}",
        unique_token
    );

    // Both should have created images - we don't require exact same image names
    // but both should be discoverable via the same label
    eprintln!("upstream created images (IDs): {}", upstream_ids.join(", "));
    eprintln!("deacon created images (IDs): {}", deacon_ids.join(", "));

    // Clean up the deacon image(s)
    if !deacon_ids.is_empty() {
        for id in &deacon_ids {
            let _ = std::process::Command::new("docker")
                .args(["rmi", id])
                .output();
        }
    }
}

/// Test build with build args
#[test]
fn parity_build_with_build_args() {
    if !parity_utils::parity_enabled() {
        eprintln!("Skipping parity test: {}", parity_utils::skip_reason());
        return;
    }
    if !parity_utils::docker_available() {
        eprintln!(
            "Skipping parity test (Docker unavailable): {}",
            parity_utils::skip_reason()
        );
        return;
    }
    if !parity_utils::upstream_available() {
        eprintln!("Skipping parity test: {}", parity_utils::skip_reason());
        return;
    }

    let tmp = TempDir::new().unwrap();
    let ws = tmp.path();
    let unique_token = format!("parity-build-args-{}", std::process::id());

    // Create Dockerfile that uses build arg
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

    // Create root-level .devcontainer.json with build args
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

    // upstream: build
    let st1 =
        parity_utils::run_upstream(ws, &["build", "--workspace-folder", &ws.to_string_lossy()])
            .unwrap();
    assert!(
        st1.status.success(),
        "upstream build failed (code {:?}): {}",
        st1.status.code(),
        String::from_utf8_lossy(&st1.stderr)
    );

    // Find image IDs by parity token first, then inspect for the build arg label
    fn docker_list_image_ids_by_label(label: &str) -> Vec<String> {
        let out = std::process::Command::new("docker")
            .args([
                "images",
                "--filter",
                &format!("label={}", label),
                "--format",
                "{{.ID}}",
            ])
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

    let upstream_ids = docker_list_image_ids_by_label(&format!("parity.token={}", unique_token));
    assert!(
        !upstream_ids.is_empty(),
        "upstream build should produce at least one image with parity token label"
    );
    let mut found_build_arg = false;
    for id in &upstream_ids {
        let out = std::process::Command::new("docker")
            .args(["inspect", "-f", "{{ json .Config.Labels }}", id])
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "docker inspect failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        let labels_json = String::from_utf8_lossy(&out.stdout);
        if labels_json.contains("\"build.arg.value\":\"parity-test\"") {
            found_build_arg = true;
            break;
        }
    }
    assert!(
        found_build_arg,
        "upstream image should carry build.arg.value=parity-test label"
    );

    // Clean up the upstream image(s)
    if !upstream_ids.is_empty() {
        for id in &upstream_ids {
            let _ = std::process::Command::new("docker")
                .args(["rmi", id])
                .output();
        }
    }

    // deacon: build
    let st2 = parity_utils::run_deacon(ws, &["build", "--workspace-folder", &ws.to_string_lossy()])
        .unwrap();
    assert!(
        st2.status.success(),
        "deacon build failed (code {:?}): {}",
        st2.status.code(),
        String::from_utf8_lossy(&st2.stderr)
    );

    let deacon_ids = docker_list_image_ids_by_label(&format!("parity.token={}", unique_token));
    assert!(
        !deacon_ids.is_empty(),
        "deacon build should produce at least one image with parity token label"
    );
    let mut found_build_arg2 = false;
    for id in &deacon_ids {
        let out = std::process::Command::new("docker")
            .args(["inspect", "-f", "{{ json .Config.Labels }}", id])
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "docker inspect failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        let labels_json = String::from_utf8_lossy(&out.stdout);
        if labels_json.contains("\"build.arg.value\":\"parity-test\"") {
            found_build_arg2 = true;
            break;
        }
    }
    assert!(
        found_build_arg2,
        "deacon image should carry build.arg.value=parity-test label"
    );

    // Both should have processed build args correctly
    eprintln!(
        "upstream images with token label: {}",
        upstream_ids.join(", ")
    );
    eprintln!("deacon images with token label: {}", deacon_ids.join(", "));

    // Clean up the deacon image(s)
    if !deacon_ids.is_empty() {
        for id in &deacon_ids {
            let _ = std::process::Command::new("docker")
                .args(["rmi", id])
                .output();
        }
    }
}

/// Test build with --push flag produces correct JSON output format (Phase 4)
#[test]
fn parity_build_push_json_output() {
    // This test verifies that when --push is used (and succeeds or fails properly),
    // the JSON output conforms to the BuildSuccess schema with pushed field
    if !parity_utils::parity_enabled() {
        eprintln!("Skipping parity test: {}", parity_utils::skip_reason());
        return;
    }
    if !parity_utils::docker_available() {
        eprintln!(
            "Skipping parity test (Docker unavailable): {}",
            parity_utils::skip_reason()
        );
        return;
    }

    let tmp = TempDir::new().unwrap();
    let ws = tmp.path();

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

    // Run deacon build with --push (will fail if BuildKit not available or no registry access)
    let st = parity_utils::run_deacon(
        ws,
        &[
            "build",
            "--workspace-folder",
            &ws.to_string_lossy(),
            "--push",
            "--image-name",
            "localhost:5000/test-push:latest",
            "--output-format",
            "json",
        ],
    )
    .unwrap();

    // Check output format regardless of success/failure
    let stdout = String::from_utf8_lossy(&st.stdout);
    let stderr = String::from_utf8_lossy(&st.stderr);

    if st.status.success() {
        // If successful, verify JSON output contains pushed field
        let parsed: serde_json::Value =
            serde_json::from_str(stdout.trim()).expect("stdout should be valid JSON");
        assert_eq!(
            parsed["outcome"], "success",
            "Build should have success outcome"
        );

        // Verify pushed field is present
        assert!(
            parsed["pushed"].is_boolean(),
            "pushed field should be present and boolean"
        );

        // Clean up pushed image if any
        let _ = std::process::Command::new("docker")
            .args(["rmi", "localhost:5000/test-push:latest"])
            .output();
    } else {
        // If failed, should have proper error output (BuildKit requirement or registry error)
        assert!(
            stdout.contains("BuildKit is required") || 
            stderr.contains("BuildKit is required") ||
            stdout.contains("outcome") || // JSON error format
            stderr.contains("Docker"),
            "Expected BuildKit or Docker error in failure case"
        );
    }
}

/// Test build with --output flag produces correct JSON output format (Phase 4)
#[test]
fn parity_build_output_json_format() {
    // This test verifies that when --output is used, the JSON output
    // conforms to the BuildSuccess schema with exportPath field
    if !parity_utils::parity_enabled() {
        eprintln!("Skipping parity test: {}", parity_utils::skip_reason());
        return;
    }
    if !parity_utils::docker_available() {
        eprintln!(
            "Skipping parity test (Docker unavailable): {}",
            parity_utils::skip_reason()
        );
        return;
    }

    let tmp = TempDir::new().unwrap();
    let ws = tmp.path();

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

    // Run deacon build with --output
    let st = parity_utils::run_deacon(
        ws,
        &[
            "build",
            "--workspace-folder",
            &ws.to_string_lossy(),
            "--output",
            &output_spec,
            "--output-format",
            "json",
        ],
    )
    .unwrap();

    let stdout = String::from_utf8_lossy(&st.stdout);
    let stderr = String::from_utf8_lossy(&st.stderr);

    if st.status.success() {
        // If successful, verify JSON output contains exportPath field
        let parsed: serde_json::Value =
            serde_json::from_str(stdout.trim()).expect("stdout should be valid JSON");
        assert_eq!(
            parsed["outcome"], "success",
            "Build should have success outcome"
        );

        // Verify exportPath field is present
        assert!(
            parsed["exportPath"].is_string(),
            "exportPath field should be present and string"
        );

        // Clean up export file if created
        let _ = std::fs::remove_file(&export_path);
    } else {
        // If failed, should have proper error output (BuildKit requirement)
        assert!(
            stdout.contains("BuildKit is required")
                || stderr.contains("BuildKit is required")
                || stdout.contains("outcome")
                || stderr.contains("Docker"),
            "Expected BuildKit or Docker error in failure case"
        );
    }
}

/// Test BuildKit-only feature detection (Phase 4 - T014A)
#[test]
fn parity_build_buildkit_only_features_regression() {
    // This is a regression test to ensure that BuildKit-only features
    // (like advanced cache, push, export) are properly gated
    if !parity_utils::parity_enabled() {
        eprintln!("Skipping parity test: {}", parity_utils::skip_reason());
        return;
    }
    if !parity_utils::docker_available() {
        eprintln!(
            "Skipping parity test (Docker unavailable): {}",
            parity_utils::skip_reason()
        );
        return;
    }

    let tmp = TempDir::new().unwrap();
    let ws = tmp.path();

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

    // Test multiple BuildKit-only flags
    let buildkit_flags = vec![
        ("--platform", "linux/amd64"),
        ("--cache-to", "type=local,dest=/tmp/cache"),
    ];

    for (flag_name, flag_value) in buildkit_flags {
        let st = parity_utils::run_deacon(
            ws,
            &[
                "build",
                "--workspace-folder",
                &ws.to_string_lossy(),
                flag_name,
                flag_value,
                "--output-format",
                "json",
            ],
        )
        .unwrap();

        let stdout = String::from_utf8_lossy(&st.stdout);
        let stderr = String::from_utf8_lossy(&st.stderr);

        // If BuildKit is not available, should fail with proper error
        if !st.status.success() {
            assert!(
                stdout.contains("BuildKit is required")
                    || stderr.contains("BuildKit is required")
                    || stdout.contains("outcome")
                    || stderr.contains("Docker"),
                "BuildKit-only flag {} should fail gracefully without BuildKit",
                flag_name
            );
        }
        // If successful, BuildKit was available and feature worked
    }
}

/// Test image-reference build with feature application and tagging (Phase 5)
#[test]
fn parity_build_image_reference() {
    // This test verifies that deacon can build from an image reference
    // and apply features and tags correctly
    if !parity_utils::parity_enabled() {
        eprintln!("Skipping parity test: {}", parity_utils::skip_reason());
        return;
    }
    if !parity_utils::docker_available() {
        eprintln!(
            "Skipping parity test (Docker unavailable): {}",
            parity_utils::skip_reason()
        );
        return;
    }

    let tmp = TempDir::new().unwrap();
    let ws = tmp.path();
    let unique_token = format!("parity-image-ref-{}", std::process::id());

    // Create devcontainer.json with image reference
    fs::write(
        ws.join(".devcontainer.json"),
        r#"{
        "name": "ParityBuildImageRef",
        "image": "alpine:3.19"
    }
    "#,
    )
    .unwrap();

    // Run deacon build with image reference and custom tag
    let custom_tag = format!("test-image-ref:{}", unique_token);
    let st = parity_utils::run_deacon(
        ws,
        &[
            "build",
            "--workspace-folder",
            &ws.to_string_lossy(),
            "--image-name",
            &custom_tag,
            "--label",
            &format!("parity.token={}", unique_token),
            "--output-format",
            "json",
        ],
    )
    .unwrap();

    assert!(
        st.status.success(),
        "deacon image-reference build failed (code {:?}): {}",
        st.status.code(),
        String::from_utf8_lossy(&st.stderr)
    );

    // Verify JSON output
    let stdout = String::from_utf8_lossy(&st.stdout);
    let parsed: serde_json::Value =
        serde_json::from_str(stdout.trim()).expect("stdout should be valid JSON");
    assert_eq!(
        parsed["outcome"], "success",
        "Build should have success outcome"
    );

    // Verify imageName array contains custom tag
    let image_names = parsed["imageName"]
        .as_array()
        .expect("imageName should be an array");
    let tags: Vec<String> = image_names
        .iter()
        .filter_map(|v| v.as_str())
        .map(|s| s.to_string())
        .collect();
    assert!(
        tags.iter().any(|t| t.contains(&unique_token)),
        "imageName should contain custom tag with unique token: {:?}",
        tags
    );

    // Verify image was created with label
    let images_check = std::process::Command::new("docker")
        .args([
            "images",
            "--filter",
            &format!("label=parity.token={}", unique_token),
            "--format",
            "{{.ID}}",
        ])
        .output()
        .unwrap();
    assert!(
        images_check.status.success(),
        "docker images check failed: {}",
        String::from_utf8_lossy(&images_check.stderr)
    );

    let image_ids: Vec<String> = String::from_utf8_lossy(&images_check.stdout)
        .lines()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();
    assert!(
        !image_ids.is_empty(),
        "Image-reference build should create an image with parity token label"
    );

    // Clean up
    for id in &image_ids {
        let _ = std::process::Command::new("docker")
            .args(["rmi", id])
            .output();
    }
    let _ = std::process::Command::new("docker")
        .args(["rmi", &custom_tag])
        .output();
}
