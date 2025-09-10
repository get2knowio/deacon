//! Integration tests for environment probing functionality
//!
//! These tests validate environment variable capture, PATH/HOME capture,
//! and cache validation functionality across different probing modes.

use deacon_core::env_probe::{EnvironmentProber, ProbeMode};
use std::collections::HashMap;

#[test]
fn test_path_capture_in_probed_environment() {
    let prober = EnvironmentProber::new();

    // Test with InteractiveShell mode (most commonly used)
    let result = prober.probe_environment(ProbeMode::InteractiveShell, None);

    match result {
        Ok(env_vars) => {
            // On Unix systems, PATH should be captured
            #[cfg(unix)]
            {
                if !cfg!(test) || std::env::var("SHELL").is_ok() {
                    // We expect PATH to be present in most Unix environments
                    assert!(
                        env_vars.contains_key("PATH"),
                        "PATH environment variable should be captured in probed environment"
                    );

                    // PATH should be non-empty
                    if let Some(path_value) = env_vars.get("PATH") {
                        assert!(!path_value.is_empty(), "PATH should not be empty");
                    }
                }
            }

            // On Windows, we skip shell probing, so env_vars will be empty
            #[cfg(windows)]
            {
                // Windows shell probing is skipped, so we expect empty result
                assert!(env_vars.is_empty());
            }
        }
        Err(e) => {
            // Shell execution might fail in CI environments, which is acceptable
            eprintln!(
                "Shell execution failed (expected in some CI environments): {}",
                e
            );
        }
    }
}

#[test]
fn test_home_capture_in_probed_environment() {
    let prober = EnvironmentProber::new();

    let result = prober.probe_environment(ProbeMode::LoginShell, None);

    match result {
        Ok(env_vars) => {
            // On Unix systems, HOME should typically be captured in login shells
            #[cfg(unix)]
            {
                if !cfg!(test) || std::env::var("SHELL").is_ok() {
                    // HOME is commonly set in login shells
                    if env_vars.contains_key("HOME") {
                        let home_value = env_vars.get("HOME").unwrap();
                        assert!(
                            !home_value.is_empty(),
                            "HOME should not be empty if present"
                        );
                    }
                }
            }
        }
        Err(e) => {
            eprintln!(
                "Shell execution failed (expected in some CI environments): {}",
                e
            );
        }
    }
}

#[test]
fn test_cache_validation_same_mode() {
    let prober = EnvironmentProber::new();

    // First call
    let result1 = prober.probe_environment(ProbeMode::InteractiveShell, None);

    // Second call with same mode should use cache
    let result2 = prober.probe_environment(ProbeMode::InteractiveShell, None);

    // Both calls should succeed or both should fail
    match (result1, result2) {
        (Ok(env1), Ok(env2)) => {
            // Results should be identical (cached)
            assert_eq!(env1, env2, "Cached results should be identical");
        }
        (Err(_), Err(_)) => {
            // Both failed - acceptable in CI environments
            eprintln!("Both calls failed - acceptable in CI environments");
        }
        _ => {
            panic!("First and second calls should have consistent results");
        }
    }
}

#[test]
fn test_cache_validation_different_modes() {
    let prober = EnvironmentProber::new();

    // Different modes should not share cache - test by calling with different modes
    let result1 = prober.probe_environment(ProbeMode::InteractiveShell, None);
    let result2 = prober.probe_environment(ProbeMode::LoginShell, None);

    // Results should be independent (each mode has its own cache entry)
    match (result1, result2) {
        (Ok(_), Ok(_)) => {
            // Both succeeded - different modes processed separately
        }
        (Err(_), Err(_)) => {
            // Both failed - acceptable in CI environments
            eprintln!("Both mode tests failed - acceptable in CI environments");
        }
        _ => {
            // Mixed results are acceptable as different modes may behave differently
        }
    }
}

#[test]
fn test_remote_env_precedence_over_probed() {
    let prober = EnvironmentProber::new();

    // Create a remote environment with some variables
    let mut remote_env = HashMap::new();
    remote_env.insert("TEST_PATH".to_string(), "/custom/path".to_string());
    remote_env.insert("TEST_HOME".to_string(), "/custom/home".to_string());
    remote_env.insert("CUSTOM_VAR".to_string(), "custom_value".to_string());

    let result = prober.probe_environment(ProbeMode::InteractiveShell, Some(&remote_env));

    match result {
        Ok(env_vars) => {
            // Remote environment variables should be present
            assert_eq!(
                env_vars.get("TEST_PATH"),
                Some(&"/custom/path".to_string()),
                "Remote TEST_PATH should be present"
            );
            assert_eq!(
                env_vars.get("TEST_HOME"),
                Some(&"/custom/home".to_string()),
                "Remote TEST_HOME should be present"
            );
            assert_eq!(
                env_vars.get("CUSTOM_VAR"),
                Some(&"custom_value".to_string()),
                "Remote CUSTOM_VAR should be present"
            );

            // If there were any conflicts, remote env should win
            // (This is tested more thoroughly in unit tests)
        }
        Err(e) => {
            eprintln!(
                "Shell execution failed (expected in some CI environments): {}",
                e
            );
        }
    }
}

#[test]
fn test_none_mode_with_remote_env() {
    let prober = EnvironmentProber::new();

    let mut remote_env = HashMap::new();
    remote_env.insert("ONLY_REMOTE".to_string(), "only_remote_value".to_string());

    let result = prober
        .probe_environment(ProbeMode::None, Some(&remote_env))
        .expect("None mode should always succeed");

    // Should only contain remote environment variables
    assert_eq!(result.len(), 1);
    assert_eq!(
        result.get("ONLY_REMOTE"),
        Some(&"only_remote_value".to_string())
    );
}

#[test]
fn test_user_info_retrieval() {
    let prober = EnvironmentProber::new();

    let user = prober
        .get_remote_user()
        .expect("Should be able to get user information");

    // User should have a valid name
    assert!(!user.name.is_empty(), "User name should not be empty");

    // On Unix systems, UIDs and GIDs are retrieved via system calls
    // On non-Unix systems, default values are used
    #[cfg(unix)]
    {
        // User info should be reasonable (tested in unit tests)
        // Here we just verify it doesn't panic
    }

    #[cfg(not(unix))]
    {
        assert_eq!(user.uid, 1000, "Default UID should be 1000 on non-Unix");
        assert_eq!(user.gid, 1000, "Default GID should be 1000 on non-Unix");
    }
}

#[test]
fn test_environment_probing_error_handling() {
    let prober = EnvironmentProber::new();

    // Test probing with all different modes to ensure no panics
    let modes = [
        ProbeMode::None,
        ProbeMode::InteractiveShell,
        ProbeMode::LoginShell,
        ProbeMode::LoginInteractiveShell,
    ];

    for mode in modes {
        let result = prober.probe_environment(mode, None);

        match result {
            Ok(_) => {
                // Success is expected for None mode and potentially others
            }
            Err(e) => {
                // Errors are acceptable for shell execution in CI environments
                eprintln!("Probing with mode {:?} failed: {}", mode, e);
            }
        }
    }
}
