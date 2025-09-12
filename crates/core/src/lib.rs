//! Core library for the DevContainer CLI
//!
//! This crate contains shared logic for configuration resolution, Docker integration,
//! feature system, template system, lifecycle execution, logging, and error handling.

pub mod cache;
pub mod compose;
pub mod config;
pub mod container;
pub mod container_lifecycle;
pub mod docker;
pub mod doctor;
pub mod dotfiles;
pub mod env_probe;
pub mod errors;
pub mod feature_installer;
pub mod features;
pub mod lifecycle;
pub mod logging;
pub mod mount;
pub mod oci;
pub mod ports;
pub mod redaction;
pub mod retry;
pub mod state;

#[cfg(feature = "plugins")]
pub mod plugins;
pub mod templates;
pub mod user_mapping;
pub mod variable;

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
