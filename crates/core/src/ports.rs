//! Port forwarding and event handling
//!
//! This module handles port forwarding simulation, event emission, and attribute
//! processing for DevContainer port configurations.

use crate::config::{DevContainerConfig, OnAutoForward, PortAttributes, PortSpec};
use crate::docker::{ContainerInfo, ExposedPort, PortMapping};
use crate::redaction::{RedactingWriter, RedactionConfig, SecretRegistry};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
#[allow(unused_imports)] // Used by RedactingWriter.write_line() method
use std::io::Write;
use tracing::{info, warn};

/// A port event that represents the state of a forwarded port
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PortEvent {
    /// The port number being forwarded
    pub port: u16,
    /// Protocol handling hint from portsAttributes (`http`/`https`)
    pub protocol: Option<String>,
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
    /// Whether tools should try to elevate privileges for low local ports
    pub elevate_if_needed: Option<bool>,
}

/// Port forwarding manager that handles port discovery, matching, and event emission
pub struct PortForwardingManager;

impl PortForwardingManager {
    /// Process container ports and emit events for configured forwards
    pub fn process_container_ports(
        config: &DevContainerConfig,
        container_info: &ContainerInfo,
        emit_events: bool,
        redaction_config: Option<&RedactionConfig>,
        secret_registry: Option<&SecretRegistry>,
    ) -> Vec<PortEvent> {
        let mut events = Vec::new();

        // Collect all configured ports from forwardPorts and appPort
        let configured_ports = Self::collect_configured_ports(config);

        // Validate port attributes and warn about unknown references
        Self::validate_port_attributes(config, &configured_ports);

        // Process exposed ports from container
        for exposed_port in &container_info.exposed_ports {
            if let Some(port_config) = configured_ports.get(&exposed_port.port) {
                let port_mapping = container_info.port_mappings.iter().find(|pm| {
                    pm.container_port == exposed_port.port && pm.protocol == exposed_port.protocol
                });

                let event =
                    Self::create_port_event(exposed_port, port_mapping, port_config, config);

                if emit_events {
                    Self::emit_port_event(&event, redaction_config, secret_registry);
                }

                events.push(event);
            }
        }

        // Process port mappings that might not have exposed ports defined
        for port_mapping in &container_info.port_mappings {
            if let Some(port_config) = configured_ports.get(&port_mapping.container_port) {
                // Skip if we already processed this port via exposed ports
                if container_info.exposed_ports.iter().any(|ep| {
                    ep.port == port_mapping.container_port && ep.protocol == port_mapping.protocol
                }) {
                    continue;
                }

                let exposed_port = ExposedPort {
                    port: port_mapping.container_port,
                    protocol: port_mapping.protocol.clone(),
                };

                let event =
                    Self::create_port_event(&exposed_port, Some(port_mapping), port_config, config);

                if emit_events {
                    Self::emit_port_event(&event, redaction_config, secret_registry);
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

        // Add appPort if specified (single value or array)
        if let Some(app_port) = &config.app_port {
            for port_spec in app_port.specs() {
                if let Some(port_num) = port_spec.primary_port() {
                    ports.insert(port_num, port_spec);
                }
            }
        }

        ports
    }

    /// Validate port attributes and warn about unknown port references
    fn validate_port_attributes(
        config: &DevContainerConfig,
        configured_ports: &HashMap<u16, &PortSpec>,
    ) {
        // Check each port attribute reference
        for port_key in config.ports_attributes.keys() {
            let mut found = false;

            // Try direct port number match
            if let Ok(port_num) = port_key.parse::<u16>() {
                if configured_ports.contains_key(&port_num) {
                    found = true;
                }
            }

            // Try with transport protocol suffix removal
            if !found {
                for suffix in ["/tcp", "/udp"] {
                    if let Some(port_without_suffix) = port_key.strip_suffix(suffix) {
                        if let Ok(port_num) = port_without_suffix.parse::<u16>() {
                            if configured_ports.contains_key(&port_num) {
                                found = true;
                                break;
                            }
                        }
                    }
                }
            }

            // Try exact string match with configured port specs
            if !found {
                for port_spec in configured_ports.values() {
                    if port_spec.as_string() == *port_key {
                        found = true;
                        break;
                    }
                }
            }

            if !found {
                warn!(
                    "Port attribute '{}' does not match any configured port in forwardPorts or appPort",
                    port_key
                );
            }
        }
    }

    /// Create a port event from the exposed port and configuration
    fn create_port_event(
        exposed_port: &ExposedPort,
        port_mapping: Option<&PortMapping>,
        _port_spec: &PortSpec,
        config: &DevContainerConfig,
    ) -> PortEvent {
        // Get port attributes for this specific port
        let port_attrs =
            Self::get_port_attributes_for(exposed_port.port, &exposed_port.protocol, config);

        PortEvent {
            port: exposed_port.port,
            protocol: port_attrs.protocol,
            label: port_attrs.label,
            on_auto_forward: port_attrs.on_auto_forward,
            auto_forwarded: port_mapping.is_some(),
            local_port: port_mapping.map(|pm| pm.host_port),
            host_ip: port_mapping.map(|pm| pm.host_ip.clone()),
            description: port_attrs.description,
            open_preview: port_attrs.open_preview,
            require_local_port: port_attrs.require_local_port,
            elevate_if_needed: port_attrs.elevate_if_needed,
        }
    }

    /// Get merged port attributes for a specific port and transport protocol
    fn get_port_attributes_for(
        port: u16,
        transport_protocol: &str,
        config: &DevContainerConfig,
    ) -> PortAttributes {
        let mut attrs = PortAttributes {
            label: None,
            on_auto_forward: None,
            protocol: None,
            open_preview: None,
            require_local_port: None,
            elevate_if_needed: None,
            description: None,
        };

        // Apply otherPortsAttributes as defaults
        if let Some(other_attrs) = &config.other_ports_attributes {
            Self::merge_port_attributes(&mut attrs, other_attrs);
        }

        // Override with specific port attributes
        let port_key = port.to_string();
        if let Some(specific_attrs) = config.ports_attributes.get(&port_key) {
            Self::merge_port_attributes(&mut attrs, specific_attrs);
        }

        // Also try with transport protocol suffix
        let port_key_with_protocol = format!("{}/{}", port, transport_protocol);
        if let Some(specific_attrs) = config.ports_attributes.get(&port_key_with_protocol) {
            Self::merge_port_attributes(&mut attrs, specific_attrs);
        }

        attrs
    }

    fn merge_port_attributes(target: &mut PortAttributes, source: &PortAttributes) {
        if source.label.is_some() {
            target.label = source.label.clone();
        }
        if source.on_auto_forward.is_some() {
            target.on_auto_forward = source.on_auto_forward.clone();
        }
        if source.protocol.is_some() {
            target.protocol = source.protocol.clone();
        }
        if source.open_preview.is_some() {
            target.open_preview = source.open_preview;
        }
        if source.require_local_port.is_some() {
            target.require_local_port = source.require_local_port;
        }
        if source.elevate_if_needed.is_some() {
            target.elevate_if_needed = source.elevate_if_needed;
        }
        if source.description.is_some() {
            target.description = source.description.clone();
        }
    }

    /// Emit a port event to stdout with PORT_EVENT prefix
    fn emit_port_event(
        event: &PortEvent,
        redaction_config: Option<&RedactionConfig>,
        secret_registry: Option<&SecretRegistry>,
    ) {
        match serde_json::to_string(event) {
            Ok(json) => {
                let output_line = format!("PORT_EVENT: {}", json);

                // Apply redaction if configuration is provided
                if let (Some(config), Some(registry)) = (redaction_config, secret_registry) {
                    let mut stdout = std::io::stdout();
                    let mut redacting_writer =
                        RedactingWriter::new(&mut stdout, config.clone(), registry);
                    if let Err(e) = redacting_writer.write_line(&output_line) {
                        warn!("Failed to write redacted port event: {}", e);
                    }
                } else {
                    // Fall back to direct output when redaction is not configured
                    println!("{}", output_line);
                }
            }
            Err(e) => {
                warn!("Failed to serialize port event: {}", e);
            }
        }

        // Handle onAutoForward behaviors
        let url_scheme = event.protocol.as_deref().unwrap_or("http");
        if let Some(ref action) = event.on_auto_forward {
            match action {
                OnAutoForward::Notify => {
                    info!(
                        "Port {} is now available at localhost:{}",
                        event.port,
                        event.local_port.unwrap_or(event.port)
                    );
                }
                OnAutoForward::OpenBrowser | OnAutoForward::OpenBrowserOnce => {
                    // Report availability + the URL. The actual browser launch
                    // happens in the `up` caller (which owns the machine-owner
                    // browser resolution + headless gate); this core path stays
                    // pure reporting. `openBrowserOnce` vs `openBrowser` is moot
                    // on this one-shot static path.
                    info!(
                        "Port {} is now available at {}://localhost:{}",
                        event.port,
                        url_scheme,
                        event.local_port.unwrap_or(event.port)
                    );
                }
                OnAutoForward::OpenPreview => {
                    // No preview pane in a CLI; surface availability only (the
                    // user chose notify-only semantics for openPreview).
                    info!(
                        "Port {} is now available at localhost:{}",
                        event.port,
                        event.local_port.unwrap_or(event.port)
                    );
                }
                OnAutoForward::Silent => {
                    // Do nothing - silent forwarding
                }
                OnAutoForward::Ignore => {
                    // Should not happen as ignored ports shouldn't generate events
                    warn!(
                        "Port {} marked as ignore but still generated event",
                        event.port
                    );
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::AppPort;
    use std::collections::HashMap;

    #[test]
    #[allow(clippy::field_reassign_with_default)]
    fn test_collect_configured_ports() {
        let mut config = DevContainerConfig::default();
        config.forward_ports = vec![
            PortSpec::Number(3000),
            PortSpec::String("8080:8080".to_string()),
        ];
        config.app_port = Some(AppPort::Single(PortSpec::Number(4000)));

        let ports = PortForwardingManager::collect_configured_ports(&config);

        assert_eq!(ports.len(), 3);
        assert!(ports.contains_key(&3000));
        assert!(ports.contains_key(&8080));
        assert!(ports.contains_key(&4000));
    }

    #[test]
    #[allow(clippy::field_reassign_with_default)]
    fn test_get_port_attributes_with_defaults() {
        let mut ports_attributes = HashMap::new();
        ports_attributes.insert(
            "3000".to_string(),
            PortAttributes {
                label: Some("Web Server".to_string()),
                on_auto_forward: Some(OnAutoForward::Notify),
                protocol: None,
                open_preview: None,
                require_local_port: None,
                elevate_if_needed: None,
                description: None,
            },
        );

        let mut config = DevContainerConfig::default();
        config.ports_attributes = ports_attributes;
        config.other_ports_attributes = Some(PortAttributes {
            label: Some("Default Service".to_string()),
            on_auto_forward: Some(OnAutoForward::Silent),
            protocol: None,
            open_preview: Some(false),
            require_local_port: Some(false),
            elevate_if_needed: None,
            description: Some("Default description".to_string()),
        });

        // Test specific port override
        let attrs = PortForwardingManager::get_port_attributes_for(3000, "tcp", &config);
        assert_eq!(attrs.label, Some("Web Server".to_string()));
        assert_eq!(attrs.on_auto_forward, Some(OnAutoForward::Notify));
        assert_eq!(attrs.open_preview, Some(false)); // From default
        assert_eq!(attrs.description, Some("Default description".to_string())); // From default

        // Test fallback to defaults
        let attrs = PortForwardingManager::get_port_attributes_for(8080, "tcp", &config);
        assert_eq!(attrs.label, Some("Default Service".to_string()));
        assert_eq!(attrs.on_auto_forward, Some(OnAutoForward::Silent));
        assert_eq!(attrs.open_preview, Some(false));
        assert_eq!(attrs.description, Some("Default description".to_string()));
    }

    #[test]
    #[allow(clippy::field_reassign_with_default)]
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
        ports_attributes.insert(
            "3000".to_string(),
            PortAttributes {
                label: Some("Web Server".to_string()),
                on_auto_forward: Some(OnAutoForward::Notify),
                protocol: None,
                open_preview: Some(true),
                require_local_port: None,
                elevate_if_needed: None,
                description: Some("Main web server".to_string()),
            },
        );

        let mut config = DevContainerConfig::default();
        config.ports_attributes = ports_attributes;

        let event = PortForwardingManager::create_port_event(
            &exposed_port,
            Some(&port_mapping),
            &port_spec,
            &config,
        );

        assert_eq!(event.port, 3000);
        assert_eq!(event.protocol, None);
        assert_eq!(event.label, Some("Web Server".to_string()));
        assert_eq!(event.on_auto_forward, Some(OnAutoForward::Notify));
        assert!(event.auto_forwarded);
        assert_eq!(event.local_port, Some(3000));
        assert_eq!(event.host_ip, Some("0.0.0.0".to_string()));
        assert_eq!(event.description, Some("Main web server".to_string()));
        assert_eq!(event.open_preview, Some(true));
    }

    #[test]
    #[allow(clippy::field_reassign_with_default)]
    fn test_port_event_includes_protocol_and_elevation_attributes() {
        let exposed_port = ExposedPort {
            port: 8443,
            protocol: "udp".to_string(),
        };
        let port_mapping = PortMapping {
            host_port: 443,
            container_port: 8443,
            protocol: "udp".to_string(),
            host_ip: "0.0.0.0".to_string(),
        };

        let mut ports_attributes = HashMap::new();
        ports_attributes.insert(
            "8443".to_string(),
            PortAttributes {
                label: Some("Base label".to_string()),
                on_auto_forward: None,
                protocol: Some("http".to_string()),
                open_preview: None,
                require_local_port: None,
                elevate_if_needed: Some(false),
                description: None,
            },
        );
        ports_attributes.insert(
            "8443/udp".to_string(),
            PortAttributes {
                label: Some("Secure API".to_string()),
                on_auto_forward: None,
                protocol: Some("https".to_string()),
                open_preview: None,
                require_local_port: None,
                elevate_if_needed: Some(true),
                description: None,
            },
        );

        let mut config = DevContainerConfig::default();
        config.forward_ports = vec![PortSpec::Number(8443)];
        config.ports_attributes = ports_attributes;

        let event = PortForwardingManager::create_port_event(
            &exposed_port,
            Some(&port_mapping),
            &PortSpec::Number(8443),
            &config,
        );

        assert_eq!(event.protocol, Some("https".to_string()));
        assert_eq!(event.label, Some("Secure API".to_string()));
        assert_eq!(event.elevate_if_needed, Some(true));
    }

    #[test]
    #[allow(clippy::field_reassign_with_default)]
    fn test_unknown_port_attributes_warning() {
        // Test that warnings are generated for port attributes that don't match configured ports
        let mut ports_attributes = HashMap::new();
        ports_attributes.insert(
            "3000".to_string(),
            PortAttributes {
                label: Some("Valid Port".to_string()),
                on_auto_forward: Some(OnAutoForward::Notify),
                protocol: None,
                open_preview: None,
                require_local_port: None,
                elevate_if_needed: None,
                description: None,
            },
        );
        ports_attributes.insert(
            "9999".to_string(),
            PortAttributes {
                label: Some("Unknown Port".to_string()),
                on_auto_forward: Some(OnAutoForward::Silent),
                protocol: None,
                open_preview: None,
                require_local_port: None,
                elevate_if_needed: None,
                description: None,
            },
        );

        let mut config = DevContainerConfig::default();
        config.forward_ports = vec![PortSpec::Number(3000)]; // Only 3000 is configured
        config.ports_attributes = ports_attributes;

        let container_info = ContainerInfo {
            id: "test-container-123".to_string(),
            names: vec!["test-container".to_string()],
            image: "node:18".to_string(),
            status: "running".to_string(),
            state: "running".to_string(),
            exposed_ports: vec![ExposedPort {
                port: 3000,
                protocol: "tcp".to_string(),
            }],
            port_mappings: vec![PortMapping {
                host_port: 3000,
                container_port: 3000,
                protocol: "tcp".to_string(),
                host_ip: "0.0.0.0".to_string(),
            }],
            env: HashMap::new(),
            labels: HashMap::new(),
            mounts: vec![],
        };

        // This should generate a warning for port 9999 which is not configured
        let events = PortForwardingManager::process_container_ports(
            &config,
            &container_info,
            false,
            None,
            None,
        );

        // Only the configured port (3000) should generate an event
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].port, 3000);

        // Note: The warning for port 9999 would be generated but not easily testable
        // without capturing log output. In practice, this test validates the logic
        // and ensures the warning path is exercised.
    }
}
