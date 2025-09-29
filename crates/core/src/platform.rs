//! Platform detection and cross-platform utilities
//!
//! This module provides functionality for detecting the current platform environment
//! and converting paths for cross-platform compatibility, particularly for Windows
//! and WSL environments with Docker Desktop integration.

use std::fs;
use std::path::Path;
use tracing::{debug, instrument};

/// Platform types supported by deacon
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Platform {
    /// Native Linux
    Linux,
    /// macOS
    MacOS,
    /// Native Windows
    Windows,
    /// Windows Subsystem for Linux
    WSL,
}

impl Platform {
    /// Detect the current platform environment
    #[instrument]
    pub fn detect() -> Self {
        if cfg!(target_os = "windows") {
            return Platform::Windows;
        }

        if cfg!(target_os = "macos") {
            return Platform::MacOS;
        }

        if cfg!(target_os = "linux") {
            // Check if we're running in WSL by examining /proc/version
            if Self::is_wsl() {
                Platform::WSL
            } else {
                Platform::Linux
            }
        } else {
            // Fallback to Linux for unknown Unix-like systems
            Platform::Linux
        }
    }

    /// Check if the current environment is WSL
    fn is_wsl() -> bool {
        // WSL has "Microsoft" in /proc/version
        if let Ok(version_content) = fs::read_to_string("/proc/version") {
            let is_wsl = version_content.to_lowercase().contains("microsoft");
            debug!(
                "WSL detection: {} (found 'microsoft' in /proc/version: {})",
                is_wsl, is_wsl
            );
            is_wsl
        } else {
            debug!("WSL detection: false (unable to read /proc/version)");
            false
        }
    }

    /// Check if this platform requires Docker Desktop path conversion
    pub fn needs_docker_desktop_path_conversion(self) -> bool {
        matches!(self, Platform::Windows | Platform::WSL)
    }

    /// Check if this platform supports full container capabilities
    pub fn supports_full_capabilities(self) -> bool {
        matches!(self, Platform::Linux | Platform::MacOS)
    }

    /// Check if this platform supports full user remapping
    pub fn supports_full_user_remapping(self) -> bool {
        matches!(self, Platform::Linux | Platform::MacOS)
    }
}

/// Convert Windows-style paths to Docker Desktop compatible paths
///
/// Docker Desktop on Windows and WSL expects paths in Unix format with drive letters
/// converted from `C:\path` to `/c/path` or `/mnt/c/path` format.
#[instrument]
pub fn convert_path_for_docker_desktop(path: &Path) -> String {
    let path_str = path.to_string_lossy();

    // If it's already a Unix-style path, return as-is
    if path_str.starts_with('/') {
        return path_str.to_string();
    }

    // Handle Windows long path prefixes
    if let Some(rest) = path_str.strip_prefix(r"\\?\") {
        // Handle UNC paths: \\?\UNC\server\share\path -> //server/share/path
        if let Some(unc_path) = rest.strip_prefix("UNC\\") {
            return format!("//{}", unc_path.replace('\\', "/"));
        }
        // Handle long drive paths: \\?\C:\path -> /c/path
        return convert_windows_path_to_docker(rest);
    }

    // Handle standard Windows paths
    if path_str.len() >= 2 && path_str.chars().nth(1) == Some(':') {
        return convert_windows_path_to_docker(&path_str);
    }

    // If it doesn't match Windows patterns, return the original path
    // This handles relative paths and other Unix-style paths
    path_str.replace('\\', "/")
}

/// Convert a Windows path to Docker Desktop format
fn convert_windows_path_to_docker(windows_path: &str) -> String {
    if windows_path.len() >= 2 && windows_path.chars().nth(1) == Some(':') {
        let drive_letter = windows_path.chars().next().unwrap().to_ascii_lowercase();
        let rest = &windows_path[2..].replace('\\', "/");

        // Use /drive_letter/path format (Docker Desktop standard)
        if rest.is_empty() || rest == "/" {
            format!("/{}", drive_letter)
        } else if rest.starts_with('/') {
            format!("/{}{}", drive_letter, rest)
        } else {
            format!("/{}/{}", drive_letter, rest)
        }
    } else {
        // Not a drive-based path, just convert backslashes
        windows_path.replace('\\', "/")
    }
}

/// Normalize line endings in text content for cross-platform compatibility
///
/// Converts CRLF (\r\n) and standalone CR (\r) to LF (\n) to ensure
/// consistent line endings for lifecycle scripts and configuration files.
#[instrument(skip(content))]
pub fn normalize_line_endings(content: &str) -> String {
    // First convert CRLF to LF, then standalone CR to LF
    content.replace("\r\n", "\n").replace('\r', "\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_platform_detection() {
        // We can't test actual platform detection in unit tests easily,
        // but we can test the logic structure
        let platform = Platform::detect();

        // Should be one of the valid platforms
        match platform {
            Platform::Linux | Platform::MacOS | Platform::Windows | Platform::WSL => {
                // This is expected
            }
        }
    }

    #[test]
    fn test_docker_desktop_path_conversion_needs() {
        assert!(!Platform::Linux.needs_docker_desktop_path_conversion());
        assert!(!Platform::MacOS.needs_docker_desktop_path_conversion());
        assert!(Platform::Windows.needs_docker_desktop_path_conversion());
        assert!(Platform::WSL.needs_docker_desktop_path_conversion());
    }

    #[test]
    fn test_capability_support() {
        assert!(Platform::Linux.supports_full_capabilities());
        assert!(Platform::MacOS.supports_full_capabilities());
        assert!(!Platform::Windows.supports_full_capabilities());
        assert!(!Platform::WSL.supports_full_capabilities());
    }

    #[test]
    fn test_user_remapping_support() {
        assert!(Platform::Linux.supports_full_user_remapping());
        assert!(Platform::MacOS.supports_full_user_remapping());
        assert!(!Platform::Windows.supports_full_user_remapping());
        assert!(!Platform::WSL.supports_full_user_remapping());
    }

    #[test]
    fn test_convert_path_for_docker_desktop() {
        // Test Windows paths
        assert_eq!(
            convert_path_for_docker_desktop(Path::new(r"C:\Users\test\project")),
            "/c/Users/test/project"
        );

        assert_eq!(
            convert_path_for_docker_desktop(Path::new(r"D:\dev\workspace")),
            "/d/dev/workspace"
        );

        // Test root drive
        assert_eq!(convert_path_for_docker_desktop(Path::new(r"C:\")), "/c");

        // Test Unix paths (should remain unchanged)
        assert_eq!(
            convert_path_for_docker_desktop(Path::new("/home/user/project")),
            "/home/user/project"
        );

        // Test relative paths
        assert_eq!(
            convert_path_for_docker_desktop(Path::new("./project")),
            "./project"
        );

        // Test long Windows paths
        assert_eq!(
            convert_path_for_docker_desktop(Path::new(r"\\?\C:\very\long\path")),
            "/c/very/long/path"
        );
    }

    #[test]
    fn test_normalize_line_endings() {
        // Test CRLF to LF conversion
        assert_eq!(
            normalize_line_endings("line1\r\nline2\r\nline3"),
            "line1\nline2\nline3"
        );

        // Test standalone CR to LF conversion
        assert_eq!(
            normalize_line_endings("line1\rline2\rline3"),
            "line1\nline2\nline3"
        );

        // Test mixed line endings
        assert_eq!(
            normalize_line_endings("line1\r\nline2\rline3\n"),
            "line1\nline2\nline3\n"
        );

        // Test already normalized content
        assert_eq!(
            normalize_line_endings("line1\nline2\nline3"),
            "line1\nline2\nline3"
        );

        // Test empty content
        assert_eq!(normalize_line_endings(""), "");
    }

    #[test]
    fn test_windows_path_edge_cases() {
        // Test different drive letters
        assert_eq!(
            convert_path_for_docker_desktop(Path::new(r"Z:\project")),
            "/z/project"
        );

        // Test backslash conversion in non-drive paths
        assert_eq!(
            convert_path_for_docker_desktop(Path::new(r"relative\path\file.txt")),
            "relative/path/file.txt"
        );

        // Test single character paths
        assert_eq!(convert_path_for_docker_desktop(Path::new(r"C:")), "/c");
    }

    #[test]
    fn test_unc_path_handling() {
        // Test UNC paths with \\?\UNC\ prefix
        assert_eq!(
            convert_path_for_docker_desktop(Path::new(r"\\?\UNC\server\share\folder")),
            "//server/share/folder"
        );

        // Test UNC paths with nested folders
        assert_eq!(
            convert_path_for_docker_desktop(Path::new(
                r"\\?\UNC\fileserver\documents\project\file.txt"
            )),
            "//fileserver/documents/project/file.txt"
        );

        // Test regular long paths (non-UNC)
        assert_eq!(
            convert_path_for_docker_desktop(Path::new(r"\\?\D:\very\long\path\to\file")),
            "/d/very/long/path/to/file"
        );
    }
}
