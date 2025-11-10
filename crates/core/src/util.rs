//! Shared utility functions for deacon-core.

/// Generate a unique identifier with a prefix.
///
/// Combines the prefix with the current process ID and a monotonic timestamp
/// to reduce the chance of collisions when generating resource names.
///
/// This is useful for creating unique container names, temporary directories,
/// or other resources that need to avoid conflicts in parallel execution.
///
/// # Arguments
///
/// * `prefix` - A string prefix for the identifier
///
/// # Returns
///
/// A string in the format `{prefix}-{pid}-{nanos}`
///
/// # Examples
///
/// ```
/// use deacon_core::util::unique_identifier;
///
/// let name = unique_identifier("deacon-test");
/// assert!(name.starts_with("deacon-test-"));
/// ```
pub fn unique_identifier(prefix: &str) -> String {
    let pid = std::process::id();
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("{}-{}-{}", prefix, pid, nanos)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_unique_identifier_format() {
        let id = unique_identifier("test");
        assert!(id.starts_with("test-"));
        assert!(id.contains('-'));

        // Should contain at least 3 parts separated by dashes
        let parts: Vec<&str> = id.split('-').collect();
        assert!(parts.len() >= 3);
    }

    #[test]
    fn test_unique_identifier_different() {
        let id1 = unique_identifier("test");
        // Small delay to ensure different timestamp
        std::thread::sleep(std::time::Duration::from_millis(1));
        let id2 = unique_identifier("test");

        // IDs should be different due to different timestamps
        assert_ne!(id1, id2);
    }
}
