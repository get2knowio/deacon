//! Features test subcommand implementation
//!
//! Implements the `deacon features test` subcommand for running feature tests.
//!
//! # TODO: Module Migration
//! This module currently re-exports from the monolith. Implementation should be moved here.

// Temporarily re-export from monolith until migration is complete
#[allow(unused_imports)]
pub(crate) use crate::commands::features_monolith::{
    execute_features_test, execute_features_test_collection, run_feature_test_in_container,
};
