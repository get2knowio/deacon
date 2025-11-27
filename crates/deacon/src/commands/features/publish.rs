//! Features publish subcommand implementation
//!
//! Implements the `deacon features publish` subcommand for pushing features to OCI registries.
//!
//! # TODO: Module Migration
//! This module currently re-exports from the monolith. Implementation should be moved here.

// Temporarily re-export from monolith until migration is complete
#[allow(unused_imports)]
pub(crate) use crate::commands::features_monolith::{
    compute_publish_plan, execute_features_publish, output_publish_result,
};
