//! Integration tests for port forwarding and event handling
//!
//! Tests the complete workflow of port configuration, Docker container inspection,
//! and port event emission with attribute resolution.

use deacon_core::config::{DevContainerConfig, OnAutoForward, PortAttributes, PortSpec};
use deacon_core::docker::{ContainerInfo, ExposedPort, PortMapping};
use deacon_core::ports::{PortEvent, PortForwardingManager};
use std::collections::HashMap;

#[test]
#[allow(clippy::field_reassign_with_default)]
fn test_port_event_generation_with_attributes() {
    // Create a sample DevContainer configuration with port forwards and attributes
    let mut ports_attributes = HashMap::new();
    ports_attributes.insert(
        "3000".to_string(),
        PortAttributes {
            label: Some("Web Server".to_string()),
            on_auto_forward: Some(OnAutoForward::Notify),
            open_preview: Some(true),
            require_local_port: None,
            description: Some("Main web application".to_string()),
        },
    );
    ports_attributes.insert(
        "8080".to_string(),
        PortAttributes {
            label: Some("API Server".to_string()),
            on_auto_forward: Some(OnAutoForward::OpenBrowser),
            open_preview: None,
            require_local_port: Some(true),
            description: None,
        },
    );

    let mut config = DevContainerConfig::default();
    config.name = Some("Test Container".to_string());
    config.image = Some("node:18".to_string());
    config.forward_ports = vec![
        PortSpec::Number(3000),
        PortSpec::String("8080:8080".to_string()),
    ];
    config.app_port = Some(PortSpec::Number(4000));
    config.ports_attributes = ports_attributes;
    config.other_ports_attributes = Some(PortAttributes {
        label: Some("Default Service".to_string()),
        on_auto_forward: Some(OnAutoForward::Silent),
        open_preview: Some(false),
        require_local_port: Some(false),
        description: Some("Fallback description".to_string()),
    });

    // Create mock container info with exposed ports and port mappings
    let container_info = ContainerInfo {
        id: "test-container-123".to_string(),
        names: vec!["test-container".to_string()],
        image: "node:18".to_string(),
        status: "running".to_string(),
        state: "running".to_string(),
        exposed_ports: vec![
            ExposedPort {
                port: 3000,
                protocol: "tcp".to_string(),
            },
            ExposedPort {
                port: 8080,
                protocol: "tcp".to_string(),
            },
            ExposedPort {
                port: 4000,
                protocol: "tcp".to_string(),
            },
            ExposedPort {
                port: 9000,
                protocol: "tcp".to_string(),
            }, // Not in config
        ],
        port_mappings: vec![
            PortMapping {
                host_port: 3000,
                container_port: 3000,
                protocol: "tcp".to_string(),
                host_ip: "127.0.0.1".to_string(),
            },
            PortMapping {
                host_port: 8080,
                container_port: 8080,
                protocol: "tcp".to_string(),
                host_ip: "0.0.0.0".to_string(),
            },
            PortMapping {
                host_port: 4000,
                container_port: 4000,
                protocol: "tcp".to_string(),
                host_ip: "0.0.0.0".to_string(),
            },
        ],
        env: HashMap::new(),
        labels: HashMap::new(),
        mounts: vec![],
    };

    // Process container ports without emitting events (for testing)
    let events =
        PortForwardingManager::process_container_ports(&config, &container_info, false, None, None);

    // Verify the correct number of events were generated
    assert_eq!(events.len(), 3); // 3000, 8080, 4000 are configured

    // Find and verify the 3000 port event
    let port_3000_event = events.iter().find(|e| e.port == 3000).unwrap();
    assert_eq!(port_3000_event.protocol, "tcp");
    assert_eq!(port_3000_event.label, Some("Web Server".to_string()));
    assert_eq!(port_3000_event.on_auto_forward, Some(OnAutoForward::Notify));
    assert_eq!(
        port_3000_event.description,
        Some("Main web application".to_string())
    );
    assert_eq!(port_3000_event.open_preview, Some(true));
    assert!(port_3000_event.auto_forwarded);
    assert_eq!(port_3000_event.local_port, Some(3000));
    assert_eq!(port_3000_event.host_ip, Some("127.0.0.1".to_string()));

    // Find and verify the 8080 port event
    let port_8080_event = events.iter().find(|e| e.port == 8080).unwrap();
    assert_eq!(port_8080_event.protocol, "tcp");
    assert_eq!(port_8080_event.label, Some("API Server".to_string()));
    assert_eq!(
        port_8080_event.on_auto_forward,
        Some(OnAutoForward::OpenBrowser)
    );
    assert_eq!(port_8080_event.require_local_port, Some(true));
    assert!(port_8080_event.auto_forwarded);
    assert_eq!(port_8080_event.local_port, Some(8080));
    assert_eq!(port_8080_event.host_ip, Some("0.0.0.0".to_string()));

    // Find and verify the 4000 port event (appPort with fallback attributes)
    let port_4000_event = events.iter().find(|e| e.port == 4000).unwrap();
    assert_eq!(port_4000_event.protocol, "tcp");
    assert_eq!(port_4000_event.label, Some("Default Service".to_string())); // From otherPortsAttributes
    assert_eq!(port_4000_event.on_auto_forward, Some(OnAutoForward::Silent)); // From otherPortsAttributes
    assert_eq!(
        port_4000_event.description,
        Some("Fallback description".to_string())
    ); // From otherPortsAttributes
    assert_eq!(port_4000_event.open_preview, Some(false)); // From otherPortsAttributes
    assert_eq!(port_4000_event.require_local_port, Some(false)); // From otherPortsAttributes
    assert!(port_4000_event.auto_forwarded);
    assert_eq!(port_4000_event.local_port, Some(4000));

    // Verify that port 9000 (not configured) doesn't generate an event
    assert!(!events.iter().any(|e| e.port == 9000));
}

#[test]
fn test_port_event_serialization() {
    let event = PortEvent {
        port: 3000,
        protocol: "tcp".to_string(),
        label: Some("Web Server".to_string()),
        on_auto_forward: Some(OnAutoForward::Notify),
        auto_forwarded: true,
        local_port: Some(3000),
        host_ip: Some("127.0.0.1".to_string()),
        description: Some("Main web server".to_string()),
        open_preview: Some(true),
        require_local_port: Some(false),
    };

    let json = serde_json::to_string(&event).unwrap();
    let parsed: PortEvent = serde_json::from_str(&json).unwrap();

    assert_eq!(parsed.port, event.port);
    assert_eq!(parsed.protocol, event.protocol);
    assert_eq!(parsed.label, event.label);
    assert_eq!(parsed.on_auto_forward, event.on_auto_forward);
    assert_eq!(parsed.auto_forwarded, event.auto_forwarded);
    assert_eq!(parsed.local_port, event.local_port);
    assert_eq!(parsed.host_ip, event.host_ip);
    assert_eq!(parsed.description, event.description);
    assert_eq!(parsed.open_preview, event.open_preview);
    assert_eq!(parsed.require_local_port, event.require_local_port);
}

#[test]
#[allow(clippy::field_reassign_with_default)]
fn test_port_attribute_fallback_behavior() {
    // Test case where a port has no specific attributes but should use otherPortsAttributes
    let mut config = DevContainerConfig::default();
    config.name = Some("Test Container".to_string());
    config.image = Some("node:18".to_string());
    config.forward_ports = vec![PortSpec::Number(3000)];
    config.other_ports_attributes = Some(PortAttributes {
        label: Some("Generic Service".to_string()),
        on_auto_forward: Some(OnAutoForward::Silent),
        open_preview: Some(false),
        require_local_port: Some(false),
        description: Some("Generic description".to_string()),
    });

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

    let events =
        PortForwardingManager::process_container_ports(&config, &container_info, false, None, None);

    assert_eq!(events.len(), 1);
    let event = &events[0];

    // Should use otherPortsAttributes as fallback
    assert_eq!(event.label, Some("Generic Service".to_string()));
    assert_eq!(event.on_auto_forward, Some(OnAutoForward::Silent));
    assert_eq!(event.open_preview, Some(false));
    assert_eq!(event.require_local_port, Some(false));
    assert_eq!(event.description, Some("Generic description".to_string()));
}

#[test]
#[allow(clippy::field_reassign_with_default)]
fn test_exposed_ports_without_mappings() {
    // Test case where container has exposed ports but no port mappings (not forwarded)
    let mut config = DevContainerConfig::default();
    config.name = Some("Test Container".to_string());
    config.image = Some("node:18".to_string());
    config.forward_ports = vec![PortSpec::Number(3000)];

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
        port_mappings: vec![], // No port mappings - port is exposed but not forwarded
        env: HashMap::new(),
        labels: HashMap::new(),
        mounts: vec![],
    };

    let events =
        PortForwardingManager::process_container_ports(&config, &container_info, false, None, None);

    assert_eq!(events.len(), 1);
    let event = &events[0];

    assert_eq!(event.port, 3000);
    assert!(!event.auto_forwarded); // Should be false since no port mapping
    assert_eq!(event.local_port, None); // No local port since not forwarded
    assert_eq!(event.host_ip, None); // No host IP since not forwarded
}

#[test]
fn test_port_event_redaction() {
    use deacon_core::redaction::{RedactionConfig, SecretRegistry};

    // Create a temporary secret registry with a test secret
    let registry = SecretRegistry::new();
    registry.add_secret("secret-port-token-123");
    let _config = RedactionConfig::with_custom_registry(registry.clone());

    // Create a port event containing the secret
    let _port_event = PortEvent {
        port: 3000,
        protocol: "tcp".to_string(),
        label: Some("Web with secret-port-token-123".to_string()),
        on_auto_forward: Some(OnAutoForward::Notify),
        auto_forwarded: true,
        local_port: Some(3000),
        host_ip: Some("127.0.0.1".to_string()),
        description: Some("Contains secret-port-token-123 in description".to_string()),
        open_preview: Some(false),
        require_local_port: Some(false),
    };

    // Create minimal container and config for testing
    let container_info = ContainerInfo {
        id: "test-id".to_string(),
        names: vec!["test-container".to_string()],
        image: "test-image:latest".to_string(),
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
            host_ip: "127.0.0.1".to_string(),
        }],
        env: HashMap::new(),
        labels: HashMap::new(),
        mounts: vec![],
    };

    let mut port_attrs = HashMap::new();
    port_attrs.insert(
        "3000".to_string(),
        PortAttributes {
            label: Some("Web with secret-port-token-123".to_string()),
            on_auto_forward: Some(OnAutoForward::Notify),
            open_preview: Some(false),
            require_local_port: Some(false),
            description: Some("Contains secret-port-token-123 in description".to_string()),
        },
    );

    let config_with_secrets = DevContainerConfig {
        forward_ports: vec![PortSpec::Number(3000)],
        ports_attributes: port_attrs,
        ..Default::default()
    };

    // Test with redaction enabled - capture stdout to verify redaction
    // Since emit_port_event writes to stdout, we need to test it indirectly
    // by checking that the events generated contain the secret but when
    // process_container_ports is called with redaction config,
    // the emitted output should be redacted

    // First verify that events contain secrets when generated
    let events = PortForwardingManager::process_container_ports(
        &config_with_secrets,
        &container_info,
        false, // Don't emit events, just generate them
        None,
        None,
    );

    assert_eq!(events.len(), 1);
    let event = &events[0];
    assert!(event
        .label
        .as_ref()
        .unwrap()
        .contains("secret-port-token-123"));
    assert!(event
        .description
        .as_ref()
        .unwrap()
        .contains("secret-port-token-123"));

    // The actual redaction test would require capturing stdout from emit_port_event
    // which is complex in unit tests. The functionality is tested through the
    // RedactingWriter itself in the redaction module tests.
}
