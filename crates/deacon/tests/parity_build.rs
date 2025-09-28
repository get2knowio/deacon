//! Parity tests comparing deacon vs upstream devcontainer CLI for `build` functionality.
//!
//! These tests verify that deacon's build command behaves functionally equivalent to
//! the upstream devcontainer CLI in terms of image creation and discoverability.

use assert_cmd::Command;
use std::fs;
use tempfile::TempDir;

mod parity_utils;

fn upstream_bin() -> String {
    std::env::var("DEACON_PARITY_DEVCONTAINER").unwrap_or_else(|_| "devcontainer".to_string())
}

/// Test build succeeds and creates discoverable image
#[test]
fn parity_build_creates_discoverable_image() {
    if !parity_utils::upstream_available() {
        eprintln!("Skipping parity test: {}", parity_utils::skip_reason());
        return;
    }

    let tmp = TempDir::new().unwrap();
    let ws = tmp.path();
    let unique_token = format!("parity-build-{}", std::process::id());

    fs::create_dir(ws.join(".devcontainer")).unwrap();

    // Create Dockerfile with unique label
    fs::write(
        ws.join("Dockerfile"),
        format!(
            r#"FROM alpine:3.19
ARG FOO
ENV FOO_ENV=$FOO
LABEL parity.token={}
"#,
            unique_token
        ),
    )
    .unwrap();

    // Create devcontainer.json with build context
    fs::write(
        ws.join(".devcontainer/devcontainer.json"),
        r#"{
  "name": "ParityBuild",
  "dockerFile": "../Dockerfile",
  "build": {
    "context": ".."
  }
}
"#,
    )
    .unwrap();

    // upstream: build
    let mut build1 = std::process::Command::new(upstream_bin());
    build1.current_dir(ws);
    build1.arg("build");
    build1.arg("--workspace-folder");
    build1.arg(ws);
    let st1 = build1.output().unwrap();
    assert!(
        st1.status.success(),
        "upstream build failed: {}",
        String::from_utf8_lossy(&st1.stderr)
    );

    // Check if upstream created an image with our label
    let mut inspect1 = std::process::Command::new("docker");
    inspect1.arg("images");
    inspect1.arg("--filter");
    inspect1.arg(format!("label=parity.token={}", unique_token));
    inspect1.arg("--format");
    inspect1.arg("{{.Repository}}:{{.Tag}}");
    let images1 = inspect1.output().unwrap();
    assert!(
        images1.status.success(),
        "docker images failed after upstream build: {}",
        String::from_utf8_lossy(&images1.stderr)
    );
    let upstream_images = String::from_utf8_lossy(&images1.stdout).trim().to_string();
    assert!(
        !upstream_images.is_empty(),
        "upstream build should create an image with label parity.token={}",
        unique_token
    );

    // Clean up the upstream image to avoid conflicts
    if !upstream_images.is_empty() {
        for image in upstream_images.lines() {
            if !image.trim().is_empty() {
                let mut rmi = std::process::Command::new("docker");
                rmi.arg("rmi");
                rmi.arg(image.trim());
                let _ = rmi.output(); // Ignore errors in cleanup
            }
        }
    }

    // deacon: build
    let mut build2 = Command::cargo_bin("deacon").unwrap();
    let st2 = build2
        .current_dir(ws)
        .arg("build")
        .arg("--workspace-folder")
        .arg(ws)
        .assert()
        .get_output()
        .to_owned();
    assert!(
        st2.status.success(),
        "deacon build failed: {}",
        String::from_utf8_lossy(&st2.stderr)
    );

    // Check if deacon created an image with our label
    let mut inspect2 = std::process::Command::new("docker");
    inspect2.arg("images");
    inspect2.arg("--filter");
    inspect2.arg(format!("label=parity.token={}", unique_token));
    inspect2.arg("--format");
    inspect2.arg("{{.Repository}}:{{.Tag}}");
    let images2 = inspect2.output().unwrap();
    assert!(
        images2.status.success(),
        "docker images failed after deacon build: {}",
        String::from_utf8_lossy(&images2.stderr)
    );
    let deacon_images = String::from_utf8_lossy(&images2.stdout).trim().to_string();
    assert!(
        !deacon_images.is_empty(),
        "deacon build should create an image with label parity.token={}",
        unique_token
    );

    // Both should have created images - we don't require exact same image names
    // but both should be discoverable via the same label
    eprintln!("upstream created images: {}", upstream_images);
    eprintln!("deacon created images: {}", deacon_images);

    // Clean up the deacon image
    if !deacon_images.is_empty() {
        for image in deacon_images.lines() {
            if !image.trim().is_empty() {
                let mut rmi = std::process::Command::new("docker");
                rmi.arg("rmi");
                rmi.arg(image.trim());
                let _ = rmi.output(); // Ignore errors in cleanup
            }
        }
    }
}

/// Test build with build args
#[test]
fn parity_build_with_build_args() {
    if !parity_utils::upstream_available() {
        eprintln!("Skipping parity test: {}", parity_utils::skip_reason());
        return;
    }

    let tmp = TempDir::new().unwrap();
    let ws = tmp.path();
    let unique_token = format!("parity-build-args-{}", std::process::id());

    fs::create_dir(ws.join(".devcontainer")).unwrap();

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

    // Create devcontainer.json with build args
    fs::write(
        ws.join(".devcontainer/devcontainer.json"),
        r#"{
  "name": "ParityBuildArgs",
  "dockerFile": "../Dockerfile",
  "build": {
    "context": "..",
    "args": {
      "BUILD_ARG_VALUE": "parity-test"
    }
  }
}
"#,
    )
    .unwrap();

    // upstream: build
    let mut build1 = std::process::Command::new(upstream_bin());
    build1.current_dir(ws);
    build1.arg("build");
    build1.arg("--workspace-folder");
    build1.arg(ws);
    let st1 = build1.output().unwrap();
    assert!(
        st1.status.success(),
        "upstream build failed: {}",
        String::from_utf8_lossy(&st1.stderr)
    );

    // Check if upstream created an image with our build arg label
    let mut inspect1 = std::process::Command::new("docker");
    inspect1.arg("images");
    inspect1.arg("--filter");
    inspect1.arg(format!("label=parity.token={}", unique_token));
    inspect1.arg("--filter");
    inspect1.arg("label=build.arg.value=parity-test");
    inspect1.arg("--format");
    inspect1.arg("{{.Repository}}:{{.Tag}}");
    let images1 = inspect1.output().unwrap();
    assert!(
        images1.status.success(),
        "docker images failed after upstream build: {}",
        String::from_utf8_lossy(&images1.stderr)
    );
    let upstream_images = String::from_utf8_lossy(&images1.stdout).trim().to_string();
    assert!(
        !upstream_images.is_empty(),
        "upstream build should create an image with build arg label"
    );

    // Clean up the upstream image
    if !upstream_images.is_empty() {
        for image in upstream_images.lines() {
            if !image.trim().is_empty() {
                let mut rmi = std::process::Command::new("docker");
                rmi.arg("rmi");
                rmi.arg(image.trim());
                let _ = rmi.output(); // Ignore errors in cleanup
            }
        }
    }

    // deacon: build
    let mut build2 = Command::cargo_bin("deacon").unwrap();
    let st2 = build2
        .current_dir(ws)
        .arg("build")
        .arg("--workspace-folder")
        .arg(ws)
        .assert()
        .get_output()
        .to_owned();
    assert!(
        st2.status.success(),
        "deacon build failed: {}",
        String::from_utf8_lossy(&st2.stderr)
    );

    // Check if deacon created an image with our build arg label
    let mut inspect2 = std::process::Command::new("docker");
    inspect2.arg("images");
    inspect2.arg("--filter");
    inspect2.arg(format!("label=parity.token={}", unique_token));
    inspect2.arg("--filter");
    inspect2.arg("label=build.arg.value=parity-test");
    inspect2.arg("--format");
    inspect2.arg("{{.Repository}}:{{.Tag}}");
    let images2 = inspect2.output().unwrap();
    assert!(
        images2.status.success(),
        "docker images failed after deacon build: {}",
        String::from_utf8_lossy(&images2.stderr)
    );
    let deacon_images = String::from_utf8_lossy(&images2.stdout).trim().to_string();
    assert!(
        !deacon_images.is_empty(),
        "deacon build should create an image with build arg label"
    );

    // Both should have processed build args correctly
    eprintln!(
        "upstream created images with build args: {}",
        upstream_images
    );
    eprintln!("deacon created images with build args: {}", deacon_images);

    // Clean up the deacon image
    if !deacon_images.is_empty() {
        for image in deacon_images.lines() {
            if !image.trim().is_empty() {
                let mut rmi = std::process::Command::new("docker");
                rmi.arg("rmi");
                rmi.arg(image.trim());
                let _ = rmi.output(); // Ignore errors in cleanup
            }
        }
    }
}
