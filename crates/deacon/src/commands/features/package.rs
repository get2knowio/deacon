//! Features package subcommand implementation
//!
//! Implements the `deacon features package` subcommand for creating feature archives.
//!
//! # TODO: Module Migration
//! This module currently re-exports from the monolith. Implementation should be moved here.

// Temporarily re-export from monolith until migration is complete
#[allow(unused_imports)]
pub(crate) use crate::commands::features_monolith::{
    create_feature_package, execute_features_package,
};
