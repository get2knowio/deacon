//! Shared testcontainers helpers for integration tests.
//!
//! This module provides reusable patterns for container-based testing using testcontainers.
//! All containers are automatically cleaned up when dropped (RAII pattern).

#![allow(dead_code)]

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

use testcontainers::core::WaitFor;
use testcontainers::runners::AsyncRunner;
use testcontainers::{ContainerAsync, ContainerRequest, GenericImage, ImageExt};

/// Default timeout for container operations.
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);

/// Create a simple Alpine container request that sleeps forever.
/// Useful for tests that need a running container to exec into.
pub fn alpine_sleep_image() -> ContainerRequest<GenericImage> {
    GenericImage::new("alpine", "3.18")
        .with_wait_for(WaitFor::Nothing)
        .with_cmd(["sleep", "infinity"])
}

/// Create an Alpine container with custom labels.
/// Useful for tests that need to find containers by label.
pub fn alpine_sleep_with_labels(labels: &[(&str, &str)]) -> ContainerRequest<GenericImage> {
    let mut request = GenericImage::new("alpine", "3.18")
        .with_wait_for(WaitFor::Nothing)
        .with_cmd(["sleep", "infinity"]);

    for (key, value) in labels {
        request = request.with_label(*key, *value);
    }

    request
}

/// Create a container request running a simple HTTP server.
/// Returns the image configured to expose port 80.
pub fn http_server_image() -> ContainerRequest<GenericImage> {
    GenericImage::new("nginx", "alpine")
        .with_wait_for(WaitFor::message_on_stderr("start worker process"))
        .with_cmd(["nginx", "-g", "daemon off;"])
}

/// Start an Alpine container asynchronously.
/// The container is automatically cleaned up when the returned handle is dropped.
pub async fn start_alpine_container(
) -> Result<ContainerAsync<GenericImage>, testcontainers::TestcontainersError> {
    alpine_sleep_image().start().await
}

/// Helper to get container ID for use with deacon CLI.
pub async fn container_id<I: testcontainers::Image>(container: &ContainerAsync<I>) -> String {
    container.id().to_string()
}

/// Helper to get a mapped host port for a container port.
pub async fn get_host_port<I: testcontainers::Image>(
    container: &ContainerAsync<I>,
    container_port: u16,
) -> u16 {
    container.get_host_port_ipv4(container_port).await.unwrap()
}

/// Test fixture that combines a devcontainer workspace with testcontainers.
/// Automatically cleans up both the container and workspace on drop.
pub struct DevcontainerTestFixture {
    /// Path to temporary workspace directory.
    pub workspace: PathBuf,
    /// Environment variables to set for deacon commands.
    pub env: HashMap<String, String>,
    /// Temporary directory handle (keeps directory alive).
    _temp_dir: tempfile::TempDir,
}

impl DevcontainerTestFixture {
    /// Create a new test fixture with a temporary workspace.
    pub fn new() -> std::io::Result<Self> {
        let temp_dir = tempfile::TempDir::new()?;
        let workspace = temp_dir.path().to_path_buf();
        Ok(Self {
            workspace,
            env: HashMap::new(),
            _temp_dir: temp_dir,
        })
    }

    /// Create a devcontainer.json file in the workspace.
    pub fn write_devcontainer_json(&self, content: &str) -> std::io::Result<()> {
        let devcontainer_dir = self.workspace.join(".devcontainer");
        std::fs::create_dir_all(&devcontainer_dir)?;
        std::fs::write(devcontainer_dir.join("devcontainer.json"), content)?;
        Ok(())
    }

    /// Get the workspace path.
    pub fn workspace_path(&self) -> &Path {
        &self.workspace
    }
}

impl Default for DevcontainerTestFixture {
    fn default() -> Self {
        Self::new().expect("failed to create test fixture")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_alpine_container_starts() {
        let container = start_alpine_container().await.unwrap();
        let id = container_id(&container).await;
        assert!(!id.is_empty());
        // Container automatically cleaned up when dropped
    }

    #[test]
    fn test_fixture_creates_workspace() {
        let fixture = DevcontainerTestFixture::new().unwrap();
        assert!(fixture.workspace_path().exists());
    }
}
