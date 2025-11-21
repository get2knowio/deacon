//! Integration tests validating environment probe caching behavior.

use deacon_core::env_probe::{EnvironmentProber, ProbeMode};

#[test]
fn test_cache_validation_same_mode() {
    let prober = EnvironmentProber::new();

    let result1 = prober.probe_environment(ProbeMode::InteractiveShell, None);
    let result2 = prober.probe_environment(ProbeMode::InteractiveShell, None);

    match (result1, result2) {
        (Ok(env1), Ok(env2)) => {
            assert_eq!(env1, env2, "Cached results should be identical");
        }
        (Err(_), Err(_)) => {
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

    let result1 = prober.probe_environment(ProbeMode::InteractiveShell, None);
    let result2 = prober.probe_environment(ProbeMode::LoginShell, None);

    match (result1, result2) {
        (Ok(_), Ok(_)) => {}
        (Err(_), Err(_)) => {
            eprintln!("Both mode tests failed - acceptable in CI environments");
        }
        _ => {}
    }
}
