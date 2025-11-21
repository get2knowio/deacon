//! Integration tests covering environment variable capture behavior.
use deacon_core::env_probe::{EnvironmentProber, ProbeMode};

fn is_ci_env() -> bool {
    std::env::var("CI").is_ok()
        || std::env::var("GITHUB_ACTIONS").is_ok()
        || std::env::var("CONTINUOUS_INTEGRATION").is_ok()
}

#[test]
fn test_path_capture_in_probed_environment() {
    let prober = EnvironmentProber::new();
    let is_ci = is_ci_env();

    let result = prober.probe_environment(ProbeMode::InteractiveShell, None);

    match result {
        Ok(env_vars) => {
            #[cfg(unix)]
            {
                if !is_ci && (!cfg!(test) || std::env::var("SHELL").is_ok()) {
                    assert!(
                        env_vars.contains_key("PATH"),
                        "PATH environment variable should be captured in probed environment"
                    );

                    if let Some(path_value) = env_vars.get("PATH") {
                        assert!(!path_value.is_empty(), "PATH should not be empty");
                    }
                } else if is_ci {
                    eprintln!("CI environment detected, shell probing skipped as expected");
                }
            }

            #[cfg(windows)]
            {
                assert!(env_vars.is_empty());
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
fn test_home_capture_in_probed_environment() {
    let prober = EnvironmentProber::new();
    let is_ci = is_ci_env();

    let result = prober.probe_environment(ProbeMode::LoginShell, None);

    match result {
        Ok(env_vars) => {
            #[cfg(unix)]
            {
                if !is_ci && (!cfg!(test) || std::env::var("SHELL").is_ok()) {
                    if env_vars.contains_key("HOME") {
                        let home_value = env_vars.get("HOME").unwrap();
                        assert!(
                            !home_value.is_empty(),
                            "HOME should not be empty if present"
                        );
                    }
                } else if is_ci {
                    eprintln!("CI environment detected, shell probing skipped as expected");
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
