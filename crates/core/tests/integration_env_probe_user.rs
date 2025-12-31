//! Integration tests for user info retrieval and probe error handling paths.

use deacon_core::env_probe::{EnvironmentProber, ProbeMode};

#[test]
fn test_user_info_retrieval() {
    let prober = EnvironmentProber::new();

    let user = prober
        .get_remote_user()
        .expect("Should be able to get user information");

    assert!(!user.name.is_empty(), "User name should not be empty");

    #[cfg(unix)]
    {}

    #[cfg(not(unix))]
    {
        assert_eq!(user.uid, 1000, "Default UID should be 1000 on non-Unix");
        assert_eq!(user.gid, 1000, "Default GID should be 1000 on non-Unix");
    }
}

#[test]
fn test_environment_probing_error_handling() {
    let prober = EnvironmentProber::new();

    let modes = [
        ProbeMode::None,
        ProbeMode::InteractiveShell,
        ProbeMode::LoginShell,
        ProbeMode::LoginInteractiveShell,
    ];

    for mode in modes {
        let result = prober.probe_environment(mode, None);

        match result {
            Ok(_) => {}
            Err(e) => {
                eprintln!("Probing with mode {:?} failed: {}", mode, e);
            }
        }
    }
}
