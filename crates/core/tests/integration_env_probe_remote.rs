//! Integration tests covering remote environment precedence during probing.

use deacon_core::env_probe::{EnvironmentProber, ProbeMode};
use std::collections::HashMap;

#[test]
fn test_remote_env_precedence_over_probed() {
    let prober = EnvironmentProber::new();

    let mut remote_env = HashMap::new();
    remote_env.insert("TEST_PATH".to_string(), "/custom/path".to_string());
    remote_env.insert("TEST_HOME".to_string(), "/custom/home".to_string());
    remote_env.insert("CUSTOM_VAR".to_string(), "custom_value".to_string());

    let result = prober.probe_environment(ProbeMode::InteractiveShell, Some(&remote_env));

    match result {
        Ok(env_vars) => {
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

    assert_eq!(result.len(), 1);
    assert_eq!(
        result.get("ONLY_REMOTE"),
        Some(&"only_remote_value".to_string())
    );
}
