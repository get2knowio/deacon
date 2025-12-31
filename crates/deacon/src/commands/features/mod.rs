//! Features command implementation
//!
//! Implements the `deacon features` subcommands for testing, packaging, and publishing
//! DevContainer features. Follows the CLI specification for feature management.
//!
//! ## Module Organization
//!
//! This module is organized into focused submodules:
//! - `shared` - Common types and utilities used across all features subcommands
//! - `plan` - Features plan subcommand for computing installation order
//! - `package` - Features package subcommand for creating feature archives
//! - `publish` - Features publish subcommand for pushing to OCI registries
//! - `test` - Features test subcommand for running feature tests

// Submodules
#[allow(dead_code)]
mod package;
#[allow(dead_code)]
mod plan;
#[allow(dead_code)]
mod publish;
pub mod shared;
#[allow(dead_code)]
mod test;

// Re-export public types from shared module
#[allow(unused_imports)]
pub use shared::{
    create_feature_tgz, enumerate_and_validate_collection, validate_single,
    write_collection_metadata, CollectionMetadata, FeatureDescriptor, PackagingMode,
    SourceInformation,
};

// Re-export public types from plan module
#[allow(unused_imports)]
pub use plan::FeaturesPlanResult;

// Temporary re-exports from monolith until we complete the migration
#[allow(unused_imports)]
pub use crate::commands::features_monolith::{
    execute_features, execute_features_publish, FeaturesArgs, FeaturesResult,
};
