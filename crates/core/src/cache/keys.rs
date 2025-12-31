//! Cache key types for different domains

use serde::{Deserialize, Serialize};
use std::fmt::Debug;
use std::path::PathBuf;

/// Cache key for feature-related data
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct FeatureCacheKey {
    /// Feature identifier (e.g., "ghcr.io/devcontainers/features/node")
    pub feature_id: String,
    /// Feature version (e.g., "1.0.0" or "latest")
    pub version: String,
    /// Optional options hash for parameterized features
    pub options_hash: Option<String>,
}

impl FeatureCacheKey {
    pub fn new(feature_id: String, version: String, options_hash: Option<String>) -> Self {
        Self {
            feature_id,
            version,
            options_hash,
        }
    }
}

/// Cache key for configuration parsing results
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ConfigCacheKey {
    /// Path to the configuration file
    pub config_path: PathBuf,
    /// Last modified time as seconds since epoch
    pub last_modified: u64,
    /// File size in bytes for additional validation
    pub file_size: u64,
}

impl ConfigCacheKey {
    pub fn new(config_path: PathBuf, last_modified: u64, file_size: u64) -> Self {
        Self {
            config_path,
            last_modified,
            file_size,
        }
    }
}

/// Cache key for environment probe results
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ProbeCacheKey {
    /// Type of probe (e.g., "docker_version", "system_info")
    pub probe_type: String,
    /// Optional target or context for the probe
    pub target: Option<String>,
    /// Probe parameters hash for differentiation
    pub params_hash: Option<String>,
}

impl ProbeCacheKey {
    pub fn new(probe_type: String, target: Option<String>, params_hash: Option<String>) -> Self {
        Self {
            probe_type,
            target,
            params_hash,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn test_feature_cache_key() {
        let key1 = FeatureCacheKey::new(
            "ghcr.io/devcontainers/features/node".to_string(),
            "18".to_string(),
            Some("abc123".to_string()),
        );
        let key2 = FeatureCacheKey::new(
            "ghcr.io/devcontainers/features/node".to_string(),
            "18".to_string(),
            Some("abc123".to_string()),
        );
        let key3 = FeatureCacheKey::new(
            "ghcr.io/devcontainers/features/node".to_string(),
            "20".to_string(),
            Some("abc123".to_string()),
        );

        assert_eq!(key1, key2);
        assert_ne!(key1, key3);
    }

    #[test]
    fn test_config_cache_key() {
        let path = Path::new("/workspace/.devcontainer/devcontainer.json").to_path_buf();
        let key1 = ConfigCacheKey::new(path.clone(), 1234567890, 1024);
        let key2 = ConfigCacheKey::new(path.clone(), 1234567890, 1024);
        let key3 = ConfigCacheKey::new(path, 1234567891, 1024);

        assert_eq!(key1, key2);
        assert_ne!(key1, key3);
    }

    #[test]
    fn test_probe_cache_key() {
        let key1 = ProbeCacheKey::new(
            "docker_version".to_string(),
            Some("docker".to_string()),
            None,
        );
        let key2 = ProbeCacheKey::new(
            "docker_version".to_string(),
            Some("docker".to_string()),
            None,
        );
        let key3 = ProbeCacheKey::new("system_info".to_string(), Some("docker".to_string()), None);

        assert_eq!(key1, key2);
        assert_ne!(key1, key3);
    }
}
