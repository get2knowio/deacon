//! Workspace resolution utilities including Git worktree and repository root support
//!
//! This module provides functionality to correctly identify workspace roots,
//! including detection of Git worktrees for proper isolation and container naming,
//! and git repository root detection for `--mount-workspace-git-root` support.

use crate::errors::{DeaconError, Result};
use std::fs;
use std::path::{Path, PathBuf};
use tracing::{debug, instrument};

/// Make `path` absolute against the current directory and collapse `.` / `..`
/// **lexically** — without resolving symlinks.
///
/// This is the workspace path's canonical form everywhere deacon reports it, mounts it,
/// or hashes it into a container identity. It is deliberately NOT
/// [`std::fs::canonicalize`]: the reference CLI preserves the path the user named, using
/// `git rev-parse --show-cdup` rather than `--show-toplevel` for exactly that reason
/// (`spec-common/git.ts:24`, "Preserves symlinked paths"), and the spec defines
/// `${localWorkspaceFolder}` as the path of the folder *that was opened*
/// (`devcontainerjson-reference.md:157`). deacon used to canonicalize, so a workspace
/// reached through a symlink was silently renamed to its real path — issue #665.
///
/// Collapsing `..` textually is what `path.resolve()` does on the reference's side. It can
/// differ from the kernel's answer when a component is a symlink, and that is the point:
/// following the link is the behavior being removed.
pub fn absolutize(path: &Path) -> PathBuf {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        match std::env::current_dir() {
            Ok(cwd) => cwd.join(path),
            Err(e) => {
                debug!("No current directory to absolutize against ({e}); using path as given");
                path.to_path_buf()
            }
        }
    };
    normalize_lexically(&absolute)
}

/// Collapse `.` and `..` components textually, touching no filesystem.
fn normalize_lexically(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            std::path::Component::ParentDir => {
                out.pop();
            }
            std::path::Component::CurDir => {}
            other => out.push(other.as_os_str()),
        }
    }
    out
}

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

    // Absolutize WITHOUT resolving symlinks: the reported root, the mount source and the
    // identity hash all derive from this, and the reference preserves the path as named
    // (#665). See [`absolutize`].
    let canonical = absolutize(path);

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

/// Compute the container-side workspace folder (`${containerWorkspaceFolder}`),
/// matching the reference CLI's algorithm (verified against `@devcontainers/cli`).
///
/// - If the config declares an explicit `workspaceFolder`, it is used verbatim.
/// - Otherwise the value is `/workspaces/<basename(root)>[/<subpath>]`, where
///   `root` is the git worktree/repository root when `mount_workspace_git_root`
///   is set (else the workspace folder itself), and `<subpath>` is the path from
///   `root` down to the workspace folder (empty when the workspace *is* the root
///   or is not inside a git repository).
///
/// This is the single source of truth for the value; `read-configuration`, and
/// `up`/`exec`/`run-user-commands` lifecycle working-dir derivation all route
/// through it so the reported and the used value never diverge (issue #309).
pub fn container_workspace_folder(
    workspace_folder: &Path,
    config_workspace_folder: Option<&str>,
    mount_workspace_git_root: bool,
) -> String {
    // (a) explicit workspaceFolder wins verbatim.
    if let Some(wf) = config_workspace_folder {
        if !wf.trim().is_empty() {
            return wf.to_string();
        }
    }

    let canonical_ws = absolutize(workspace_folder);

    // Root the container path at the git root (default) or the workspace folder.
    let root = if mount_workspace_git_root {
        resolve_workspace_root(&canonical_ws).unwrap_or_else(|_| canonical_ws.clone())
    } else {
        canonical_ws.clone()
    };

    let base = root
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("workspace");

    // (c) subpath from the root down to the workspace folder; empty for (d)/(e).
    let subpath = canonical_ws
        .strip_prefix(&root)
        .ok()
        .map(|p| p.to_string_lossy().replace('\\', "/"))
        .filter(|s| !s.is_empty());

    match subpath {
        Some(sub) => format!("/workspaces/{base}/{sub}"),
        None => format!("/workspaces/{base}"),
    }
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

    // Absolutize without resolving symlinks (#665) — walking up from the path as named is
    // what `git rev-parse --show-cdup` does, and `.git` is still reached through the link.
    let canonical = absolutize(path);

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

/// Where a git worktree's common `.git` directory has to be mounted for git to work
/// inside the container, and the container-side folder the worktree itself moves to.
///
/// A worktree's `.git` is a file holding `gitdir: <path>`. When that path is **relative**
/// (`git worktree add --relative-paths`), it only resolves container-side if the worktree
/// and the common dir keep the same relative arrangement they have on the host — which is
/// why the worktree stops being mounted at `/workspaces/<basename>` and moves up to the
/// nearest ancestor that also contains the common dir.
#[derive(Debug, Clone, PartialEq)]
pub struct GitWorktreeCommonDir {
    /// Container path the workspace is bind-mounted at, replacing `/workspaces/<basename>`.
    pub container_mount_folder: String,
    /// Host path of the common `.git` directory.
    pub host_common_dir: PathBuf,
    /// Container path the common `.git` directory must be mounted at.
    pub container_common_dir: String,
}

impl GitWorktreeCommonDir {
    /// The `--mount` specification for the common directory, in the same Docker CLI string
    /// form as every other mount deacon emits (comma-bearing paths quoted, #663).
    pub fn additional_mount_string(&self) -> String {
        format!(
            "type=bind,{},{}",
            crate::mount::format_mount_field("source", &self.host_common_dir.to_string_lossy()),
            crate::mount::format_mount_field("target", &self.container_common_dir),
        )
    }
}

/// Resolve the extra mount `--mount-git-worktree-common-dir` asks for, if this folder is a
/// git worktree created with relative paths.
///
/// Mirrors the reference CLI's `getWorkspaceConfiguration`
/// (`spec-node/utils.ts:390-419`): only a `.git` **file** with a **relative** `gitdir:`
/// qualifies; an absolute one yields `None` and nothing is renamed. `gitdir` names
/// `<common>/worktrees/<name>`, so the common dir is two levels above it.
///
/// Both resolutions are purely lexical, matching `path.resolve` — no filesystem access
/// beyond reading the `.git` file, and no symlink resolution.
pub fn resolve_git_worktree_common_dir(host_mount_folder: &Path) -> Option<GitWorktreeCommonDir> {
    let dot_git = host_mount_folder.join(".git");
    if !dot_git.is_file() {
        return None;
    }
    let gitdir = parse_git_file(&dot_git).ok().flatten()?;
    if gitdir.is_absolute() {
        debug!(
            "Worktree gitdir '{}' is absolute; no common-dir mount",
            gitdir.display()
        );
        return None;
    }

    let host_common_dir = lexical_join(host_mount_folder, &gitdir.join("..").join(".."));

    // Walk up from the workspace until the current directory contains the common dir, so
    // the mount covers both. The segments collected on the way (root-most first) are the
    // container-side path the workspace lands at.
    let mut segments: Vec<String> = Vec::new();
    let mut current = host_mount_folder.to_path_buf();
    loop {
        if host_common_dir.starts_with(&current) && host_common_dir != current {
            break;
        }
        let parent = match current.parent() {
            Some(parent) if parent != current => parent.to_path_buf(),
            _ => break,
        };
        segments.insert(
            0,
            current
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default(),
        );
        current = parent;
    }

    let container_mount_folder = format!("/workspaces/{}", segments.join("/"));
    let container_common_dir = lexical_join(
        Path::new(&container_mount_folder),
        &gitdir.join("..").join(".."),
    )
    .to_string_lossy()
    .replace('\\', "/");

    Some(GitWorktreeCommonDir {
        container_mount_folder,
        host_common_dir,
        container_common_dir,
    })
}

/// Join `relative` onto `base` and collapse `.` / `..` textually, the way `path.resolve`
/// does. `Path::join` alone leaves the `..` components in the result.
fn lexical_join(base: &Path, relative: &Path) -> PathBuf {
    normalize_lexically(&base.join(relative))
}

/// [`container_workspace_folder`], re-based when the worktree's common dir is mounted.
///
/// The reference computes the container-side workspace folder from the *mount folder*, so
/// once `--mount-git-worktree-common-dir` moves that mount up the tree, the workspace
/// folder moves with it — `/workspaces/worktrees/feature/packages/app` rather than
/// `/workspaces/feature/packages/app`. An authored `workspaceFolder` still wins verbatim.
pub fn container_workspace_folder_for_worktree(
    workspace_folder: &Path,
    config_workspace_folder: Option<&str>,
    mount_workspace_git_root: bool,
    worktree: Option<&GitWorktreeCommonDir>,
) -> String {
    let Some(worktree) = worktree else {
        return container_workspace_folder(
            workspace_folder,
            config_workspace_folder,
            mount_workspace_git_root,
        );
    };
    if let Some(wf) = config_workspace_folder {
        if !wf.trim().is_empty() {
            return wf.to_string();
        }
    }

    let canonical_ws = absolutize(workspace_folder);
    let root = if mount_workspace_git_root {
        resolve_workspace_root(&canonical_ws).unwrap_or_else(|_| canonical_ws.clone())
    } else {
        canonical_ws.clone()
    };

    match canonical_ws
        .strip_prefix(&root)
        .ok()
        .map(|p| p.to_string_lossy().replace('\\', "/"))
        .filter(|s| !s.is_empty())
    {
        Some(sub) => format!("{}/{}", worktree.container_mount_folder, sub),
        None => worktree.container_mount_folder.clone(),
    }
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
        let path = Path::new("/deacon-test-path-outside-any-repository");

        // No .git at all
        let result = find_git_repository_root(path)?;
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

    // container_workspace_folder — matches the reference CLI (issue #309).
    // Oracle-verified against @devcontainers/cli@0.87.0 for all four cases.

    #[test]
    fn test_container_workspace_folder_explicit_wins_verbatim() -> anyhow::Result<()> {
        // (a) An explicit workspaceFolder is used as-is, ignoring git/subpath.
        let temp = TempDir::new()?;
        let sub = temp.path().join("sub").join("pkg");
        fs::create_dir_all(&sub)?;
        assert_eq!(
            container_workspace_folder(&sub, Some("/opt/app"), true),
            "/opt/app"
        );
        Ok(())
    }

    #[test]
    fn test_container_workspace_folder_git_subdir_appends_subpath() -> anyhow::Result<()> {
        // (c) git root + subdir → /workspaces/<gitRootBasename>/<subpath>.
        let temp = TempDir::new()?;
        fs::create_dir(temp.path().join(".git"))?;
        let sub = temp.path().join("sub").join("pkg");
        fs::create_dir_all(&sub)?;
        let base = temp
            .path()
            .canonicalize()?
            .file_name()
            .unwrap()
            .to_string_lossy()
            .to_string();
        assert_eq!(
            container_workspace_folder(&sub, None, true),
            format!("/workspaces/{base}/sub/pkg")
        );
        Ok(())
    }

    #[test]
    fn test_container_workspace_folder_at_git_root_no_subpath() -> anyhow::Result<()> {
        // (d) workspace IS the git root → /workspaces/<basename> (no subpath).
        let temp = TempDir::new()?;
        fs::create_dir(temp.path().join(".git"))?;
        let base = temp
            .path()
            .canonicalize()?
            .file_name()
            .unwrap()
            .to_string_lossy()
            .to_string();
        assert_eq!(
            container_workspace_folder(temp.path(), None, true),
            format!("/workspaces/{base}")
        );
        Ok(())
    }

    #[test]
    fn test_container_workspace_folder_non_git_uses_basename() -> anyhow::Result<()> {
        // (e) not in a git repo → /workspaces/<workspaceFolderBasename>.
        let temp = TempDir::new()?;
        let ws = temp.path().join("myproj");
        fs::create_dir_all(&ws)?;
        assert_eq!(
            container_workspace_folder(&ws, None, true),
            "/workspaces/myproj"
        );
        // With git-root mounting disabled the workspace folder is the root, so a
        // git-subdir still resolves to its own basename (no walk, no subpath).
        assert_eq!(
            container_workspace_folder(&ws, None, false),
            "/workspaces/myproj"
        );
        Ok(())
    }
}

#[cfg(test)]
mod worktree_common_dir_tests {
    //! `--mount-git-worktree-common-dir` (#664). The trees here are built by hand rather
    //! than by `git worktree add --relative-paths`: the resolution reads the `.git` file and
    //! resolves lexically, so it needs no git binary — and `--relative-paths` only exists
    //! from git 2.48, which no CI runner is guaranteed to have.

    use super::*;
    use std::fs;
    use tempfile::TempDir;

    /// Lay out `<root>/<repo>/.git/worktrees/<name>` and a worktree at `<root>/<worktree>`
    /// whose `.git` file holds `gitdir`.
    fn worktree(root: &Path, repo: &str, worktree: &str, gitdir: &str) -> PathBuf {
        fs::create_dir_all(root.join(repo).join(".git").join("worktrees")).unwrap();
        let worktree_path = root.join(worktree);
        fs::create_dir_all(&worktree_path).unwrap();
        fs::write(worktree_path.join(".git"), format!("gitdir: {}\n", gitdir)).unwrap();
        worktree_path
    }

    #[test]
    fn a_sibling_worktree_moves_nothing_and_mounts_the_common_dir_beside_it() {
        let temp = TempDir::new().unwrap();
        let root = temp.path();
        let path = worktree(
            root,
            "mainrepo",
            "feature",
            "../mainrepo/.git/worktrees/feature",
        );

        let resolved = resolve_git_worktree_common_dir(&path).expect("relative gitdir resolves");
        assert_eq!(resolved.container_mount_folder, "/workspaces/feature");
        assert_eq!(resolved.host_common_dir, root.join("mainrepo").join(".git"));
        assert_eq!(resolved.container_common_dir, "/workspaces/mainrepo/.git");
        assert_eq!(
            resolved.additional_mount_string(),
            format!(
                "type=bind,source={},target=/workspaces/mainrepo/.git",
                root.join("mainrepo").join(".git").display()
            )
        );
    }

    #[test]
    fn a_worktree_two_levels_down_is_mounted_from_the_common_ancestor() {
        let temp = TempDir::new().unwrap();
        let root = temp.path();
        let path = worktree(
            root,
            "repos/main",
            "worktrees/feature",
            "../../repos/main/.git/worktrees/feature",
        );

        let resolved = resolve_git_worktree_common_dir(&path).expect("relative gitdir resolves");
        // NOT `/workspaces/feature`: the worktree moves up so the sibling repo fits beside it,
        // which is the only arrangement in which the relative `gitdir` still resolves.
        assert_eq!(
            resolved.container_mount_folder,
            "/workspaces/worktrees/feature"
        );
        assert_eq!(resolved.container_common_dir, "/workspaces/repos/main/.git");
        assert_eq!(
            resolved.host_common_dir,
            root.join("repos").join("main").join(".git")
        );
    }

    #[test]
    fn an_absolute_gitdir_is_left_alone() {
        let temp = TempDir::new().unwrap();
        let root = temp.path();
        let absolute = root.join("mainrepo/.git/worktrees/absfeat");
        let path = worktree(root, "mainrepo", "absfeat", &absolute.display().to_string());

        // The reference only handles the relative form; an absolute gitdir already resolves
        // to nothing container-side and renaming the mount would not help.
        assert!(resolve_git_worktree_common_dir(&path).is_none());
    }

    #[test]
    fn an_ordinary_repository_is_not_a_worktree() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("repo");
        fs::create_dir_all(path.join(".git")).unwrap();
        assert!(resolve_git_worktree_common_dir(&path).is_none());

        let no_git = temp.path().join("plain");
        fs::create_dir_all(&no_git).unwrap();
        assert!(resolve_git_worktree_common_dir(&no_git).is_none());
    }

    #[test]
    fn the_container_workspace_folder_follows_the_relocated_mount() {
        let temp = TempDir::new().unwrap();
        let root = temp.path();
        let path = worktree(
            root,
            "repos/main",
            "worktrees/feature",
            "../../repos/main/.git/worktrees/feature",
        );
        let sub = path.join("packages").join("app");
        fs::create_dir_all(&sub).unwrap();
        let resolved = resolve_git_worktree_common_dir(&path).unwrap();

        assert_eq!(
            container_workspace_folder_for_worktree(&sub, None, true, Some(&resolved)),
            "/workspaces/worktrees/feature/packages/app"
        );
        // An authored `workspaceFolder` still wins verbatim…
        assert_eq!(
            container_workspace_folder_for_worktree(&sub, Some("/opt/app"), true, Some(&resolved)),
            "/opt/app"
        );
        // …and with no worktree the answer is the unrelocated default.
        assert_eq!(
            container_workspace_folder_for_worktree(&sub, None, true, None),
            container_workspace_folder(&sub, None, true)
        );
    }
}

#[cfg(test)]
mod absolutize_tests {
    //! [`absolutize`] is the workspace path's canonical form since #665: absolute and
    //! lexically normalized, but never symlink-resolved.

    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn an_absolute_path_keeps_its_spelling() {
        let temp = TempDir::new().unwrap();
        assert_eq!(absolutize(temp.path()), temp.path());
    }

    #[test]
    fn dot_and_dotdot_are_collapsed_textually() {
        let temp = TempDir::new().unwrap();
        let noisy = temp.path().join(".").join("b").join("..").join("c");
        assert_eq!(absolutize(&noisy), temp.path().join("c"));
    }

    /// POSIX spellings only: on Windows a rooted-but-prefixless path picks up the cwd's
    /// drive prefix, so `/a/b/c` is not the same path there.
    #[test]
    #[cfg(unix)]
    fn posix_roots_are_preserved_and_cannot_be_escaped() {
        assert_eq!(absolutize(Path::new("/a/b/c")), PathBuf::from("/a/b/c"));
        assert_eq!(absolutize(Path::new("/a/./b/../c")), PathBuf::from("/a/c"));
        // Popping past the root stays at the root rather than escaping it.
        assert_eq!(absolutize(Path::new("/../..")), PathBuf::from("/"));
    }

    #[test]
    fn a_relative_path_is_rooted_at_the_current_directory() {
        let cwd = std::env::current_dir().unwrap();
        assert_eq!(
            absolutize(Path::new("sub/dir")),
            cwd.join("sub").join("dir")
        );
        assert_eq!(absolutize(Path::new(".")), cwd);
    }

    /// The whole point of #665: the reference preserves the path the user named, so a
    /// workspace reached through a symlink keeps the link's spelling. `canonicalize` is
    /// what this function exists NOT to be.
    #[test]
    #[cfg(unix)]
    fn a_symlink_is_not_followed() {
        let temp = TempDir::new().unwrap();
        let real = temp.path().join("real");
        fs::create_dir_all(&real).unwrap();
        let link = temp.path().join("link");
        std::os::unix::fs::symlink(&real, &link).unwrap();

        assert_eq!(absolutize(&link), link);
        assert_ne!(absolutize(&link), link.canonicalize().unwrap());
        // …and the basename the container path is built from follows the link's name.
        assert_eq!(
            absolutize(&link).file_name().and_then(|n| n.to_str()),
            Some("link")
        );
    }

    /// A path that does not exist still absolutizes — `absolutize` touches no filesystem,
    /// and the existence check belongs to the callers that want one.
    #[test]
    fn a_missing_path_still_resolves() {
        let temp = TempDir::new().unwrap();
        let missing = temp.path().join("nope").join("..").join("nope");
        assert_eq!(absolutize(&missing), temp.path().join("nope"));
    }
}
