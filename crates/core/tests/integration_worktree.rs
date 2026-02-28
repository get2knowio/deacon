//! Integration tests for Git worktree support in workspace resolution

use deacon_core::config::{ConfigLoader, DevContainerConfig, DiscoveryResult};
use deacon_core::container::ContainerIdentity;
use deacon_core::observability::workspace_id;
use deacon_core::workspace::resolve_workspace_root;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use tempfile::TempDir;

/// Helper to check if git is available in the test environment
fn is_git_available() -> bool {
    Command::new("git")
        .arg("--version")
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

/// Helper to configure git for tests
fn configure_test_git_repo(repo_path: &Path) -> Result<(), std::io::Error> {
    Command::new("git")
        .current_dir(repo_path)
        .args(["config", "user.email", "test@example.com"])
        .output()?;
    Command::new("git")
        .current_dir(repo_path)
        .args(["config", "user.name", "Test User"])
        .output()?;
    Ok(())
}

/// Helper to create a git repository with a worktree
fn setup_git_worktree_fixture() -> Result<(TempDir, PathBuf, PathBuf), Box<dyn std::error::Error>> {
    // Create main repo
    let main_repo = TempDir::new()?;
    let main_path = main_repo.path().to_path_buf();

    // Initialize git repo
    Command::new("git")
        .current_dir(&main_path)
        .args(["init"])
        .output()?;

    configure_test_git_repo(&main_path)?;

    // Create initial commit
    let readme = main_path.join("README.md");
    fs::write(&readme, "# Main Repository\n")?;

    Command::new("git")
        .current_dir(&main_path)
        .args(["add", "README.md"])
        .output()?;

    Command::new("git")
        .current_dir(&main_path)
        .args(["commit", "-m", "Initial commit"])
        .output()?;

    // Create a worktree
    let worktree_path = main_path.join("../worktree-feature");
    Command::new("git")
        .current_dir(&main_path)
        .args([
            "worktree",
            "add",
            worktree_path.to_str().unwrap(),
            "-b",
            "feature-branch",
        ])
        .output()?;

    let canonical_worktree_path = worktree_path.canonicalize()?;

    Ok((main_repo, main_path, canonical_worktree_path))
}

#[test]
fn test_resolve_workspace_root_detects_worktree() {
    if !is_git_available() {
        eprintln!("Skipping test: git is not available");
        return;
    }

    let result = setup_git_worktree_fixture();
    if result.is_err() {
        eprintln!(
            "Skipping test: Could not set up git worktree: {:?}",
            result.err()
        );
        return;
    }

    let (_main_repo, _main_path, worktree_path) = result.unwrap();

    // Test that resolve_workspace_root correctly identifies the worktree root
    let resolved = resolve_workspace_root(&worktree_path).unwrap();
    assert_eq!(resolved, worktree_path);

    // Test from a subdirectory within the worktree
    let subdir = worktree_path.join("src");
    fs::create_dir_all(&subdir).unwrap();

    let resolved_from_subdir = resolve_workspace_root(&subdir).unwrap();
    assert_eq!(resolved_from_subdir, worktree_path);
}

#[test]
fn test_container_identity_worktree_isolation() {
    if !is_git_available() {
        eprintln!("Skipping test: git is not available");
        return;
    }

    let result = setup_git_worktree_fixture();
    if result.is_err() {
        eprintln!(
            "Skipping test: Could not set up git worktree: {:?}",
            result.err()
        );
        return;
    }

    let (_main_repo, main_path, worktree_path) = result.unwrap();

    let config = DevContainerConfig {
        name: Some("test-container".to_string()),
        image: Some("ubuntu:20.04".to_string()),
        ..Default::default()
    };

    // Create container identities for both main repo and worktree
    let main_identity = ContainerIdentity::new(&main_path, &config);
    let worktree_identity = ContainerIdentity::new(&worktree_path, &config);

    // Workspace hashes should be different to ensure isolation
    assert_ne!(
        main_identity.workspace_hash, worktree_identity.workspace_hash,
        "Main repo and worktree should have different workspace hashes"
    );

    // Container names should be different
    assert_ne!(
        main_identity.container_name(),
        worktree_identity.container_name(),
        "Main repo and worktree should have different container names"
    );

    // Labels should be different
    let main_labels = main_identity.labels();
    let worktree_labels = worktree_identity.labels();

    assert_ne!(
        main_labels.get("devcontainer.workspaceHash"),
        worktree_labels.get("devcontainer.workspaceHash"),
        "Workspace hash labels should differ"
    );
}

#[test]
fn test_workspace_id_worktree_isolation() {
    if !is_git_available() {
        eprintln!("Skipping test: git is not available");
        return;
    }

    let result = setup_git_worktree_fixture();
    if result.is_err() {
        eprintln!(
            "Skipping test: Could not set up git worktree: {:?}",
            result.err()
        );
        return;
    }

    let (_main_repo, main_path, worktree_path) = result.unwrap();

    // Generate workspace IDs for both
    let main_id = workspace_id(&main_path);
    let worktree_id = workspace_id(&worktree_path);

    // Should be different to ensure isolation
    assert_ne!(
        main_id, worktree_id,
        "Main repo and worktree should have different workspace IDs for observability"
    );
}

#[test]
fn test_config_discovery_in_worktree() {
    if !is_git_available() {
        eprintln!("Skipping test: git is not available");
        return;
    }

    let result = setup_git_worktree_fixture();
    if result.is_err() {
        eprintln!(
            "Skipping test: Could not set up git worktree: {:?}",
            result.err()
        );
        return;
    }

    let (_main_repo, _main_path, worktree_path) = result.unwrap();

    // Create a devcontainer config in the worktree
    let devcontainer_dir = worktree_path.join(".devcontainer");
    fs::create_dir_all(&devcontainer_dir).unwrap();

    let config_path = devcontainer_dir.join("devcontainer.json");
    let config_content = r#"{
        "name": "worktree-container",
        "image": "ubuntu:22.04"
    }"#;
    fs::write(&config_path, config_content).unwrap();

    // Discover config should find it in the worktree
    let discovered = ConfigLoader::discover_config(&worktree_path).unwrap();
    assert_eq!(discovered, DiscoveryResult::Single(config_path));
}

#[test]
fn test_multiple_worktrees_isolation() {
    if !is_git_available() {
        eprintln!("Skipping test: git is not available");
        return;
    }

    // Create main repo
    let main_repo = TempDir::new();
    if main_repo.is_err() {
        eprintln!("Skipping test: Could not create temp directory");
        return;
    }
    let main_repo = main_repo.unwrap();
    let main_path = main_repo.path().to_path_buf();

    // Initialize git repo
    if Command::new("git")
        .current_dir(&main_path)
        .args(["init"])
        .output()
        .is_err()
    {
        eprintln!("Skipping test: git init failed");
        return;
    }

    if configure_test_git_repo(&main_path).is_err() {
        eprintln!("Skipping test: git config failed");
        return;
    }

    // Create initial commit
    let readme = main_path.join("README.md");
    if fs::write(&readme, "# Main Repository\n").is_err() {
        eprintln!("Skipping test: Could not write README");
        return;
    }

    if Command::new("git")
        .current_dir(&main_path)
        .args(["add", "README.md"])
        .output()
        .is_err()
    {
        eprintln!("Skipping test: git add failed");
        return;
    }

    if Command::new("git")
        .current_dir(&main_path)
        .args(["commit", "-m", "Initial commit"])
        .output()
        .is_err()
    {
        eprintln!("Skipping test: git commit failed");
        return;
    }

    // Create two worktrees
    let worktree1_path = main_path.join("../worktree1");
    if Command::new("git")
        .current_dir(&main_path)
        .args([
            "worktree",
            "add",
            worktree1_path.to_str().unwrap(),
            "-b",
            "feature1",
        ])
        .output()
        .is_err()
    {
        eprintln!("Skipping test: Could not create worktree 1");
        return;
    }

    let worktree2_path = main_path.join("../worktree2");
    if Command::new("git")
        .current_dir(&main_path)
        .args([
            "worktree",
            "add",
            worktree2_path.to_str().unwrap(),
            "-b",
            "feature2",
        ])
        .output()
        .is_err()
    {
        eprintln!("Skipping test: Could not create worktree 2");
        return;
    }

    let worktree1_canonical = worktree1_path.canonicalize();
    let worktree2_canonical = worktree2_path.canonicalize();

    if worktree1_canonical.is_err() || worktree2_canonical.is_err() {
        eprintln!("Skipping test: Could not canonicalize worktree paths");
        return;
    }

    let worktree1_canonical = worktree1_canonical.unwrap();
    let worktree2_canonical = worktree2_canonical.unwrap();

    let config = DevContainerConfig {
        name: Some("test".to_string()),
        image: Some("ubuntu:20.04".to_string()),
        ..Default::default()
    };

    // Create identities for both worktrees
    let identity1 = ContainerIdentity::new(&worktree1_canonical, &config);
    let identity2 = ContainerIdentity::new(&worktree2_canonical, &config);

    // Ensure they have different workspace hashes (isolation)
    assert_ne!(
        identity1.workspace_hash, identity2.workspace_hash,
        "Different worktrees should have different workspace hashes"
    );

    assert_ne!(
        identity1.container_name(),
        identity2.container_name(),
        "Different worktrees should have different container names"
    );
}
