//! State management for tracking running containers and compose projects
//!
//! This module provides state persistence to track which containers and compose projects
//! are running, enabling the down command to stop them according to shutdown actions.

use crate::cache::{Cache, DiskCache};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tracing::{debug, info, instrument};

/// State information for a running container
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ContainerState {
    /// Container ID
    pub container_id: String,
    /// Container name (if any)
    pub container_name: Option<String>,
    /// Image ID used
    pub image_id: String,
    /// Shutdown action from config
    pub shutdown_action: Option<String>,
}

/// State information for a running compose project
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ComposeState {
    /// Compose project name
    pub project_name: String,
    /// Service name (primary service)
    pub service_name: String,
    /// Base directory containing compose files
    pub base_path: String,
    /// Compose file paths (relative to base_path)
    pub compose_files: Vec<String>,
    /// Shutdown action from config
    pub shutdown_action: Option<String>,
}

/// Overall state for a workspace
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum WorkspaceState {
    /// Single container workspace
    Container(ContainerState),
    /// Docker Compose workspace
    Compose(ComposeState),
}

/// State manager for tracking workspace states
pub struct StateManager {
    cache: DiskCache<String, WorkspaceState>,
}

impl StateManager {
    /// Create a new state manager with default cache directory
    pub fn new() -> Result<Self> {
        let cache_dir = Self::default_cache_dir()?;
        Self::new_with_cache_dir(cache_dir)
    }

    /// Create a new state manager with custom cache directory
    pub fn new_with_cache_dir<P: AsRef<Path>>(cache_dir: P) -> Result<Self> {
        let state_cache_dir = cache_dir.as_ref().join("state");
        let cache = DiskCache::new(&state_cache_dir)
            .with_context(|| format!("Failed to create state cache in {:?}", state_cache_dir))?;

        Ok(Self { cache })
    }

    /// Get the default cache directory for state management
    fn default_cache_dir() -> Result<PathBuf> {
        // Use the same pattern as features cache
        let cache_dir = std::env::temp_dir().join("deacon-state");
        if !cache_dir.exists() {
            std::fs::create_dir_all(&cache_dir).with_context(|| {
                format!("Failed to create state cache directory: {:?}", cache_dir)
            })?;
        }
        Ok(cache_dir)
    }

    /// Save container state for a workspace
    #[instrument(skip(self))]
    pub fn save_container_state(
        &mut self,
        workspace_hash: &str,
        container_state: ContainerState,
    ) -> Result<()> {
        debug!(
            workspace_hash = %workspace_hash,
            container_id = %container_state.container_id,
            "Saving container state"
        );

        let state = WorkspaceState::Container(container_state);
        self.cache
            .set(workspace_hash.to_string(), state)
            .with_context(|| {
                format!(
                    "Failed to save container state for workspace {}",
                    workspace_hash
                )
            })?;

        info!(
            workspace_hash = %workspace_hash,
            "Container state saved successfully"
        );

        Ok(())
    }

    /// Save compose state for a workspace
    #[instrument(skip(self))]
    pub fn save_compose_state(
        &mut self,
        workspace_hash: &str,
        compose_state: ComposeState,
    ) -> Result<()> {
        debug!(
            workspace_hash = %workspace_hash,
            project_name = %compose_state.project_name,
            "Saving compose state"
        );

        let state = WorkspaceState::Compose(compose_state);
        self.cache
            .set(workspace_hash.to_string(), state)
            .with_context(|| {
                format!(
                    "Failed to save compose state for workspace {}",
                    workspace_hash
                )
            })?;

        info!(
            workspace_hash = %workspace_hash,
            "Compose state saved successfully"
        );

        Ok(())
    }

    /// Get workspace state by workspace hash
    #[instrument(skip(self))]
    pub fn get_workspace_state(&mut self, workspace_hash: &str) -> Option<WorkspaceState> {
        debug!(workspace_hash = %workspace_hash, "Getting workspace state");

        let state = self.cache.get(&workspace_hash.to_string());

        if state.is_some() {
            debug!(workspace_hash = %workspace_hash, "Found workspace state");
        } else {
            debug!(workspace_hash = %workspace_hash, "No workspace state found");
        }

        state
    }

    /// Remove workspace state (called after successful shutdown)
    #[instrument(skip(self))]
    pub fn remove_workspace_state(&mut self, workspace_hash: &str) -> Option<WorkspaceState> {
        debug!(workspace_hash = %workspace_hash, "Removing workspace state");

        let removed = self.cache.remove(&workspace_hash.to_string());

        if removed.is_some() {
            info!(workspace_hash = %workspace_hash, "Workspace state removed");
        } else {
            debug!(workspace_hash = %workspace_hash, "No workspace state to remove");
        }

        removed
    }

    /// List all tracked workspace hashes
    pub fn list_workspace_hashes(&self) -> Vec<String> {
        // Note: This would require extending the Cache trait to support listing keys
        // For now, we can implement a simpler approach by scanning the cache directory
        // This is acceptable since the cache is file-based

        let cache_dir = std::env::temp_dir().join("deacon-state").join("state");
        if !cache_dir.exists() {
            return Vec::new();
        }

        let mut workspace_hashes = Vec::new();
        if let Ok(entries) = std::fs::read_dir(&cache_dir) {
            for entry in entries.flatten() {
                if let Some(file_name) = entry.file_name().to_str() {
                    // Remove the file extension to get the workspace hash
                    if let Some(hash) = file_name.strip_suffix(".bin") {
                        workspace_hashes.push(hash.to_string());
                    }
                }
            }
        }

        workspace_hashes
    }

    /// Clear all workspace states (for testing/cleanup)
    pub fn clear_all(&mut self) {
        self.cache.clear();
    }

    /// Get cache statistics
    pub fn stats(&self) -> crate::cache::CacheStats {
        self.cache.stats()
    }
}

impl Default for StateManager {
    fn default() -> Self {
        Self::new().expect("Failed to create default StateManager")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_state_manager_creation() {
        let temp_dir = TempDir::new().unwrap();
        let state_manager = StateManager::new_with_cache_dir(temp_dir.path()).unwrap();

        // Should be able to create successfully
        assert_eq!(state_manager.stats().entries, 0);
    }

    #[test]
    fn test_container_state_round_trip() {
        let temp_dir = TempDir::new().unwrap();
        let mut state_manager = StateManager::new_with_cache_dir(temp_dir.path()).unwrap();

        let container_state = ContainerState {
            container_id: "abc123".to_string(),
            container_name: Some("test-container".to_string()),
            image_id: "image123".to_string(),
            shutdown_action: Some("stopContainer".to_string()),
        };

        let workspace_hash = "test-workspace-hash";

        // Save state
        state_manager
            .save_container_state(workspace_hash, container_state.clone())
            .unwrap();

        // Retrieve state
        let retrieved = state_manager.get_workspace_state(workspace_hash).unwrap();

        match retrieved {
            WorkspaceState::Container(retrieved_container) => {
                assert_eq!(retrieved_container, container_state);
            }
            _ => panic!("Expected container state"),
        }
    }

    #[test]
    fn test_compose_state_round_trip() {
        let temp_dir = TempDir::new().unwrap();
        let mut state_manager = StateManager::new_with_cache_dir(temp_dir.path()).unwrap();

        let compose_state = ComposeState {
            project_name: "test-project".to_string(),
            service_name: "app".to_string(),
            base_path: "/workspace".to_string(),
            compose_files: vec!["docker-compose.yml".to_string()],
            shutdown_action: Some("stopCompose".to_string()),
        };

        let workspace_hash = "test-workspace-hash";

        // Save state
        state_manager
            .save_compose_state(workspace_hash, compose_state.clone())
            .unwrap();

        // Retrieve state
        let retrieved = state_manager.get_workspace_state(workspace_hash).unwrap();

        match retrieved {
            WorkspaceState::Compose(retrieved_compose) => {
                assert_eq!(retrieved_compose, compose_state);
            }
            _ => panic!("Expected compose state"),
        }
    }

    #[test]
    fn test_remove_workspace_state() {
        let temp_dir = TempDir::new().unwrap();
        let mut state_manager = StateManager::new_with_cache_dir(temp_dir.path()).unwrap();

        let container_state = ContainerState {
            container_id: "abc123".to_string(),
            container_name: None,
            image_id: "image123".to_string(),
            shutdown_action: None,
        };

        let workspace_hash = "test-workspace-hash";

        // Save state
        state_manager
            .save_container_state(workspace_hash, container_state.clone())
            .unwrap();

        // Verify it exists
        assert!(state_manager.get_workspace_state(workspace_hash).is_some());

        // Remove state
        let removed = state_manager.remove_workspace_state(workspace_hash);
        assert!(removed.is_some());

        // Verify it's gone
        assert!(state_manager.get_workspace_state(workspace_hash).is_none());
    }

    #[test]
    fn test_nonexistent_workspace_state() {
        let temp_dir = TempDir::new().unwrap();
        let mut state_manager = StateManager::new_with_cache_dir(temp_dir.path()).unwrap();

        let result = state_manager.get_workspace_state("nonexistent");
        assert!(result.is_none());
    }
}
