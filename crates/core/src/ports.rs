//! Port forwarding and event handling
//!
//! This module handles port forwarding simulation, event emission, and attribute
//! processing for DevContainer port configurations.

use crate::config::{DevContainerConfig, OnAutoForward, PortAttributes, PortSpec};
use crate::docker::{ContainerInfo, ExposedPort, PortMapping};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tracing::{info, warn};

/// A port event that represents the state of a forwarded port
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PortEvent {
    /// The port number being forwarded
    pub port: u16,
    /// The protocol (tcp/udp)
    pub protocol: String,
    /// Human-readable label for this port
    pub label: Option<String>,
    /// The action taken when this port was auto-forwarded
    pub on_auto_forward: Option<OnAutoForward>,
    /// Whether this port is auto-forwarded
    pub auto_forwarded: bool,
    /// Local (host) port if different from container port
    pub local_port: Option<u16>,
    /// Host IP address for the binding
    pub host_ip: Option<String>,
    /// Additional description of this port
    pub description: Option<String>,
    /// Whether to open a preview automatically
    pub open_preview: Option<bool>,
    /// Whether this port requires a specific local port
    pub require_local_port: Option<bool>,
}

/// Port forwarding manager that handles port discovery, matching, and event emission
pub struct PortForwardingManager;

impl PortForwardingManager {
    /// Process container ports and emit events for configured forwards
    pub fn process_container_ports(
        config: &DevContainerConfig,
        container_info: &ContainerInfo,
        emit_events: bool,
    ) -> Vec<PortEvent> {
        let mut events = Vec::new();

        // Collect all configured ports from forwardPorts and appPort
        let configured_ports = Self::collect_configured_ports(config);
        
        // Process exposed ports from container
        for exposed_port in &container_info.exposed_ports {
            if let Some(port_config) = configured_ports.get(&exposed_port.port) {
                let port_mapping = container_info
                    .port_mappings
                    .iter()
                    .find(|pm| pm.container_port == exposed_port.port && pm.protocol == exposed_port.protocol);
                
                let event = Self::create_port_event(
                    exposed_port,
                    port_mapping,
                    port_config,
                    config,
                );
                
                if emit_events {
                    Self::emit_port_event(&event);
                }
                
                events.push(event);
            }
        }

        // Process port mappings that might not have exposed ports defined
        for port_mapping in &container_info.port_mappings {
            if let Some(port_config) = configured_ports.get(&port_mapping.container_port) {
                // Skip if we already processed this port via exposed ports
                if container_info.exposed_ports.iter().any(|ep| 
                    ep.port == port_mapping.container_port && ep.protocol == port_mapping.protocol
                ) {
                    continue;
                }
                
                let exposed_port = ExposedPort {
                    port: port_mapping.container_port,
                    protocol: port_mapping.protocol.clone(),
                };
                
                let event = Self::create_port_event(
                    &exposed_port,
                    Some(port_mapping),
                    port_config,
                    config,
                );
                
                if emit_events {
                    Self::emit_port_event(&event);
                }
                
                events.push(event);
            }
        }

        events
    }

    /// Collect all configured ports from the DevContainer configuration
    fn collect_configured_ports(config: &DevContainerConfig) -> HashMap<u16, &PortSpec> {
        let mut ports = HashMap::new();

        // Add ports from forwardPorts
        for port_spec in &config.forward_ports {
            if let Some(port_num) = port_spec.primary_port() {
                ports.insert(port_num, port_spec);
            }
        }

        // Add appPort if specified
        if let Some(app_port) = &config.app_port {
            if let Some(port_num) = app_port.primary_port() {
                ports.insert(port_num, app_port);
            }
        }

        ports
    }

    /// Create a port event from the exposed port and configuration
    fn create_port_event(
        exposed_port: &ExposedPort,
        port_mapping: Option<&PortMapping>,
        _port_spec: &PortSpec,
        config: &DevContainerConfig,
    ) -> PortEvent {
        // Get port attributes for this specific port
        let port_attrs = Self::get_port_attributes(exposed_port.port, config);

        PortEvent {
            port: exposed_port.port,
            protocol: exposed_port.protocol.clone(),
            label: port_attrs.label,
            on_auto_forward: port_attrs.on_auto_forward,
            auto_forwarded: port_mapping.is_some(),
            local_port: port_mapping.map(|pm| pm.host_port),
            host_ip: port_mapping.map(|pm| pm.host_ip.clone()),
            description: port_attrs.description,
            open_preview: port_attrs.open_preview,
            require_local_port: port_attrs.require_local_port,
        }
    }

    /// Get merged port attributes for a specific port
    fn get_port_attributes(port: u16, config: &DevContainerConfig) -> PortAttributes {
        let mut attrs = PortAttributes {
            label: None,
            on_auto_forward: None,
            open_preview: None,
            require_local_port: None,
            description: None,
        };

        // Apply otherPortsAttributes as defaults
        if let Some(other_attrs) = &config.other_ports_attributes {
            attrs.label = other_attrs.label.clone();
            attrs.on_auto_forward = other_attrs.on_auto_forward.clone();
            attrs.open_preview = other_attrs.open_preview;
            attrs.require_local_port = other_attrs.require_local_port;
            attrs.description = other_attrs.description.clone();
        }

        // Override with specific port attributes
        let port_key = port.to_string();
        if let Some(specific_attrs) = config.ports_attributes.get(&port_key) {
            if specific_attrs.label.is_some() {
                attrs.label = specific_attrs.label.clone();
            }
            if specific_attrs.on_auto_forward.is_some() {
                attrs.on_auto_forward = specific_attrs.on_auto_forward.clone();
            }
            if specific_attrs.open_preview.is_some() {
                attrs.open_preview = specific_attrs.open_preview;
            }
            if specific_attrs.require_local_port.is_some() {
                attrs.require_local_port = specific_attrs.require_local_port;
            }
            if specific_attrs.description.is_some() {
                attrs.description = specific_attrs.description.clone();
            }
        }

        // Also try with protocol suffix
        let port_key_tcp = format!("{}/tcp", port);
        if let Some(specific_attrs) = config.ports_attributes.get(&port_key_tcp) {
            if specific_attrs.label.is_some() {
                attrs.label = specific_attrs.label.clone();
            }
            if specific_attrs.on_auto_forward.is_some() {
                attrs.on_auto_forward = specific_attrs.on_auto_forward.clone();
            }
            if specific_attrs.open_preview.is_some() {
                attrs.open_preview = specific_attrs.open_preview;
            }
            if specific_attrs.require_local_port.is_some() {
                attrs.require_local_port = specific_attrs.require_local_port;
            }
            if specific_attrs.description.is_some() {
                attrs.description = specific_attrs.description.clone();
            }
        }

        attrs
    }

    /// Emit a port event to stdout with PORT_EVENT prefix
    fn emit_port_event(event: &PortEvent) {
        match serde_json::to_string(event) {
            Ok(json) => {
                println!("PORT_EVENT: {}", json);
            }
            Err(e) => {
                warn!("Failed to serialize port event: {}", e);
            }
        }

        // Handle onAutoForward behaviors
        if let Some(ref action) = event.on_auto_forward {
            match action {
                OnAutoForward::Notify => {
                    info!("Port {} is now available at localhost:{}", 
                          event.port, 
                          event.local_port.unwrap_or(event.port));
                }
                OnAutoForward::OpenBrowser => {
                    info!("Port {} is now available - would open browser at http://localhost:{}", 
                          event.port, 
                          event.local_port.unwrap_or(event.port));
                    // In a real implementation, this would open the browser
                }
                OnAutoForward::OpenPreview => {
                    info!("Port {} is now available - would open preview panel", event.port);
                    // In a real implementation, this would open a preview panel
                }
                OnAutoForward::Silent => {
                    // Do nothing - silent forwarding
                }
                OnAutoForward::Ignore => {
                    // Should not happen as ignored ports shouldn't generate events
                    warn!("Port {} marked as ignore but still generated event", event.port);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn test_collect_configured_ports() {
        let config = DevContainerConfig {
            extends: None,
            name: None,
            image: None,
            dockerfile: None,
            build: None,
            docker_compose_file: None,
            service: None,
            run_services: vec![],
            features: serde_json::Value::Object(Default::default()),
            customizations: serde_json::Value::Object(Default::default()),
            workspace_folder: None,
            workspace_mount: None,
            mounts: vec![],
            container_env: HashMap::new(),
            remote_env: HashMap::new(),
            container_user: None,
            remote_user: None,
            update_remote_user_uid: None,
            forward_ports: vec![
                PortSpec::Number(3000),
                PortSpec::String("8080:8080".to_string()),
            ],
            app_port: Some(PortSpec::Number(4000)),
            ports_attributes: HashMap::new(),
            other_ports_attributes: None,
            run_args: vec![],
            shutdown_action: None,
            override_command: None,
            on_create_command: None,
            post_start_command: None,
            post_create_command: None,
            post_attach_command: None,
            initialize_command: None,
            update_content_command: None,
        };

        let ports = PortForwardingManager::collect_configured_ports(&config);
        
        assert_eq!(ports.len(), 3);
        assert!(ports.contains_key(&3000));
        assert!(ports.contains_key(&8080));
        assert!(ports.contains_key(&4000));
    }

    #[test]
    fn test_get_port_attributes_with_defaults() {
        let mut ports_attributes = HashMap::new();
        ports_attributes.insert("3000".to_string(), PortAttributes {
            label: Some("Web Server".to_string()),
            on_auto_forward: Some(OnAutoForward::Notify),
            open_preview: None,
            require_local_port: None,
            description: None,
        });

        let config = DevContainerConfig {
            extends: None,
            name: None,
            image: None,
            dockerfile: None,
            build: None,
            docker_compose_file: None,
            service: None,
            run_services: vec![],
            features: serde_json::Value::Object(Default::default()),
            customizations: serde_json::Value::Object(Default::default()),
            workspace_folder: None,
            workspace_mount: None,
            mounts: vec![],
            container_env: HashMap::new(),
            remote_env: HashMap::new(),
            container_user: None,
            remote_user: None,
            update_remote_user_uid: None,
            forward_ports: vec![],
            app_port: None,
            ports_attributes,
            other_ports_attributes: Some(PortAttributes {
                label: Some("Default Service".to_string()),
                on_auto_forward: Some(OnAutoForward::Silent),
                open_preview: Some(false),
                require_local_port: Some(false),
                description: Some("Default description".to_string()),
            }),
            run_args: vec![],
            shutdown_action: None,
            override_command: None,
            on_create_command: None,
            post_start_command: None,
            post_create_command: None,
            post_attach_command: None,
            initialize_command: None,
            update_content_command: None,
        };

        // Test specific port override
        let attrs = PortForwardingManager::get_port_attributes(3000, &config);
        assert_eq!(attrs.label, Some("Web Server".to_string()));
        assert_eq!(attrs.on_auto_forward, Some(OnAutoForward::Notify));
        assert_eq!(attrs.open_preview, Some(false)); // From default
        assert_eq!(attrs.description, Some("Default description".to_string())); // From default

        // Test fallback to defaults
        let attrs = PortForwardingManager::get_port_attributes(8080, &config);
        assert_eq!(attrs.label, Some("Default Service".to_string()));
        assert_eq!(attrs.on_auto_forward, Some(OnAutoForward::Silent));
        assert_eq!(attrs.open_preview, Some(false));
        assert_eq!(attrs.description, Some("Default description".to_string()));
    }

    #[test]
    fn test_create_port_event() {
        let exposed_port = ExposedPort {
            port: 3000,
            protocol: "tcp".to_string(),
        };

        let port_mapping = PortMapping {
            host_port: 3000,
            container_port: 3000,
            protocol: "tcp".to_string(),
            host_ip: "0.0.0.0".to_string(),
        };

        let port_spec = PortSpec::Number(3000);

        let mut ports_attributes = HashMap::new();
        ports_attributes.insert("3000".to_string(), PortAttributes {
            label: Some("Web Server".to_string()),
            on_auto_forward: Some(OnAutoForward::Notify),
            open_preview: Some(true),
            require_local_port: None,
            description: Some("Main web server".to_string()),
        });

        let config = DevContainerConfig {
            extends: None,
            name: None,
            image: None,
            dockerfile: None,
            build: None,
            docker_compose_file: None,
            service: None,
            run_services: vec![],
            features: serde_json::Value::Object(Default::default()),
            customizations: serde_json::Value::Object(Default::default()),
            workspace_folder: None,
            workspace_mount: None,
            mounts: vec![],
            container_env: HashMap::new(),
            remote_env: HashMap::new(),
            container_user: None,
            remote_user: None,
            update_remote_user_uid: None,
            forward_ports: vec![],
            app_port: None,
            ports_attributes,
            other_ports_attributes: None,
            run_args: vec![],
            shutdown_action: None,
            override_command: None,
            on_create_command: None,
            post_start_command: None,
            post_create_command: None,
            post_attach_command: None,
            initialize_command: None,
            update_content_command: None,
        };

        let event = PortForwardingManager::create_port_event(
            &exposed_port,
            Some(&port_mapping),
            &port_spec,
            &config,
        );

        assert_eq!(event.port, 3000);
        assert_eq!(event.protocol, "tcp");
        assert_eq!(event.label, Some("Web Server".to_string()));
        assert_eq!(event.on_auto_forward, Some(OnAutoForward::Notify));
        assert!(event.auto_forwarded);
        assert_eq!(event.local_port, Some(3000));
        assert_eq!(event.host_ip, Some("0.0.0.0".to_string()));
        assert_eq!(event.description, Some("Main web server".to_string()));
        assert_eq!(event.open_preview, Some(true));
    }
}