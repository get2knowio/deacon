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
