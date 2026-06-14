//! Shared testcontainers helpers for integration tests.
//!
//! This module provides reusable patterns for container-based testing using testcontainers.
//! All containers are automatically cleaned up when dropped (RAII pattern).

#![allow(dead_code)]

use testcontainers::core::WaitFor;
use testcontainers::runners::AsyncRunner;
use testcontainers::{ContainerAsync, ContainerRequest, GenericImage, ImageExt};

/// testcontainers-rs talks to the docker API socket; it cannot drive rootless
/// podman (no podman socket is started in CI). Tests that spin up a container
/// via testcontainers and then exec into it with deacon must therefore skip
/// when the active runtime (`DEACON_CONTAINER_RUNTIME`) is not docker — under
/// podman the testcontainers container lives in docker while deacon looks in
/// podman's store, which is an invalid cross-runtime scenario, not a deacon bug.
///
/// Returns `true` (caller should `return` early) when running under non-docker.
pub fn skip_if_not_docker_runtime() -> bool {
    match std::env::var("DEACON_CONTAINER_RUNTIME") {
        Ok(rt) if rt != "docker" => {
            eprintln!(
                "skipping testcontainers-based test: DEACON_CONTAINER_RUNTIME={rt} \
                 (testcontainers requires the docker API)"
            );
            true
        }
        _ => false,
    }
}

/// Create a simple Alpine container request that sleeps forever.
/// Useful for tests that need a running container to exec into.
pub fn alpine_sleep_image() -> ContainerRequest<GenericImage> {
    GenericImage::new("alpine", "3.18")
        .with_wait_for(WaitFor::seconds(1))
        .with_cmd(["sleep", "infinity"])
}

/// Create an Alpine container with custom labels.
/// Useful for tests that need to find containers by label.
pub fn alpine_sleep_with_labels(labels: &[(&str, &str)]) -> ContainerRequest<GenericImage> {
    let mut request = GenericImage::new("alpine", "3.18")
        .with_wait_for(WaitFor::seconds(1))
        .with_cmd(["sleep", "infinity"]);

    for (key, value) in labels {
        request = request.with_label(*key, *value);
    }

    request
}

/// Start an Alpine container asynchronously.
/// The container is automatically cleaned up when the returned handle is dropped.
pub async fn start_alpine_container()
-> Result<ContainerAsync<GenericImage>, testcontainers::TestcontainersError> {
    alpine_sleep_image().start().await
}

/// Helper to get container ID for use with deacon CLI.
pub fn container_id<I: testcontainers::Image>(container: &ContainerAsync<I>) -> String {
    container.id().to_string()
}

/// Helper to get a mapped host port for a container port.
pub async fn get_host_port<I: testcontainers::Image>(
    container: &ContainerAsync<I>,
    container_port: u16,
) -> u16 {
    container
        .get_host_port_ipv4(container_port)
        .await
        .expect("container port mapping should exist")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_alpine_container_starts() {
        let container = start_alpine_container().await.unwrap();
        let id = container_id(&container);
        assert!(!id.is_empty());
        // Container automatically cleaned up when dropped
    }
}
