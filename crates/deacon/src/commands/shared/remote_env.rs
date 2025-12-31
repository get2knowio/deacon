use anyhow::Result;
use deacon_core::errors::DeaconError;

/// Parsed and normalized remote environment variable from CLI input.
///
/// Validates env var entries provided via `--remote-env` / `--env`.
#[derive(Debug, Clone, PartialEq)]
pub struct NormalizedRemoteEnv {
    pub name: String,
    pub value: String,
}

impl NormalizedRemoteEnv {
    /// Parse and validate a remote env string from CLI.
    ///
    /// Expected format: `NAME=value`
    ///
    /// Returns error if format is invalid (missing =).
    pub fn parse(env_str: &str) -> Result<Self> {
        let parts: Vec<&str> = env_str.splitn(2, '=').collect();

        if parts.len() != 2 {
            return Err(
                DeaconError::Config(deacon_core::errors::ConfigError::Validation {
                    message: format!(
                        "Invalid remote-env format: '{}'. Expected: NAME=value",
                        env_str
                    ),
                })
                .into(),
            );
        }

        Ok(Self {
            name: parts[0].to_string(),
            value: parts[1].to_string(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_valid_remote_env() {
        let env = NormalizedRemoteEnv::parse("FOO=bar").unwrap();
        assert_eq!(env.name, "FOO");
        assert_eq!(env.value, "bar");
    }

    #[test]
    fn preserves_empty_value() {
        let env = NormalizedRemoteEnv::parse("EMPTY=").unwrap();
        assert_eq!(env.name, "EMPTY");
        assert_eq!(env.value, "");
    }

    #[test]
    fn rejects_missing_equals() {
        let err = NormalizedRemoteEnv::parse("INVALID").unwrap_err();
        assert!(
            err.downcast_ref::<DeaconError>().is_some(),
            "expected DeaconError validation failure, got {}",
            err
        );
    }
}
