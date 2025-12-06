//! Integration tests for up command build options propagation
//!
//! These tests verify that cache-from, cache-to, and builder options are correctly
//! parsed from CLI arguments and propagated through the BuildOptions struct.
//!
//! Per spec (specs/007-up-build-parity/spec.md):
//! - FR-001: Up MUST apply user-specified BuildKit cache-from and cache-to options
//!   to both Dockerfile builds and feature builds consistently.
//! - FR-002: Up MUST honor build executor selection (buildx/builder) for builds.
//! - FR-003: Up MUST leave build behavior unchanged when no BuildKit or cache options
//!   are provided, avoiding implicit defaults that alter build outputs.

use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use tempfile::TempDir;

// =============================================================================
// Unit-style tests for BuildOptions construction and Docker arg generation
// =============================================================================

/// Test that BuildOptions with cache-from generates correct docker arguments
#[test]
fn test_build_options_generates_cache_from_args() {
    use deacon_core::build::BuildOptions;

    let options = BuildOptions {
        no_cache: false,
        cache_from: vec![
            "type=registry,ref=myregistry.io/cache:latest".to_string(),
            "type=local,src=/tmp/cache".to_string(),
        ],
        cache_to: None,
        builder: None,
    };

    let args = options.to_docker_args();

    // Should generate --cache-from for each entry
    assert_eq!(args.len(), 4);
    assert_eq!(args[0], "--cache-from");
    assert_eq!(args[1], "type=registry,ref=myregistry.io/cache:latest");
    assert_eq!(args[2], "--cache-from");
    assert_eq!(args[3], "type=local,src=/tmp/cache");
}

/// Test that BuildOptions with cache-to generates correct docker arguments
#[test]
fn test_build_options_generates_cache_to_args() {
    use deacon_core::build::BuildOptions;

    let options = BuildOptions {
        no_cache: false,
        cache_from: vec![],
        cache_to: Some("type=registry,ref=myregistry.io/cache:build".to_string()),
        builder: None,
    };

    let args = options.to_docker_args();

    assert_eq!(args.len(), 2);
    assert_eq!(args[0], "--cache-to");
    assert_eq!(args[1], "type=registry,ref=myregistry.io/cache:build");
}

/// Test that BuildOptions with builder generates correct docker arguments
#[test]
fn test_build_options_generates_builder_args() {
    use deacon_core::build::BuildOptions;

    let options = BuildOptions {
        no_cache: false,
        cache_from: vec![],
        cache_to: None,
        builder: Some("mybuilder".to_string()),
    };

    let args = options.to_docker_args();

    assert_eq!(args.len(), 2);
    assert_eq!(args[0], "--builder");
    assert_eq!(args[1], "mybuilder");
}

/// Test that BuildOptions with all cache options generates correct docker arguments
#[test]
fn test_build_options_generates_all_cache_args() {
    use deacon_core::build::BuildOptions;

    let options = BuildOptions {
        no_cache: true,
        cache_from: vec!["type=registry,ref=cache:latest".to_string()],
        cache_to: Some("type=registry,ref=cache:build".to_string()),
        builder: Some("cloud-builder".to_string()),
    };

    let args = options.to_docker_args();

    // Should include: --no-cache, --cache-from <val>, --cache-to <val>, --builder <val>
    assert_eq!(args.len(), 7);
    assert_eq!(args[0], "--no-cache");
    assert_eq!(args[1], "--cache-from");
    assert_eq!(args[2], "type=registry,ref=cache:latest");
    assert_eq!(args[3], "--cache-to");
    assert_eq!(args[4], "type=registry,ref=cache:build");
    assert_eq!(args[5], "--builder");
    assert_eq!(args[6], "cloud-builder");
}

/// Test that BuildOptions::default() generates no extra args (FR-003)
#[test]
fn test_build_options_default_no_args() {
    use deacon_core::build::BuildOptions;

    let options = BuildOptions::default();

    // Per FR-003: defaults should not inject any cache or builder settings
    let args = options.to_docker_args();
    assert!(
        args.is_empty(),
        "BuildOptions::default() should generate no docker args, got: {:?}",
        args
    );
}

/// Test that BuildOptions::default() has correct field values
#[test]
fn test_build_options_default_field_values() {
    use deacon_core::build::BuildOptions;

    let options = BuildOptions::default();

    assert!(!options.no_cache, "default no_cache should be false");
    assert!(
        options.cache_from.is_empty(),
        "default cache_from should be empty"
    );
    assert!(
        options.cache_to.is_none(),
        "default cache_to should be None"
    );
    assert!(options.builder.is_none(), "default builder should be None");
    assert!(
        options.is_default(),
        "BuildOptions::default().is_default() should return true"
    );
}

// =============================================================================
// Unit-style tests for BuildKitOptions detection
// =============================================================================

/// Test BuildKitOptions requires_buildkit detection for cache-from
#[test]
fn test_buildkit_options_detection_cache_from() {
    use deacon_core::build::buildkit::BuildKitOptions;

    let options = BuildKitOptions {
        cache_from: vec!["type=registry,ref=test".to_string()],
        cache_to: vec![],
        builder: None,
    };

    assert!(
        options.requires_buildkit(),
        "cache_from should require BuildKit"
    );
    assert_eq!(
        options.buildkit_required_options(),
        vec!["--cache-from"],
        "should report --cache-from as requiring BuildKit"
    );
}

/// Test BuildKitOptions requires_buildkit detection for cache-to
#[test]
fn test_buildkit_options_detection_cache_to() {
    use deacon_core::build::buildkit::BuildKitOptions;

    let options = BuildKitOptions {
        cache_from: vec![],
        cache_to: vec!["type=registry,ref=test".to_string()],
        builder: None,
    };

    assert!(
        options.requires_buildkit(),
        "cache_to should require BuildKit"
    );
    assert_eq!(
        options.buildkit_required_options(),
        vec!["--cache-to"],
        "should report --cache-to as requiring BuildKit"
    );
}

/// Test BuildKitOptions requires_buildkit detection for builder
#[test]
fn test_buildkit_options_detection_builder() {
    use deacon_core::build::buildkit::BuildKitOptions;

    let options = BuildKitOptions {
        cache_from: vec![],
        cache_to: vec![],
        builder: Some("mybuilder".to_string()),
    };

    assert!(
        options.requires_buildkit(),
        "builder should require BuildKit"
    );
    assert_eq!(
        options.buildkit_required_options(),
        vec!["--builder"],
        "should report --builder as requiring BuildKit"
    );
}

/// Test BuildKitOptions requires_buildkit detection for combined options
#[test]
fn test_buildkit_options_detection_combined() {
    use deacon_core::build::buildkit::BuildKitOptions;

    let options = BuildKitOptions {
        cache_from: vec!["type=registry,ref=from".to_string()],
        cache_to: vec!["type=registry,ref=to".to_string()],
        builder: Some("mybuilder".to_string()),
    };

    assert!(options.requires_buildkit());
    let required = options.buildkit_required_options();
    assert_eq!(required.len(), 3);
    assert!(required.contains(&"--cache-from"));
    assert!(required.contains(&"--cache-to"));
    assert!(required.contains(&"--builder"));
}

/// Test BuildKitOptions does not require buildkit when empty
#[test]
fn test_buildkit_options_detection_empty() {
    use deacon_core::build::buildkit::BuildKitOptions;

    let options = BuildKitOptions::default();

    assert!(
        !options.requires_buildkit(),
        "empty options should not require BuildKit"
    );
    assert!(
        options.buildkit_required_options().is_empty(),
        "empty options should report no BuildKit requirements"
    );
}

/// Test that require_buildkit_for_options passes when no options require BuildKit
#[test]
fn test_require_buildkit_for_options_passes_when_empty() {
    use deacon_core::build::buildkit::{require_buildkit_for_options, BuildKitOptions};

    let options = BuildKitOptions::default();
    let result = require_buildkit_for_options(&options);

    // Should always pass when no BuildKit-requiring options are set
    assert!(
        result.is_ok(),
        "require_buildkit_for_options should pass for default options"
    );
}

// =============================================================================
// BuildOptions requires_buildkit method tests
// =============================================================================

/// Test BuildOptions requires_buildkit for cache_from
#[test]
fn test_build_options_requires_buildkit_cache_from() {
    use deacon_core::build::BuildOptions;

    let options = BuildOptions {
        cache_from: vec!["type=registry,ref=test".to_string()],
        ..Default::default()
    };

    assert!(options.requires_buildkit());
    assert!(!options.is_default());
}

/// Test BuildOptions requires_buildkit for cache_to
#[test]
fn test_build_options_requires_buildkit_cache_to() {
    use deacon_core::build::BuildOptions;

    let options = BuildOptions {
        cache_to: Some("type=registry,ref=test".to_string()),
        ..Default::default()
    };

    assert!(options.requires_buildkit());
    assert!(!options.is_default());
}

/// Test BuildOptions requires_buildkit for builder
#[test]
fn test_build_options_requires_buildkit_builder() {
    use deacon_core::build::BuildOptions;

    let options = BuildOptions {
        builder: Some("mybuilder".to_string()),
        ..Default::default()
    };

    assert!(options.requires_buildkit());
    assert!(!options.is_default());
}

/// Test BuildOptions no_cache does not require buildkit by itself
#[test]
fn test_build_options_no_cache_does_not_require_buildkit() {
    use deacon_core::build::BuildOptions;

    let options = BuildOptions {
        no_cache: true,
        ..Default::default()
    };

    // no_cache alone doesn't require BuildKit - legacy docker build supports it
    assert!(!options.requires_buildkit());
    // But it's not default behavior
    assert!(!options.is_default());
}

// =============================================================================
// CLI integration tests for argument parsing
// =============================================================================

/// Test that up command accepts --cache-from flag
#[test]
fn up_accepts_cache_from_flag() {
    let temp_dir = TempDir::new().unwrap();
    let devcontainer_config = r#"{
        "name": "Test Container",
        "image": "alpine:3.19"
    }"#;

    fs::create_dir(temp_dir.path().join(".devcontainer")).unwrap();
    fs::write(
        temp_dir.path().join(".devcontainer/devcontainer.json"),
        devcontainer_config,
    )
    .unwrap();

    let mut cmd = Command::cargo_bin("deacon").unwrap();
    let assert = cmd
        .current_dir(&temp_dir)
        .arg("up")
        .arg("--cache-from")
        .arg("type=registry,ref=myregistry.io/cache:latest")
        .arg("--cache-from")
        .arg("type=local,src=/tmp/cache")
        .assert();

    let output = assert.get_output();
    // The command may fail due to Docker not being available, but it should NOT fail
    // due to argument parsing errors
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        // Should not be a clap/argument parsing error
        assert!(
            !stderr.contains("error: unexpected argument")
                && !stderr.contains("error: invalid value"),
            "Argument parsing should succeed; stderr: {}",
            stderr
        );
    }
}

/// Test that up command accepts --cache-to flag
#[test]
fn up_accepts_cache_to_flag() {
    let temp_dir = TempDir::new().unwrap();
    let devcontainer_config = r#"{
        "name": "Test Container",
        "image": "alpine:3.19"
    }"#;

    fs::create_dir(temp_dir.path().join(".devcontainer")).unwrap();
    fs::write(
        temp_dir.path().join(".devcontainer/devcontainer.json"),
        devcontainer_config,
    )
    .unwrap();

    let mut cmd = Command::cargo_bin("deacon").unwrap();
    let assert = cmd
        .current_dir(&temp_dir)
        .arg("up")
        .arg("--cache-to")
        .arg("type=registry,ref=myregistry.io/cache:build")
        .assert();

    let output = assert.get_output();
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            !stderr.contains("error: unexpected argument")
                && !stderr.contains("error: invalid value"),
            "Argument parsing should succeed; stderr: {}",
            stderr
        );
    }
}

/// Test that up command accepts --buildkit flag
#[test]
fn up_accepts_buildkit_flag() {
    let temp_dir = TempDir::new().unwrap();
    let devcontainer_config = r#"{
        "name": "Test Container",
        "image": "alpine:3.19"
    }"#;

    fs::create_dir(temp_dir.path().join(".devcontainer")).unwrap();
    fs::write(
        temp_dir.path().join(".devcontainer/devcontainer.json"),
        devcontainer_config,
    )
    .unwrap();

    // Test with 'auto' value
    let mut cmd = Command::cargo_bin("deacon").unwrap();
    let assert = cmd
        .current_dir(&temp_dir)
        .arg("up")
        .arg("--buildkit")
        .arg("auto")
        .assert();

    let output = assert.get_output();
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            !stderr.contains("error: unexpected argument")
                && !stderr.contains("error: invalid value"),
            "Argument parsing for --buildkit auto should succeed; stderr: {}",
            stderr
        );
    }

    // Test with 'never' value
    let mut cmd2 = Command::cargo_bin("deacon").unwrap();
    let assert2 = cmd2
        .current_dir(&temp_dir)
        .arg("up")
        .arg("--buildkit")
        .arg("never")
        .assert();

    let output2 = assert2.get_output();
    if !output2.status.success() {
        let stderr2 = String::from_utf8_lossy(&output2.stderr);
        assert!(
            !stderr2.contains("error: unexpected argument")
                && !stderr2.contains("error: invalid value"),
            "Argument parsing for --buildkit never should succeed; stderr: {}",
            stderr2
        );
    }
}

/// Test that up command accepts combined build options
#[test]
fn up_accepts_combined_build_options() {
    let temp_dir = TempDir::new().unwrap();
    let devcontainer_config = r#"{
        "name": "Test Container",
        "image": "alpine:3.19"
    }"#;

    fs::create_dir(temp_dir.path().join(".devcontainer")).unwrap();
    fs::write(
        temp_dir.path().join(".devcontainer/devcontainer.json"),
        devcontainer_config,
    )
    .unwrap();

    let mut cmd = Command::cargo_bin("deacon").unwrap();
    let assert = cmd
        .current_dir(&temp_dir)
        .arg("up")
        .arg("--cache-from")
        .arg("type=registry,ref=cache:latest")
        .arg("--cache-to")
        .arg("type=registry,ref=cache:build")
        .arg("--buildkit")
        .arg("auto")
        .arg("--build-no-cache")
        .assert();

    let output = assert.get_output();
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            !stderr.contains("error: unexpected argument")
                && !stderr.contains("error: invalid value"),
            "Combined build options should parse successfully; stderr: {}",
            stderr
        );
    }
}

/// Test that up --help includes cache-from, cache-to, and buildkit options
#[test]
fn up_help_includes_build_options() {
    let mut cmd = Command::cargo_bin("deacon").unwrap();
    cmd.arg("up")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("--cache-from"))
        .stdout(predicate::str::contains("--cache-to"))
        .stdout(predicate::str::contains("--buildkit"))
        .stdout(predicate::str::contains("--build-no-cache"));
}

// =============================================================================
// Default behavior verification tests (FR-003)
// =============================================================================

/// Test that default BuildOptions does not require BuildKit features.
/// This ensures no implicit BuildKit requirements when options are absent.
#[test]
fn default_build_options_does_not_require_buildkit() {
    use deacon_core::build::BuildOptions;

    let options = BuildOptions::default();

    // Per FR-003: default options must not trigger BuildKit requirements
    assert!(
        !options.requires_buildkit(),
        "Default BuildOptions should not require BuildKit"
    );
    assert!(
        options.to_docker_args().is_empty(),
        "Default BuildOptions should produce no docker args"
    );
}

// =============================================================================
// Cache order preservation tests
// =============================================================================

/// Test that cache-from order is preserved in docker args
#[test]
fn test_build_options_preserves_cache_from_order() {
    use deacon_core::build::BuildOptions;

    let options = BuildOptions {
        cache_from: vec![
            "first".to_string(),
            "second".to_string(),
            "third".to_string(),
        ],
        ..Default::default()
    };

    let args = options.to_docker_args();

    // Verify order is preserved: --cache-from first --cache-from second --cache-from third
    assert_eq!(args[0], "--cache-from");
    assert_eq!(args[1], "first");
    assert_eq!(args[2], "--cache-from");
    assert_eq!(args[3], "second");
    assert_eq!(args[4], "--cache-from");
    assert_eq!(args[5], "third");
}

/// Test that values are preserved exactly as provided
#[test]
fn test_build_options_preserves_exact_values() {
    use deacon_core::build::BuildOptions;

    // Realistic cache spec values
    let options = BuildOptions {
        cache_from: vec!["type=gha,scope=main".to_string()],
        cache_to: Some("type=gha,scope=main,mode=max".to_string()),
        ..Default::default()
    };

    let args = options.to_docker_args();

    assert!(args.contains(&"type=gha,scope=main".to_string()));
    assert!(args.contains(&"type=gha,scope=main,mode=max".to_string()));
}

// =============================================================================
// Cache Warning Behavior Tests (T024)
//
// Per spec edge case: "Cache-from or cache-to sources that are unreachable
// should degrade gracefully with clear warnings while allowing the build to
// proceed without cached layers."
//
// The actual warning detection logic is tested in docker.rs unit tests.
// These integration tests verify the CLI contract and document expected behavior.
// =============================================================================

/// Verify that cache-from with invalid/unreachable endpoints does not cause
/// argument parsing to fail. The build should proceed (and warn at runtime).
#[test]
fn cache_from_unreachable_endpoint_accepted_by_cli() {
    let temp_dir = TempDir::new().unwrap();
    let devcontainer_config = r#"{
        "name": "Test Container",
        "image": "alpine:3.19"
    }"#;

    fs::create_dir(temp_dir.path().join(".devcontainer")).unwrap();
    fs::write(
        temp_dir.path().join(".devcontainer/devcontainer.json"),
        devcontainer_config,
    )
    .unwrap();

    // Use a clearly unreachable endpoint - CLI should accept this
    // (warnings are emitted at build time, not argument parsing time)
    let mut cmd = Command::cargo_bin("deacon").unwrap();
    let assert = cmd
        .current_dir(&temp_dir)
        .arg("up")
        .arg("--cache-from")
        .arg("type=registry,ref=nonexistent.invalid/cache:latest")
        .assert();

    let output = assert.get_output();
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        // Should not fail due to argument validation of the cache endpoint
        assert!(
            !stderr.contains("error: invalid value for '--cache-from'"),
            "CLI should accept unreachable cache endpoints; stderr: {}",
            stderr
        );
    }
}

/// Verify that cache-to with invalid/unreachable endpoints does not cause
/// argument parsing to fail. The build should proceed (and warn at runtime).
#[test]
fn cache_to_unreachable_endpoint_accepted_by_cli() {
    let temp_dir = TempDir::new().unwrap();
    let devcontainer_config = r#"{
        "name": "Test Container",
        "image": "alpine:3.19"
    }"#;

    fs::create_dir(temp_dir.path().join(".devcontainer")).unwrap();
    fs::write(
        temp_dir.path().join(".devcontainer/devcontainer.json"),
        devcontainer_config,
    )
    .unwrap();

    // Use a clearly unreachable endpoint - CLI should accept this
    let mut cmd = Command::cargo_bin("deacon").unwrap();
    let assert = cmd
        .current_dir(&temp_dir)
        .arg("up")
        .arg("--cache-to")
        .arg("type=registry,ref=nonexistent.invalid/cache:build")
        .assert();

    let output = assert.get_output();
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        // Should not fail due to argument validation of the cache endpoint
        assert!(
            !stderr.contains("error: invalid value for '--cache-to'"),
            "CLI should accept unreachable cache endpoints; stderr: {}",
            stderr
        );
    }
}

/// Verify that multiple cache sources with mixed valid/invalid endpoints are accepted.
/// This tests the warn-and-continue behavior where some sources may fail.
#[test]
fn mixed_cache_sources_accepted_by_cli() {
    let temp_dir = TempDir::new().unwrap();
    let devcontainer_config = r#"{
        "name": "Test Container",
        "image": "alpine:3.19"
    }"#;

    fs::create_dir(temp_dir.path().join(".devcontainer")).unwrap();
    fs::write(
        temp_dir.path().join(".devcontainer/devcontainer.json"),
        devcontainer_config,
    )
    .unwrap();

    // Mix of endpoints - some may be unreachable
    let mut cmd = Command::cargo_bin("deacon").unwrap();
    let assert = cmd
        .current_dir(&temp_dir)
        .arg("up")
        .arg("--cache-from")
        .arg("type=registry,ref=ghcr.io/org/cache:latest")
        .arg("--cache-from")
        .arg("type=registry,ref=nonexistent.invalid/cache:backup")
        .arg("--cache-from")
        .arg("type=local,src=/nonexistent/cache/path")
        .assert();

    let output = assert.get_output();
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        // Should not fail due to argument validation
        assert!(
            !stderr.contains("error: invalid value")
                && !stderr.contains("error: unexpected argument"),
            "CLI should accept mixed cache sources; stderr: {}",
            stderr
        );
    }
}

/// Verify that GitHub Actions cache type is accepted (common CI pattern).
#[test]
fn github_actions_cache_type_accepted() {
    let temp_dir = TempDir::new().unwrap();
    let devcontainer_config = r#"{
        "name": "Test Container",
        "image": "alpine:3.19"
    }"#;

    fs::create_dir(temp_dir.path().join(".devcontainer")).unwrap();
    fs::write(
        temp_dir.path().join(".devcontainer/devcontainer.json"),
        devcontainer_config,
    )
    .unwrap();

    // GitHub Actions cache (type=gha) is commonly used in CI
    let mut cmd = Command::cargo_bin("deacon").unwrap();
    let assert = cmd
        .current_dir(&temp_dir)
        .arg("up")
        .arg("--cache-from")
        .arg("type=gha,scope=main")
        .arg("--cache-to")
        .arg("type=gha,scope=main,mode=max")
        .assert();

    let output = assert.get_output();
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            !stderr.contains("error: invalid value")
                && !stderr.contains("error: unexpected argument"),
            "CLI should accept type=gha cache; stderr: {}",
            stderr
        );
    }
}

/// Verify that local cache type is accepted.
#[test]
fn local_cache_type_accepted() {
    let temp_dir = TempDir::new().unwrap();
    let devcontainer_config = r#"{
        "name": "Test Container",
        "image": "alpine:3.19"
    }"#;

    fs::create_dir(temp_dir.path().join(".devcontainer")).unwrap();
    fs::write(
        temp_dir.path().join(".devcontainer/devcontainer.json"),
        devcontainer_config,
    )
    .unwrap();

    // Local cache directories
    let mut cmd = Command::cargo_bin("deacon").unwrap();
    let assert = cmd
        .current_dir(&temp_dir)
        .arg("up")
        .arg("--cache-from")
        .arg("type=local,src=/tmp/buildx-cache")
        .arg("--cache-to")
        .arg("type=local,dest=/tmp/buildx-cache-out")
        .assert();

    let output = assert.get_output();
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            !stderr.contains("error: invalid value")
                && !stderr.contains("error: unexpected argument"),
            "CLI should accept type=local cache; stderr: {}",
            stderr
        );
    }
}

/// Verify S3 cache type is accepted.
#[test]
fn s3_cache_type_accepted() {
    let temp_dir = TempDir::new().unwrap();
    let devcontainer_config = r#"{
        "name": "Test Container",
        "image": "alpine:3.19"
    }"#;

    fs::create_dir(temp_dir.path().join(".devcontainer")).unwrap();
    fs::write(
        temp_dir.path().join(".devcontainer/devcontainer.json"),
        devcontainer_config,
    )
    .unwrap();

    let mut cmd = Command::cargo_bin("deacon").unwrap();
    let assert = cmd
        .current_dir(&temp_dir)
        .arg("up")
        .arg("--cache-from")
        .arg("type=s3,region=us-east-1,bucket=my-cache-bucket")
        .arg("--cache-to")
        .arg("type=s3,region=us-east-1,bucket=my-cache-bucket,mode=max")
        .assert();

    let output = assert.get_output();
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            !stderr.contains("error: invalid value")
                && !stderr.contains("error: unexpected argument"),
            "CLI should accept type=s3 cache; stderr: {}",
            stderr
        );
    }
}

/// Verify Azure Blob Storage cache type is accepted.
#[test]
fn azblob_cache_type_accepted() {
    let temp_dir = TempDir::new().unwrap();
    let devcontainer_config = r#"{
        "name": "Test Container",
        "image": "alpine:3.19"
    }"#;

    fs::create_dir(temp_dir.path().join(".devcontainer")).unwrap();
    fs::write(
        temp_dir.path().join(".devcontainer/devcontainer.json"),
        devcontainer_config,
    )
    .unwrap();

    let mut cmd = Command::cargo_bin("deacon").unwrap();
    let assert = cmd
        .current_dir(&temp_dir)
        .arg("up")
        .arg("--cache-from")
        .arg("type=azblob,account_url=https://myaccount.blob.core.windows.net")
        .arg("--cache-to")
        .arg("type=azblob,account_url=https://myaccount.blob.core.windows.net,mode=max")
        .assert();

    let output = assert.get_output();
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            !stderr.contains("error: invalid value")
                && !stderr.contains("error: unexpected argument"),
            "CLI should accept type=azblob cache; stderr: {}",
            stderr
        );
    }
}
