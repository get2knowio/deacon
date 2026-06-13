//! Port event handling for the up command.
//!
//! This module contains:
//! - `handle_port_events` - Handle port events for compose projects
//! - `handle_container_port_events` - Handle port events for single containers

use anyhow::Result;
use deacon_core::compose::{ComposeManager, ComposeProject};
use deacon_core::config::{DevContainerConfig, OnAutoForward};
use deacon_core::docker::Docker;
use deacon_core::ports::{PortEvent, PortForwardingManager};
use deacon_core::runtime::ContainerRuntimeImpl;
use std::path::Path;
use tracing::{debug, instrument, warn};

use super::forward::{auto_open_enabled, resolve_browser_cli};

/// Open the machine owner's browser for static-path port events whose
/// `onAutoForward` is `openBrowser`/`openBrowserOnce` and which have a reachable
/// host port (i.e. `-p`-published). Suppressed entirely when `--auto-forward` is
/// active (the forwarder daemon owns browser-opening). Best-effort — never
/// fails `up`; only opens a loopback URL with a machine-owner-chosen program.
fn open_browsers_for_events(events: &[PortEvent], auto_forward: bool, udf: Option<&Path>) {
    if auto_forward {
        return; // the --auto-forward daemon is the single opener
    }
    let browser = resolve_browser_cli(udf);
    if !auto_open_enabled(browser.is_some()) {
        return;
    }
    for ev in events {
        let opens = matches!(
            ev.on_auto_forward,
            Some(OnAutoForward::OpenBrowser) | Some(OnAutoForward::OpenBrowserOnce)
        );
        // Only open when the port is actually reachable on the host (a `-p`
        // mapping gave it a host `local_port`).
        let Some(host_port) = ev.local_port.filter(|_| opens) else {
            continue;
        };
        let scheme = ev.protocol.as_deref().unwrap_or("http");
        let url = format!("{scheme}://127.0.0.1:{host_port}");
        if let Err(e) = deacon_core::browser::open_url_blocking(browser.as_deref(), &url) {
            debug!(error = %e, url = %url, "static-path browser open failed (best-effort)");
        }
    }
}

/// Handle port events for compose projects
#[instrument(skip(config, project, redaction_config, secret_registry, docker_path))]
pub(crate) async fn handle_port_events(
    config: &DevContainerConfig,
    project: &ComposeProject,
    redaction_config: &deacon_core::redaction::RedactionConfig,
    secret_registry: &deacon_core::redaction::SecretRegistry,
    docker_path: &str,
    auto_forward: bool,
    user_data_folder: Option<&Path>,
) -> Result<()> {
    debug!("Processing port events for compose project");

    let compose_manager = ComposeManager::with_docker_path(docker_path.to_string());
    let docker = deacon_core::docker::CliDocker::new();

    // Get all services in the project
    let command = compose_manager.get_command(project);
    let services = match command.ps().await {
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
            open_browsers_for_events(&events, auto_forward, user_data_folder);
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
    auto_forward: bool,
    user_data_folder: Option<&Path>,
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
    open_browsers_for_events(&events, auto_forward, user_data_folder);

    Ok(())
}
