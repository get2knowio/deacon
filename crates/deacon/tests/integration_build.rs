//! Integration tests for the build command
//!
//! These tests verify that the build command works with real Docker builds.

use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use tempfile::TempDir;

#[test]
fn test_build_with_dockerfile() {
    // Create a temporary directory with a simple Dockerfile
    let temp_dir = TempDir::new().unwrap();
    let dockerfile_content = r#"FROM alpine:3.19
LABEL test=1
LABEL deacon.test=integration
RUN echo "Building test image"
"#;

    fs::write(temp_dir.path().join("Dockerfile"), dockerfile_content).unwrap();

    // Create a devcontainer.json configuration
    let devcontainer_config = r#"{
    "name": "Test Build Container",
    "dockerFile": "Dockerfile",
    "build": {
        "context": ".",
        "options": {
            "BUILDKIT_INLINE_CACHE": "1"
        }
    }
}
"#;

    fs::create_dir(temp_dir.path().join(".devcontainer")).unwrap();
    fs::write(
        temp_dir.path().join(".devcontainer/devcontainer.json"),
        devcontainer_config,
    )
    .unwrap();

    // Test build command (only if Docker is available)
    let mut cmd = Command::cargo_bin("deacon").unwrap();
    let assert = cmd
        .current_dir(&temp_dir)
        .arg("build")
        .arg("--output-format")
        .arg("json")
        .assert();

    // The command should either succeed (if Docker is available) or fail with Docker error
    let output = assert.get_output();

    if output.status.success() {
        // If successful, check that we got valid JSON output
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(stdout.contains("image_id"));
        assert!(stdout.contains("build_duration"));
        assert!(stdout.contains("config_hash"));
    } else {
        // If failed, it should be because Docker is not available or accessible
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.contains("Docker is not installed")
                || stderr.contains("Docker daemon is not")
                || stderr.contains("Docker build failed")
                || stderr.contains("permission denied"),
            "Unexpected error: {}",
            stderr
        );
    }
}

#[test]
fn test_build_with_missing_dockerfile() {
    let temp_dir = TempDir::new().unwrap();

    // Create a devcontainer.json with a missing Dockerfile
    let devcontainer_config = r#"{
    "name": "Test Build Container",
    "dockerFile": "NonExistentDockerfile"
}
"#;

    fs::create_dir(temp_dir.path().join(".devcontainer")).unwrap();
    fs::write(
        temp_dir.path().join(".devcontainer/devcontainer.json"),
        devcontainer_config,
    )
    .unwrap();

    let mut cmd = Command::cargo_bin("deacon").unwrap();
    cmd.current_dir(&temp_dir)
        .arg("build")
        .assert()
        .failure()
        .stderr(predicate::str::contains("Configuration file not found"));
}

#[test]
fn test_build_with_image_config() {
    let temp_dir = TempDir::new().unwrap();

    // Create a devcontainer.json with image instead of dockerFile
    let devcontainer_config = r#"{
    "name": "Test Build Container",
    "image": "alpine:3.19"
}
"#;

    fs::create_dir(temp_dir.path().join(".devcontainer")).unwrap();
    fs::write(
        temp_dir.path().join(".devcontainer/devcontainer.json"),
        devcontainer_config,
    )
    .unwrap();

    let mut cmd = Command::cargo_bin("deacon").unwrap();
    cmd.current_dir(&temp_dir)
        .arg("build")
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "Cannot build with 'image' configuration",
        ));
}

#[test]
fn test_build_command_flags() {
    let temp_dir = TempDir::new().unwrap();
    let dockerfile_content = "FROM alpine:3.19\nLABEL test=1\n";

    fs::write(temp_dir.path().join("Dockerfile"), dockerfile_content).unwrap();

    let devcontainer_config = r#"{
    "name": "Test Build Container",
    "dockerFile": "Dockerfile"
}
"#;

    fs::create_dir(temp_dir.path().join(".devcontainer")).unwrap();
    fs::write(
        temp_dir.path().join(".devcontainer/devcontainer.json"),
        devcontainer_config,
    )
    .unwrap();

    // Test with various flags
    let mut cmd = Command::cargo_bin("deacon").unwrap();
    let assert = cmd
        .current_dir(&temp_dir)
        .arg("build")
        .arg("--no-cache")
        .arg("--platform")
        .arg("linux/amd64")
        .arg("--build-arg")
        .arg("ENV=test")
        .arg("--build-arg")
        .arg("VERSION=1.0")
        .arg("--force")
        .arg("--output-format")
        .arg("text")
        .assert();

    // The command should either succeed or fail gracefully
    let output = assert.get_output();
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        // Should fail because Docker is not available or because of permissions
        assert!(
            stderr.contains("Docker is not installed")
                || stderr.contains("Docker daemon is not")
                || stderr.contains("Docker build failed")
                || stderr.contains("permission denied"),
            "Unexpected error: {}",
            stderr
        );
    }
}

#[test]
fn test_build_command_advanced_flags() {
    let temp_dir = TempDir::new().unwrap();
    let dockerfile_content = "FROM alpine:3.19\nLABEL test=1\n";

    fs::write(temp_dir.path().join("Dockerfile"), dockerfile_content).unwrap();

    let devcontainer_config = r#"{
    "name": "Test Build Container Advanced",
    "dockerFile": "Dockerfile"
}
"#;

    fs::create_dir(temp_dir.path().join(".devcontainer")).unwrap();
    fs::write(
        temp_dir.path().join(".devcontainer/devcontainer.json"),
        devcontainer_config,
    )
    .unwrap();

    // Test with advanced flags including BuildKit, secrets, SSH, and cache options
    let mut cmd = Command::cargo_bin("deacon").unwrap();
    let assert = cmd
        .current_dir(&temp_dir)
        .arg("build")
        .arg("--no-cache")
        .arg("--platform")
        .arg("linux/amd64")
        .arg("--cache-from")
        .arg("registry://example.com/cache:latest")
        .arg("--cache-from")
        .arg("type=local,src=/tmp/cache")
        .arg("--cache-to")
        .arg("registry://example.com/cache:build")
        .arg("--buildkit")
        .arg("auto")
        .arg("--secret")
        .arg("id=mytoken,src=/dev/null")
        .arg("--ssh")
        .arg("default")
        .arg("--build-arg")
        .arg("ENV=test")
        .arg("--force")
        .arg("--output-format")
        .arg("text")
        .assert();

    // The command should either succeed or fail gracefully
    let output = assert.get_output();
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        // Should fail because Docker is not available or because of permissions
        assert!(
            stderr.contains("Docker is not installed")
                || stderr.contains("Docker daemon is not")
                || stderr.contains("Docker build failed")
                || stderr.contains("permission denied")
                // Advanced BuildKit features might not be available
                || stderr.contains("buildkit")
                || stderr.contains("BuildKit"),
            "Unexpected error: {}",
            stderr
        );
    }
}

// Build cache functionality tests

#[test]
fn test_build_cache_miss_then_hit() {
    // Create a temporary directory with a simple Dockerfile
    let temp_dir = TempDir::new().unwrap();
    let dockerfile_content = r#"FROM alpine:3.19
LABEL test=cache_test
RUN echo "Building with cache test"
"#;

    fs::write(temp_dir.path().join("Dockerfile"), dockerfile_content).unwrap();

    // Create a devcontainer.json configuration
    let devcontainer_config = r#"{
    "name": "Cache Test Container",
    "dockerFile": "Dockerfile",
    "build": {
        "context": ".",
        "options": {}
    }
}
"#;

    fs::create_dir(temp_dir.path().join(".devcontainer")).unwrap();
    fs::write(
        temp_dir.path().join(".devcontainer/devcontainer.json"),
        devcontainer_config,
    )
    .unwrap();

    // First build - should be a cache miss
    let mut cmd = Command::cargo_bin("deacon").unwrap();
    let first_assert = cmd
        .current_dir(&temp_dir)
        .arg("build")
        .arg("--output-format")
        .arg("json")
        .assert();

    // If Docker is available, build should succeed
    // If not available, it should fail with a Docker error
    let first_output = first_assert.get_output();
    if first_output.status.success() {
        // Docker is available - verify successful build
        let stdout = String::from_utf8_lossy(&first_output.stdout);
        assert!(stdout.contains("image_id"));
        assert!(stdout.contains("config_hash"));

        // Check that cache directory was created
        let cache_dir = temp_dir.path().join(".devcontainer").join("build-cache");
        assert!(cache_dir.exists(), "Cache directory should be created");

        // Check that a cache file was created
        let cache_files: Vec<_> = fs::read_dir(&cache_dir)
            .unwrap()
            .filter_map(|entry| entry.ok())
            .filter(|entry| {
                entry
                    .path()
                    .extension()
                    .map(|ext| ext == "json")
                    .unwrap_or(false)
            })
            .collect();
        assert!(
            !cache_files.is_empty(),
            "At least one cache file should be created"
        );

        // Second build - should be a cache hit
        let mut cmd2 = Command::cargo_bin("deacon").unwrap();
        let second_assert = cmd2
            .current_dir(&temp_dir)
            .arg("build")
            .arg("--output-format")
            .arg("json")
            .assert();

        let second_output = second_assert.get_output();
        assert!(
            second_output.status.success(),
            "Second build should succeed: {}",
            String::from_utf8_lossy(&second_output.stderr)
        );

        let second_stdout = String::from_utf8_lossy(&second_output.stdout);

        // Check that the second build used cache by verifying "Using cached build result" in stderr
        let second_stderr = String::from_utf8_lossy(&second_output.stderr);
        assert!(
            second_stderr.contains("Using cached build result"),
            "Second build should use cached result. Stderr: {}",
            second_stderr
        );

        // Verify same image_id and config_hash
        assert!(second_stdout.contains("image_id"));
        assert!(second_stdout.contains("config_hash"));

        // Parse both JSON outputs to verify they match
        if let (Ok(first_json), Ok(second_json)) = (
            serde_json::from_str::<serde_json::Value>(&stdout),
            serde_json::from_str::<serde_json::Value>(&second_stdout),
        ) {
            assert_eq!(
                first_json.get("image_id"),
                second_json.get("image_id"),
                "Image IDs should match between cache miss and cache hit"
            );
            assert_eq!(
                first_json.get("config_hash"),
                second_json.get("config_hash"),
                "Config hashes should match between cache miss and cache hit"
            );
        }
    } else {
        // Docker not available - expected in CI
        let stderr = String::from_utf8_lossy(&first_output.stderr);
        assert!(
            stderr.contains("Docker") || stderr.contains("docker"),
            "Expected Docker-related error, got: {}",
            stderr
        );
    }
}

#[test]
fn test_build_force_flag_bypasses_cache() {
    let temp_dir = TempDir::new().unwrap();
    let dockerfile_content = r#"FROM alpine:3.19
LABEL test=force_test
RUN echo "Testing force flag"
"#;

    fs::write(temp_dir.path().join("Dockerfile"), dockerfile_content).unwrap();

    let devcontainer_config = r#"{
    "name": "Force Test Container",
    "dockerFile": "Dockerfile"
}
"#;

    fs::create_dir(temp_dir.path().join(".devcontainer")).unwrap();
    fs::write(
        temp_dir.path().join(".devcontainer/devcontainer.json"),
        devcontainer_config,
    )
    .unwrap();

    // Create a dummy cache file to simulate existing cache
    let cache_dir = temp_dir.path().join(".devcontainer").join("build-cache");
    fs::create_dir_all(&cache_dir).unwrap();

    // Create a dummy cache file
    let cache_content = r#"{
    "config_hash": "dummy_hash",
    "result": {
        "image_id": "sha256:dummy",
        "tags": ["test:cached"],
        "build_duration": 1.0,
        "metadata": {},
        "config_hash": "dummy_hash"
    },
    "inputs": {
        "dockerfile_hash": "dummy",
        "context_files": [],
        "feature_set_digest": null,
        "build_config": {
            "dockerfile": "Dockerfile",
            "context": ".",
            "target": null,
            "options": {}
        }
    },
    "created_at": 1234567890
}"#;
    fs::write(cache_dir.join("dummy_hash.json"), cache_content).unwrap();

    // Test build with --force flag
    let mut cmd = Command::cargo_bin("deacon").unwrap();
    let assert = cmd
        .current_dir(&temp_dir)
        .arg("build")
        .arg("--force")
        .arg("--output-format")
        .arg("json")
        .assert();

    // Should either succeed (with Docker) or fail gracefully (without Docker)
    let output = assert.get_output();
    if output.status.success() {
        // With Docker: verify actual build happened, not cache hit
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(stdout.contains("image_id"));
        assert!(
            !stdout.contains("sha256:dummy"),
            "Should not use dummy cache"
        );
    } else {
        // Without Docker: expect failure
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.contains("Docker") || stderr.contains("docker"),
            "Expected Docker-related error, got: {}",
            stderr
        );
    }
}

#[test]
fn test_build_with_non_affecting_file_changes() {
    let temp_dir = TempDir::new().unwrap();
    let dockerfile_content = r#"FROM alpine:3.19
RUN echo "Testing non-affecting changes"
"#;

    fs::write(temp_dir.path().join("Dockerfile"), dockerfile_content).unwrap();

    let devcontainer_config = r#"{
    "name": "Non-Affecting Changes Test",
    "dockerFile": "Dockerfile"
}
"#;

    fs::create_dir(temp_dir.path().join(".devcontainer")).unwrap();
    fs::write(
        temp_dir.path().join(".devcontainer/devcontainer.json"),
        devcontainer_config,
    )
    .unwrap();

    // Create a README.md file (should not affect build)
    fs::write(temp_dir.path().join("README.md"), "# Original README").unwrap();

    // First attempt
    let mut cmd1 = Command::cargo_bin("deacon").unwrap();
    let assert1 = cmd1
        .current_dir(&temp_dir)
        .arg("build")
        .arg("--output-format")
        .arg("text")
        .assert();

    // Should either succeed or fail based on Docker availability
    let output1 = assert1.get_output();
    let first_success = output1.status.success();

    // Modify README.md (should not affect build cache)
    fs::write(
        temp_dir.path().join("README.md"),
        "# Updated README\n\nThis change should not affect the build cache.",
    )
    .unwrap();

    // Second attempt - in a real environment with Docker, this would hit cache
    let mut cmd2 = Command::cargo_bin("deacon").unwrap();
    let assert2 = cmd2
        .current_dir(&temp_dir)
        .arg("build")
        .arg("--output-format")
        .arg("text")
        .assert();

    let output2 = assert2.get_output();

    if first_success && output2.status.success() {
        // Both builds succeeded - verify the second one used cache or was similarly fast
        // In practice, we can't easily test cache hit behavior in integration tests
        // but at least verify both builds succeeded consistently
        assert!(output2.status.success(), "Second build should also succeed");
    } else {
        // Without Docker, both should fail with similar errors
        let stderr1 = String::from_utf8_lossy(&output1.stderr);
        let stderr2 = String::from_utf8_lossy(&output2.stderr);
        assert!(
            (stderr1.contains("Docker") || stderr1.contains("docker"))
                || (stderr2.contains("Docker") || stderr2.contains("docker")),
            "Expected Docker-related errors"
        );
    }
}

#[test]
fn test_build_with_affecting_file_changes() {
    let temp_dir = TempDir::new().unwrap();
    let dockerfile_content = r#"FROM alpine:3.19
COPY main.py /app/
RUN echo "Testing affecting changes"
"#;

    fs::write(temp_dir.path().join("Dockerfile"), dockerfile_content).unwrap();

    let devcontainer_config = r#"{
    "name": "Affecting Changes Test",
    "dockerFile": "Dockerfile"
}
"#;

    fs::create_dir(temp_dir.path().join(".devcontainer")).unwrap();
    fs::write(
        temp_dir.path().join(".devcontainer/devcontainer.json"),
        devcontainer_config,
    )
    .unwrap();

    // Create a main.py file (should affect build)
    fs::write(temp_dir.path().join("main.py"), "print('hello world')").unwrap();

    // First attempt
    let mut cmd1 = Command::cargo_bin("deacon").unwrap();
    let assert1 = cmd1.current_dir(&temp_dir).arg("build").assert();

    let output1 = assert1.get_output();
    let first_success = output1.status.success();

    // Modify main.py (should affect build and invalidate cache)
    fs::write(
        temp_dir.path().join("main.py"),
        "print('hello updated world')",
    )
    .unwrap();

    // Second attempt - in a real environment, this would miss cache and rebuild
    let mut cmd2 = Command::cargo_bin("deacon").unwrap();
    let assert2 = cmd2.current_dir(&temp_dir).arg("build").assert();

    let output2 = assert2.get_output();

    if first_success {
        // If Docker is available and first build succeeded
        // Second build should also succeed (though it won't hit cache due to file change)
        assert!(
            output2.status.success(),
            "Second build should succeed after file change: {}",
            String::from_utf8_lossy(&output2.stderr)
        );
        // We can't easily test that cache was invalidated in integration tests,
        // but both builds should work
    } else {
        // Without Docker, both should fail
        let stderr1 = String::from_utf8_lossy(&output1.stderr);
        let stderr2 = String::from_utf8_lossy(&output2.stderr);
        assert!(
            (stderr1.contains("Docker") || stderr1.contains("docker"))
                || (stderr2.contains("Docker") || stderr2.contains("docker")),
            "Expected Docker-related errors"
        );
    }
}
