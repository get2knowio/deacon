//! Port event handling for the up command.
//!
//! This module contains:
//! - `handle_port_events` - Handle port events for compose projects
//! - `handle_container_port_events` - Handle port events for single containers

use anyhow::Result;
use deacon_core::compose::{ComposeManager, ComposeProject};
use deacon_core::config::DevContainerConfig;
use deacon_core::docker::Docker;
use deacon_core::ports::PortForwardingManager;
use deacon_core::runtime::ContainerRuntimeImpl;
use tracing::{debug, instrument, warn};

/// Handle port events for compose projects
#[instrument(skip(config, project, redaction_config, secret_registry, docker_path))]
pub(crate) async fn handle_port_events(
    config: &DevContainerConfig,
    project: &ComposeProject,
    redaction_config: &deacon_core::redaction::RedactionConfig,
    secret_registry: &deacon_core::redaction::SecretRegistry,
    docker_path: &str,
) -> Result<()> {
    debug!("Processing port events for compose project");

    let compose_manager = ComposeManager::with_docker_path(docker_path.to_string());
    let docker = deacon_core::docker::CliDocker::new();

    // Get all services in the project
    let command = compose_manager.get_command(project);
    let services = match command.ps() {
        Ok(services) => services,
        Err(e) => {
            warn!("Failed to list compose services: {}", e);
            return Ok(());
        }
    };

    // Process port events for all running services
    let mut total_events = 0;
    for service in services.iter().filter(|s| s.state == "running") {
        if let Some(ref container_id) = service.container_id {
            debug!(
                "Processing port events for service '{}' (container: {})",
                service.name, container_id
            );

            // Inspect the container to get port information
            let container_info = match docker.inspect_container(container_id).await? {
                Some(info) => info,
                None => {
                    warn!(
                        "Container {} not found for service '{}', skipping",
                        container_id, service.name
                    );
                    continue;
                }
            };

            debug!(
                "Service '{}' container {} has {} exposed ports and {} port mappings",
                service.name,
                container_id,
                container_info.exposed_ports.len(),
                container_info.port_mappings.len()
            );

            // Process ports and emit events for this service
            let events = PortForwardingManager::process_container_ports(
                config,
                &container_info,
                true, // emit_events = true
                Some(redaction_config),
                Some(secret_registry),
            );

            debug!(
                "Emitted {} port events for service '{}'",
                events.len(),
                service.name
            );
            total_events += events.len();
        }
    }

    debug!(
        "Emitted {} total port events across all services",
        total_events
    );
    Ok(())
}

/// Handle port events for the container
#[instrument(skip(config, redaction_config, secret_registry))]
pub(crate) async fn handle_container_port_events(
    container_id: &str,
    config: &DevContainerConfig,
    runtime: &ContainerRuntimeImpl,
    redaction_config: &deacon_core::redaction::RedactionConfig,
    secret_registry: &deacon_core::redaction::SecretRegistry,
) -> Result<()> {
    debug!("Processing port events for container");

    // Inspect the container to get port information
    let docker = runtime;
    let container_info = match docker.inspect_container(container_id).await? {
        Some(info) => info,
        None => {
            warn!("Container {} not found, skipping port events", container_id);
            return Ok(());
        }
    };

    debug!(
        "Container {} has {} exposed ports and {} port mappings",
        container_id,
        container_info.exposed_ports.len(),
        container_info.port_mappings.len()
    );

    // Process ports and emit events
    let events = PortForwardingManager::process_container_ports(
        config,
        &container_info,
        true, // emit_events = true
        Some(redaction_config),
        Some(secret_registry),
    );

    debug!("Emitted {} port events", events.len());

    Ok(())
}
