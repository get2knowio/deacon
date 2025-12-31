//! Workspace resolution utilities including Git worktree and repository root support
//!
//! This module provides functionality to correctly identify workspace roots,
//! including detection of Git worktrees for proper isolation and container naming,
//! and git repository root detection for `--mount-workspace-git-root` support.

use crate::errors::{DeaconError, Result};
use std::fs;
use std::path::{Path, PathBuf};
use tracing::{debug, instrument};

/// Result of git repository root detection
#[derive(Debug, Clone, PartialEq)]
pub struct GitRootResult {
    /// The detected git repository root
    pub git_root: PathBuf,
    /// Whether this is a git worktree (vs. a regular repository)
    pub is_worktree: bool,
}

/// Resolve the canonical workspace root path
///
/// This function handles both regular directories and Git worktrees:
/// - For regular directories: returns the canonicalized path
/// - For Git worktrees: detects the worktree and returns its root path
///
/// Git worktrees are detected by checking if `.git` is a file (not a directory)
/// that contains a `gitdir:` reference pointing to the worktrees directory.
///
/// **Note**: When invoked from a subdirectory of a git repository, this function
/// will walk up the directory tree to find and return the git repository root.
/// For direct access to git root detection logic, see [`find_git_repository_root`].
///
/// # Arguments
///
/// * `path` - The starting path to resolve (can be a subdirectory)
///
/// # Returns
///
/// Returns the canonical workspace root path. For Git worktrees, this is the
/// worktree root directory, not the main repository root.
///
/// # Example
///
/// ```rust
/// use deacon_core::workspace::resolve_workspace_root;
/// use std::path::Path;
///
/// # fn example() -> anyhow::Result<()> {
/// let workspace = resolve_workspace_root(Path::new("."))?;
/// println!("Workspace root: {}", workspace.display());
/// # Ok(())
/// # }
/// ```
#[instrument]
pub fn resolve_workspace_root(path: &Path) -> Result<PathBuf> {
    debug!("Resolving workspace root for path: {}", path.display());

    // First canonicalize the path to resolve any symlinks and relative paths
    let canonical = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());

    // Check if this is within a Git worktree
    if let Some(worktree_root) = detect_git_worktree(&canonical)? {
        debug!("Detected Git worktree root: {}", worktree_root.display());
        return Ok(worktree_root);
    }

    // Try to find the git repository root (directory containing .git)
    if let Some(result) = find_git_repository_root(&canonical)? {
        debug!("Found git repository root: {}", result.git_root.display());
        return Ok(result.git_root);
    }

    // Return the canonical path as the workspace root
    debug!("Using canonical path as workspace root");
    Ok(canonical)
}

/// Find the git repository root by walking up the directory tree
///
/// This function searches for the directory containing `.git` (whether it's
/// a directory for regular repos or a file for worktrees/submodules).
///
/// # Arguments
///
/// * `path` - The starting path to search from
///
/// # Returns
///
/// Returns `Some(GitRootResult)` with the repository root path if found,
/// or `None` if not within a git repository.
///
/// # Example
///
/// ```rust
/// use deacon_core::workspace::find_git_repository_root;
/// use std::path::Path;
///
/// # fn example() -> anyhow::Result<()> {
/// if let Some(result) = find_git_repository_root(Path::new("."))? {
///     println!("Git root: {}", result.git_root.display());
/// }
/// # Ok(())
/// # }
/// ```
#[instrument]
pub fn find_git_repository_root(path: &Path) -> Result<Option<GitRootResult>> {
    debug!("Finding git repository root for path: {}", path.display());

    // First canonicalize the path
    let canonical = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());

    // Walk up the directory tree looking for .git
    let mut current = canonical.as_path();
    loop {
        let git_path = current.join(".git");

        if git_path.exists() {
            let is_worktree = if git_path.is_file() {
                // Check if this is a worktree by examining the gitdir content
                match parse_git_file(&git_path)? {
                    Some(gitdir) => {
                        let components: Vec<_> = gitdir.components().collect();
                        components.windows(2).any(|window| {
                            if let (
                                std::path::Component::Normal(a),
                                std::path::Component::Normal(b),
                            ) = (window[0], window[1])
                            {
                                a == ".git" && b == "worktrees"
                            } else {
                                false
                            }
                        })
                    }
                    None => false,
                }
            } else {
                false
            };

            debug!(
                "Found git root at: {} (worktree: {})",
                current.display(),
                is_worktree
            );
            return Ok(Some(GitRootResult {
                git_root: current.to_path_buf(),
                is_worktree,
            }));
        }

        // Move up to parent directory
        match current.parent() {
            Some(parent) => current = parent,
            None => {
                debug!("Reached filesystem root without finding .git");
                return Ok(None);
            }
        }
    }
}

/// Detect if the given path is within a Git worktree
///
/// A Git worktree is identified by:
/// 1. A `.git` file (not directory) containing `gitdir: <path>` reference
/// 2. The referenced path contains `worktrees/<name>` in the path
///
/// # Arguments
///
/// * `path` - Path to check for Git worktree
///
/// # Returns
///
/// Returns `Some(PathBuf)` with the worktree root if detected, or `None` if not a worktree.
/// Returns an error if the worktree metadata is inconsistent or unreadable.
#[instrument]
pub fn detect_git_worktree(path: &Path) -> Result<Option<PathBuf>> {
    debug!("Checking for Git worktree at: {}", path.display());

    // Walk up the directory tree looking for .git
    let mut current = path;
    loop {
        let git_path = current.join(".git");

        if git_path.exists() {
            if git_path.is_file() {
                // This might be a worktree - read the .git file
                debug!("Found .git file at: {}", git_path.display());
                match parse_git_file(&git_path)? {
                    Some(gitdir) => {
                        // Check if this is a worktree by examining path components
                        // A worktree has the canonical pattern: .../path/.git/worktrees/<name>
                        let components: Vec<_> = gitdir.components().collect();
                        let is_worktree = components.windows(2).any(|window| {
                            if let (
                                std::path::Component::Normal(a),
                                std::path::Component::Normal(b),
                            ) = (window[0], window[1])
                            {
                                a == ".git" && b == "worktrees"
                            } else {
                                false
                            }
                        });

                        if is_worktree {
                            debug!("Detected worktree pointing to gitdir: {}", gitdir.display());
                            // The current directory is the worktree root
                            return Ok(Some(current.to_path_buf()));
                        } else {
                            debug!("Git file found but not a worktree");
                            return Ok(None);
                        }
                    }
                    None => {
                        debug!("Could not parse gitdir from .git file");
                        return Ok(None);
                    }
                }
            } else if git_path.is_dir() {
                // This is a regular git repository, not a worktree
                debug!("Found regular .git directory at: {}", git_path.display());
                return Ok(None);
            }
        }

        // Move up to parent directory
        match current.parent() {
            Some(parent) => current = parent,
            None => {
                debug!("Reached root without finding .git");
                return Ok(None);
            }
        }
    }
}

/// Parse a Git file that contains a gitdir reference
///
/// Git worktrees use a `.git` file (not directory) that contains:
/// ```text
/// gitdir: /path/to/main/repo/.git/worktrees/<name>
/// ```
///
/// # Arguments
///
/// * `git_file_path` - Path to the .git file
///
/// # Returns
///
/// Returns the gitdir path if successfully parsed, or None if the file format is invalid.
#[instrument]
fn parse_git_file(git_file_path: &Path) -> Result<Option<PathBuf>> {
    debug!("Parsing git file: {}", git_file_path.display());

    let content = fs::read_to_string(git_file_path)
        .map_err(|e| DeaconError::Config(crate::errors::ConfigError::Io(e)))?;

    // Parse the gitdir line: "gitdir: <path>"
    for line in content.lines() {
        let trimmed = line.trim();
        if let Some(stripped) = trimmed.strip_prefix("gitdir:") {
            let gitdir_path = stripped.trim();
            debug!("Extracted gitdir path: {}", gitdir_path);
            return Ok(Some(PathBuf::from(gitdir_path)));
        }
    }

    debug!("No gitdir line found in .git file");
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_resolve_workspace_root_regular_dir() -> anyhow::Result<()> {
        let temp_dir = TempDir::new()?;
        let workspace = resolve_workspace_root(temp_dir.path())?;

        // Should return canonicalized path
        assert!(workspace.exists());
        assert!(workspace.is_absolute());

        Ok(())
    }

    #[test]
    fn test_detect_git_worktree_not_a_worktree() -> anyhow::Result<()> {
        let temp_dir = TempDir::new()?;

        // No .git file/dir at all
        let result = detect_git_worktree(temp_dir.path())?;
        assert_eq!(result, None);

        Ok(())
    }

    #[test]
    fn test_detect_git_worktree_regular_repo() -> anyhow::Result<()> {
        let temp_dir = TempDir::new()?;

        // Create a regular .git directory (not a worktree)
        let git_dir = temp_dir.path().join(".git");
        fs::create_dir(&git_dir)?;

        let result = detect_git_worktree(temp_dir.path())?;
        assert_eq!(result, None);

        Ok(())
    }

    #[test]
    fn test_detect_git_worktree_with_worktree() -> anyhow::Result<()> {
        let temp_dir = TempDir::new()?;

        // Create a .git file pointing to a worktrees directory
        let git_file = temp_dir.path().join(".git");
        let gitdir_content = "gitdir: /path/to/repo/.git/worktrees/my-worktree\n";
        fs::write(&git_file, gitdir_content)?;

        let result = detect_git_worktree(temp_dir.path())?;
        assert!(result.is_some());
        assert_eq!(result.unwrap(), temp_dir.path());

        Ok(())
    }

    #[test]
    fn test_parse_git_file_valid() -> anyhow::Result<()> {
        let temp_dir = TempDir::new()?;
        let git_file = temp_dir.path().join(".git");

        let content = "gitdir: /home/user/repo/.git/worktrees/feature-branch\n";
        fs::write(&git_file, content)?;

        let result = parse_git_file(&git_file)?;
        assert!(result.is_some());
        assert_eq!(
            result.unwrap(),
            PathBuf::from("/home/user/repo/.git/worktrees/feature-branch")
        );

        Ok(())
    }

    #[test]
    fn test_parse_git_file_invalid() -> anyhow::Result<()> {
        let temp_dir = TempDir::new()?;
        let git_file = temp_dir.path().join(".git");

        let content = "some random content\n";
        fs::write(&git_file, content)?;

        let result = parse_git_file(&git_file)?;
        assert_eq!(result, None);

        Ok(())
    }

    #[test]
    fn test_detect_git_worktree_from_subdirectory() -> anyhow::Result<()> {
        let temp_dir = TempDir::new()?;

        // Create a .git file at the root
        let git_file = temp_dir.path().join(".git");
        let gitdir_content = "gitdir: /path/to/repo/.git/worktrees/my-worktree\n";
        fs::write(&git_file, gitdir_content)?;

        // Create a subdirectory
        let subdir = temp_dir.path().join("src").join("components");
        fs::create_dir_all(&subdir)?;

        // Should detect worktree from subdirectory
        let result = detect_git_worktree(&subdir)?;
        assert!(result.is_some());
        assert_eq!(result.unwrap(), temp_dir.path());

        Ok(())
    }

    #[test]
    fn test_detect_git_worktree_false_positive_prevention() -> anyhow::Result<()> {
        let temp_dir = TempDir::new()?;

        // Create a .git file with a path that contains "worktrees" but not in the canonical pattern
        // This tests the fix for false positives from substring matching
        let git_file = temp_dir.path().join(".git");

        // Case 1: "worktrees" appears in a parent directory name, but not after .git
        let gitdir_content = "gitdir: /home/user/my-worktrees-project/.git/modules/submodule\n";
        fs::write(&git_file, gitdir_content)?;

        let result = detect_git_worktree(temp_dir.path())?;
        assert_eq!(
            result, None,
            "Should not detect as worktree when 'worktrees' is in parent path"
        );

        // Case 2: "worktrees" appears as part of another word
        let gitdir_content2 = "gitdir: /home/user/project/.git/my-worktrees-data/info\n";
        fs::write(&git_file, gitdir_content2)?;

        let result2 = detect_git_worktree(temp_dir.path())?;
        assert_eq!(
            result2, None,
            "Should not detect as worktree when 'worktrees' is part of another directory name"
        );

        // Case 3: Proper worktree pattern - should be detected
        let gitdir_content3 = "gitdir: /home/user/project/.git/worktrees/feature-branch\n";
        fs::write(&git_file, gitdir_content3)?;

        let result3 = detect_git_worktree(temp_dir.path())?;
        assert!(result3.is_some(), "Should detect proper worktree pattern");

        Ok(())
    }

    // ============== Tests for find_git_repository_root ==============

    #[test]
    fn test_find_git_repository_root_no_git() -> anyhow::Result<()> {
        let temp_dir = TempDir::new()?;

        // No .git at all
        let result = find_git_repository_root(temp_dir.path())?;
        assert!(
            result.is_none(),
            "Should return None when not in a git repository"
        );

        Ok(())
    }

    #[test]
    fn test_find_git_repository_root_regular_repo() -> anyhow::Result<()> {
        let temp_dir = TempDir::new()?;

        // Create a regular .git directory
        let git_dir = temp_dir.path().join(".git");
        fs::create_dir(&git_dir)?;

        let result = find_git_repository_root(temp_dir.path())?;
        assert!(result.is_some(), "Should find git repository root");

        let git_result = result.unwrap();
        assert_eq!(
            git_result.git_root.canonicalize()?,
            temp_dir.path().canonicalize()?,
            "Git root should be the temp directory"
        );
        assert!(
            !git_result.is_worktree,
            "Regular repo should not be marked as worktree"
        );

        Ok(())
    }

    #[test]
    fn test_find_git_repository_root_from_subdirectory() -> anyhow::Result<()> {
        let temp_dir = TempDir::new()?;

        // Create a regular .git directory at the root
        let git_dir = temp_dir.path().join(".git");
        fs::create_dir(&git_dir)?;

        // Create a subdirectory structure
        let subdir = temp_dir.path().join("src").join("components").join("deep");
        fs::create_dir_all(&subdir)?;

        // Find git root from subdirectory
        let result = find_git_repository_root(&subdir)?;
        assert!(
            result.is_some(),
            "Should find git repository root from subdirectory"
        );

        let git_result = result.unwrap();
        assert_eq!(
            git_result.git_root.canonicalize()?,
            temp_dir.path().canonicalize()?,
            "Git root should be the repo root, not the subdirectory"
        );

        Ok(())
    }

    #[test]
    fn test_find_git_repository_root_worktree() -> anyhow::Result<()> {
        let temp_dir = TempDir::new()?;

        // Create a .git file pointing to a worktrees directory
        let git_file = temp_dir.path().join(".git");
        let gitdir_content = "gitdir: /path/to/repo/.git/worktrees/my-worktree\n";
        fs::write(&git_file, gitdir_content)?;

        let result = find_git_repository_root(temp_dir.path())?;
        assert!(result.is_some(), "Should find worktree root");

        let git_result = result.unwrap();
        assert_eq!(
            git_result.git_root.canonicalize()?,
            temp_dir.path().canonicalize()?,
            "Git root should be the worktree root"
        );
        assert!(git_result.is_worktree, "Should be marked as worktree");

        Ok(())
    }

    #[test]
    fn test_resolve_workspace_root_finds_git_root_from_subdir() -> anyhow::Result<()> {
        let temp_dir = TempDir::new()?;

        // Create a regular .git directory at the root
        let git_dir = temp_dir.path().join(".git");
        fs::create_dir(&git_dir)?;

        // Create a subdirectory
        let subdir = temp_dir.path().join("src").join("lib");
        fs::create_dir_all(&subdir)?;

        // resolve_workspace_root should find the git root when called from subdir
        let workspace = resolve_workspace_root(&subdir)?;
        assert_eq!(
            workspace.canonicalize()?,
            temp_dir.path().canonicalize()?,
            "resolve_workspace_root should find git repository root from subdirectory"
        );

        Ok(())
    }
}
