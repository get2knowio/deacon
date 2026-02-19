//! Registry reference parsing utilities
//!
//! Shared utilities for parsing registry references across different commands.

use anyhow::Result;

/// Parse a registry reference into its components
/// Supports formats like:
/// - ghcr.io/devcontainers/features/node:18
/// - registry.com/namespace/subnamespace/name
/// - localhost:5000/namespace/name (registry with port)
/// - namespace/subnamespace/name (assumes default registry)
/// - simple-name (assumes default registry and namespace)
/// - ghcr.io/org/feature/name@sha256:digest (digest references)
pub fn parse_registry_reference(
    registry_ref: &str,
) -> Result<(String, String, String, Option<String>)> {
    // Default values
    let default_registry = "ghcr.io";
    let default_namespace = "devcontainers";

    // Split by '/' to separate registry, namespace, and name
    let parts: Vec<&str> = registry_ref.split('/').collect();

    match parts.len() {
        1 => {
            // Format: name[:tag]
            let (name, tag) = parse_name_and_tag(parts[0]);
            Ok((
                default_registry.to_string(),
                default_namespace.to_string(),
                name.to_string(),
                tag.map(|t| t.to_string()),
            ))
        }
        2 => {
            // Format: registry/name or namespace/name[:tag]
            // Check if the first part looks like a registry (contains a dot or port number)
            if looks_like_registry(parts[0]) {
                // First part is a registry, use default namespace
                let (name, tag) = parse_name_and_tag(parts[1]);
                Ok((
                    parts[0].to_string(),
                    default_namespace.to_string(),
                    name.to_string(),
                    tag.map(|t| t.to_string()),
                ))
            } else {
                // First part is a namespace, use default registry
                let (name, tag) = parse_name_and_tag(parts[1]);
                Ok((
                    default_registry.to_string(),
                    parts[0].to_string(),
                    name.to_string(),
                    tag.map(|t| t.to_string()),
                ))
            }
        }
        _ => {
            // Format: registry/namespace/.../name[:tag] or namespace/.../name[:tag]
            // Check if the first part looks like a registry (contains a dot or port number)
            if looks_like_registry(parts[0]) {
                // First part is registry, rest is namespace + name
                let registry = parts[0];
                let remaining_parts = &parts[1..];
                let (namespace, name_and_tag) = parse_namespace_and_name(remaining_parts);
                let (name, tag) = parse_name_and_tag(name_and_tag);
                Ok((
                    registry.to_string(),
                    namespace,
                    name.to_string(),
                    tag.map(|t| t.to_string()),
                ))
            } else {
                // No registry, first parts are namespace, last is name
                let (namespace, name_and_tag) = parse_namespace_and_name(&parts);
                let (name, tag) = parse_name_and_tag(name_and_tag);
                Ok((
                    default_registry.to_string(),
                    namespace,
                    name.to_string(),
                    tag.map(|t| t.to_string()),
                ))
            }
        }
    }
}

/// Check if a string looks like a registry hostname.
/// A registry has a dot (e.g. ghcr.io) or a colon followed by digits (e.g. localhost:5000).
fn looks_like_registry(s: &str) -> bool {
    if s.contains('.') {
        return true;
    }
    // Check for host:port pattern (e.g. "localhost:5000")
    if let Some(colon_pos) = s.find(':') {
        let after_colon = &s[colon_pos + 1..];
        return !after_colon.is_empty() && after_colon.chars().all(|c| c.is_ascii_digit());
    }
    false
}

/// Parse namespace and name from parts, where namespace can have multiple levels
/// Returns (namespace, name_and_tag)
fn parse_namespace_and_name<'a>(parts: &[&'a str]) -> (String, &'a str) {
    // Last part is always name[:tag], everything before is namespace
    let name_and_tag = parts[parts.len() - 1];
    let namespace_parts = &parts[..parts.len() - 1];
    let namespace = namespace_parts.join("/");
    (namespace, name_and_tag)
}

/// Parse name and tag from a name[:tag] or name[@digest] string
///
/// Handles both tag and digest references:
/// - `node:18` -> ("node", Some("18"))
/// - `node@sha256:abc123` -> ("node", Some("sha256:abc123"))
/// - `node` -> ("node", None)
pub fn parse_name_and_tag(name_and_tag: &str) -> (&str, Option<&str>) {
    // Check for digest reference first (name@sha256:...)
    if let Some(at_pos) = name_and_tag.find('@') {
        let name = &name_and_tag[..at_pos];
        let digest = &name_and_tag[at_pos + 1..];
        return (name, Some(digest));
    }
    if let Some(colon_pos) = name_and_tag.rfind(':') {
        let name = &name_and_tag[..colon_pos];
        let tag = &name_and_tag[colon_pos + 1..];
        (name, Some(tag))
    } else {
        (name_and_tag, None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_registry_reference() {
        // Test full reference
        let (registry, namespace, name, tag) =
            parse_registry_reference("ghcr.io/devcontainers/node:18").unwrap();
        assert_eq!(registry, "ghcr.io");
        assert_eq!(namespace, "devcontainers");
        assert_eq!(name, "node");
        assert_eq!(tag, Some("18".to_string()));

        // Test registry + multi-level namespace + name
        let (registry, namespace, name, tag) =
            parse_registry_reference("ghcr.io/devcontainers/features/node:1").unwrap();
        assert_eq!(registry, "ghcr.io");
        assert_eq!(namespace, "devcontainers/features");
        assert_eq!(name, "node");
        assert_eq!(tag, Some("1".to_string()));

        // Test registry + name (use default namespace)
        let (registry, namespace, name, tag) =
            parse_registry_reference("ghcr.io/myfeature").unwrap();
        assert_eq!(registry, "ghcr.io");
        assert_eq!(namespace, "devcontainers");
        assert_eq!(name, "myfeature");
        assert_eq!(tag, None);

        // Test multi-level namespace + name (use default registry)
        let (registry, namespace, name, tag) =
            parse_registry_reference("myorg/myproject/myfeature").unwrap();
        assert_eq!(registry, "ghcr.io");
        assert_eq!(namespace, "myorg/myproject");
        assert_eq!(name, "myfeature");
        assert_eq!(tag, None);

        // Test namespace + name (use default registry)
        let (registry, namespace, name, tag) =
            parse_registry_reference("myteam/myfeature").unwrap();
        assert_eq!(registry, "ghcr.io");
        assert_eq!(namespace, "myteam");
        assert_eq!(name, "myfeature");
        assert_eq!(tag, None);

        // Test name only
        let (registry, namespace, name, tag) = parse_registry_reference("myfeature").unwrap();
        assert_eq!(registry, "ghcr.io");
        assert_eq!(namespace, "devcontainers");
        assert_eq!(name, "myfeature");
        assert_eq!(tag, None);
    }

    #[test]
    fn test_parse_name_and_tag() {
        assert_eq!(parse_name_and_tag("node"), ("node", None));
        assert_eq!(parse_name_and_tag("node:18"), ("node", Some("18")));
        assert_eq!(
            parse_name_and_tag("myfeature:latest"),
            ("myfeature", Some("latest"))
        );
    }
}
