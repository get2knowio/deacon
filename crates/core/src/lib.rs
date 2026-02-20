//! Core library for the DevContainer CLI
//!
//! This crate contains shared logic for configuration resolution, Docker integration,
//! feature system, template system, lifecycle execution, logging, and error handling.

pub mod build;
pub mod cache;
pub mod compose;
pub mod config;
pub mod container;
pub mod container_env_probe;
pub mod container_lifecycle;
pub mod docker;
pub mod dockerfile_generator;
pub mod doctor;
pub mod dotfiles;
pub mod entrypoint;
pub mod env_probe;
pub mod errors;
pub mod feature_installer;
pub mod feature_ref;
pub mod features;
pub mod gpu;
pub mod host_requirements;
pub mod io;
pub mod lifecycle;
pub mod lockfile;
pub mod logging;
pub mod mount;
pub mod observability;
pub mod oci;
/// Feature version tracking and outdated detection.
/// Provides functionality for checking feature versions against registries.
pub mod outdated;
pub mod platform;
pub mod plugins;
pub mod ports;
pub mod progress;
pub mod redaction;
pub mod registry_parser;
pub mod retry;
pub mod runtime;
pub mod secrets;
pub mod security;
pub mod semver_utils;
pub mod state;
pub mod templates;
pub mod text;
pub mod user_mapping;
pub mod variable;
pub mod workspace;

// Re-export IndexMap for use by dependent crates (preserves insertion order for ordered maps)
pub use indexmap::IndexMap;

/// Get the version of the core library
pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_version() {
        let version = version();
        assert!(!version.is_empty());
        assert!(version.contains('.'));
    }
}
