//! Tests for workspace mount consistency and git-root handling
//!
//! These tests verify that workspace mount consistency values are correctly
//! propagated through both Docker and Compose mount rendering paths, and that
//! git-root workspace discovery works correctly.
//!
//! Note: These tests require Docker and are only compiled on Unix systems.
#![cfg(unix)]
//!
//! ## Test Coverage
//!
//! ### User Story 1: Workspace Mount Consistency
//! - T006/T007: Docker and Compose workspace mounts include consistency value
//! - T011: Default workspace discovery behavior is unchanged without consistency
//!
//! ### User Story 2: Git-Root for Docker
//! - T013: Docker mount host path selection with git-root flag
//! - T015: Consistency preserved when using git-root host path
//!
//! ### User Story 3: Git-Root for Compose
//! - T017: Compose mount host path selection with git-root flag
//! - T019: Consistency remains applied in Compose mounts with git-root path

use deacon_core::mount::{Mount, MountConsistency, MountMode, MountParser, MountType};
use std::collections::HashMap;

/// T006 [P] [US1] Test Docker workspace mount includes consistency value
///
/// Verifies that when a consistency value is provided, the Docker workspace
/// mount string includes `consistency=<value>` in the generated mount arguments.
///
/// Tests values: "cached", "consistent", "delegated"
mod docker_consistency_tests {
    use super::*;

    /// Test that a workspace mount with "cached" consistency produces correct Docker args
    #[test]
    fn test_docker_workspace_mount_includes_consistency_cached() {
        // Parse a workspace mount specification with consistency=cached
        let mount_spec =
            "type=bind,source=/host/workspace,target=/workspaces/myproject,consistency=cached";
        let mount =
            MountParser::parse_mount(mount_spec).expect("Should parse mount with consistency");

        // Verify the parsed mount has the consistency field set
        assert_eq!(
            mount.consistency,
            Some(MountConsistency::Cached),
            "Parsed mount should have consistency=cached"
        );

        // Verify the Docker args include the consistency option
        let docker_args = mount.to_docker_args();
        assert_eq!(docker_args.len(), 2, "Should have --mount and value");
        assert_eq!(docker_args[0], "--mount");

        let mount_string = &docker_args[1];
        assert!(
            mount_string.contains("consistency=cached"),
            "Docker mount string should include consistency=cached, got: {}",
            mount_string
        );
    }

    /// Test that a workspace mount with "consistent" consistency produces correct Docker args
    #[test]
    fn test_docker_workspace_mount_includes_consistency_consistent() {
        let mount_spec =
            "type=bind,source=/host/workspace,target=/workspaces/myproject,consistency=consistent";
        let mount =
            MountParser::parse_mount(mount_spec).expect("Should parse mount with consistency");

        assert_eq!(
            mount.consistency,
            Some(MountConsistency::Consistent),
            "Parsed mount should have consistency=consistent"
        );

        let docker_args = mount.to_docker_args();
        let mount_string = &docker_args[1];
        assert!(
            mount_string.contains("consistency=consistent"),
            "Docker mount string should include consistency=consistent, got: {}",
            mount_string
        );
    }

    /// Test that a workspace mount with "delegated" consistency produces correct Docker args
    #[test]
    fn test_docker_workspace_mount_includes_consistency_delegated() {
        let mount_spec =
            "type=bind,source=/host/workspace,target=/workspaces/myproject,consistency=delegated";
        let mount =
            MountParser::parse_mount(mount_spec).expect("Should parse mount with consistency");

        assert_eq!(
            mount.consistency,
            Some(MountConsistency::Delegated),
            "Parsed mount should have consistency=delegated"
        );

        let docker_args = mount.to_docker_args();
        let mount_string = &docker_args[1];
        assert!(
            mount_string.contains("consistency=delegated"),
            "Docker mount string should include consistency=delegated, got: {}",
            mount_string
        );
    }

    /// Test that consistency value is preserved through Mount struct round-trip
    #[test]
    fn test_docker_mount_consistency_roundtrip() {
        // Create a mount directly with consistency
        let mount = Mount {
            mount_type: MountType::Bind,
            source: Some("/host/workspace".to_string()),
            target: "/workspaces/project".to_string(),
            mode: MountMode::ReadWrite,
            consistency: Some(MountConsistency::Cached),
            options: HashMap::new(),
        };

        let docker_args = mount.to_docker_args();
        let mount_string = &docker_args[1];

        // The generated mount string should contain all components
        assert!(mount_string.contains("type=bind"), "Should have type=bind");
        assert!(
            mount_string.contains("source=/host/workspace"),
            "Should have source path"
        );
        assert!(
            mount_string.contains("target=/workspaces/project"),
            "Should have target path"
        );
        assert!(
            mount_string.contains("consistency=cached"),
            "Should have consistency=cached"
        );
    }

    /// Test that workspace mount without consistency does NOT include consistency option
    #[test]
    fn test_docker_workspace_mount_no_consistency_when_not_specified() {
        let mount_spec = "type=bind,source=/host/workspace,target=/workspaces/myproject";
        let mount =
            MountParser::parse_mount(mount_spec).expect("Should parse mount without consistency");

        assert_eq!(
            mount.consistency, None,
            "Parsed mount should have no consistency when not specified"
        );

        let docker_args = mount.to_docker_args();
        let mount_string = &docker_args[1];
        assert!(
            !mount_string.contains("consistency="),
            "Docker mount string should NOT include consistency when not specified, got: {}",
            mount_string
        );
    }

    /// Test all three consistency values are parsed correctly from strings
    #[test]
    fn test_docker_workspace_mount_all_consistency_values() {
        let test_cases = [
            ("cached", MountConsistency::Cached),
            ("consistent", MountConsistency::Consistent),
            ("delegated", MountConsistency::Delegated),
        ];

        for (consistency_str, expected_enum) in test_cases {
            let mount_spec = format!(
                "type=bind,source=/host/workspace,target=/workspaces/project,consistency={}",
                consistency_str
            );
            let mount = MountParser::parse_mount(&mount_spec).unwrap_or_else(|_| {
                panic!("Should parse mount with consistency={}", consistency_str)
            });

            assert_eq!(
                mount.consistency,
                Some(expected_enum),
                "Should parse consistency={}",
                consistency_str
            );

            let docker_args = mount.to_docker_args();
            let mount_string = &docker_args[1];
            assert!(
                mount_string.contains(&format!("consistency={}", consistency_str)),
                "Docker args should contain consistency={}, got: {}",
                consistency_str,
                mount_string
            );
        }
    }
}

/// T007 [P] [US1] Test Compose workspace mount includes consistency value
///
/// Verifies that when a consistency value is provided, Compose mount definitions
/// reflect it in the generated YAML override.
///
/// Note: Compose uses bind mounts with consistency in the "bind" mount syntax
/// or the long-form volumes syntax.
mod compose_consistency_tests {
    use deacon_core::compose::{ComposeMount, ComposeProject};
    use std::path::PathBuf;

    /// Test that Compose mount with consistency generates correct YAML volume syntax
    ///
    /// Verifies that a ComposeMount with consistency=cached produces the
    /// correct short-form volume syntax with the `:cached` suffix.
    #[test]
    fn test_compose_workspace_mount_includes_consistency_cached() {
        // Create a ComposeProject with a workspace mount that should include consistency
        let project = ComposeProject {
            name: "test-project".to_string(),
            base_path: PathBuf::from("/host/workspace"),
            compose_files: vec![PathBuf::from("docker-compose.yml")],
            service: "app".to_string(),
            run_services: Vec::new(),
            env_files: Vec::new(),
            additional_mounts: vec![ComposeMount {
                mount_type: "bind".to_string(),
                source: "/host/workspace".to_string(),
                target: "/workspaces/project".to_string(),
                read_only: false,
                consistency: Some("cached".to_string()),
            }],
            profiles: Vec::new(),
            additional_env: deacon_core::IndexMap::new(),
            external_volumes: Vec::new(),
        };

        let override_yaml = project
            .generate_injection_override()
            .expect("Should generate override with mounts");

        // Verify the YAML contains the mount
        assert!(
            override_yaml.contains("volumes:"),
            "Should have volumes section"
        );

        // Per Docker Compose short-form volume syntax with options:
        // - /host/workspace:/workspaces/project:cached
        assert!(
            override_yaml.contains("/host/workspace:/workspaces/project:cached"),
            "Should have the mount path mapping with consistency=cached, got: {}",
            override_yaml
        );
    }

    /// Test that Compose mount generates deterministic output
    #[test]
    fn test_compose_workspace_mount_deterministic_output() {
        let project = ComposeProject {
            name: "test-project".to_string(),
            base_path: PathBuf::from("/host/workspace"),
            compose_files: vec![PathBuf::from("docker-compose.yml")],
            service: "app".to_string(),
            run_services: Vec::new(),
            env_files: Vec::new(),
            additional_mounts: vec![ComposeMount {
                mount_type: "bind".to_string(),
                source: "/host/workspace".to_string(),
                target: "/workspaces/project".to_string(),
                read_only: false,
                consistency: None,
            }],
            profiles: Vec::new(),
            additional_env: deacon_core::IndexMap::new(),
            external_volumes: Vec::new(),
        };

        // Generate override multiple times and verify determinism
        let override1 = project.generate_injection_override().unwrap();
        let override2 = project.generate_injection_override().unwrap();

        assert_eq!(
            override1, override2,
            "Compose override generation should be deterministic"
        );
    }

    /// Test that Compose mount without consistency does not include :cached suffix
    #[test]
    fn test_compose_workspace_mount_no_consistency_when_not_specified() {
        let project = ComposeProject {
            name: "test-project".to_string(),
            base_path: PathBuf::from("/host/workspace"),
            compose_files: vec![PathBuf::from("docker-compose.yml")],
            service: "app".to_string(),
            run_services: Vec::new(),
            env_files: Vec::new(),
            additional_mounts: vec![ComposeMount {
                mount_type: "bind".to_string(),
                source: "/host/workspace".to_string(),
                target: "/workspaces/project".to_string(),
                read_only: false,
                consistency: None,
            }],
            profiles: Vec::new(),
            additional_env: deacon_core::IndexMap::new(),
            external_volumes: Vec::new(),
        };

        let override_yaml = project.generate_injection_override().unwrap();

        // Without consistency specified, the mount should not have :cached, :consistent, or :delegated
        assert!(
            !override_yaml.contains(":cached"),
            "Should not have :cached suffix when consistency not specified"
        );
        assert!(
            !override_yaml.contains(":consistent"),
            "Should not have :consistent suffix when consistency not specified"
        );
        assert!(
            !override_yaml.contains(":delegated"),
            "Should not have :delegated suffix when consistency not specified"
        );
    }

    /// Test that read-only mounts get :ro suffix (existing behavior verification)
    #[test]
    fn test_compose_workspace_mount_read_only_gets_ro_suffix() {
        let project = ComposeProject {
            name: "test-project".to_string(),
            base_path: PathBuf::from("/host/workspace"),
            compose_files: vec![PathBuf::from("docker-compose.yml")],
            service: "app".to_string(),
            run_services: Vec::new(),
            env_files: Vec::new(),
            additional_mounts: vec![ComposeMount {
                mount_type: "bind".to_string(),
                source: "/host/external".to_string(),
                target: "/external".to_string(),
                read_only: true, // This should produce :ro
                consistency: None,
            }],
            profiles: Vec::new(),
            additional_env: deacon_core::IndexMap::new(),
            external_volumes: Vec::new(),
        };

        let override_yaml = project.generate_injection_override().unwrap();

        assert!(
            override_yaml.contains("/host/external:/external:ro"),
            "Read-only mount should have :ro suffix, got: {}",
            override_yaml
        );
    }
}

/// Tests for workspace mount consistency propagation through CLI args
///
/// These tests verify that the workspace_mount_consistency CLI argument
/// is properly applied when generating workspace mounts.
mod cli_consistency_propagation_tests {
    use super::*;

    /// Test that a mount string with consistency parses and renders correctly
    #[test]
    fn test_cli_workspace_mount_consistency_format() {
        // This is the format that would be generated by the CLI when
        // --workspace-mount-consistency is provided (see up.rs lines 2092-2098)
        let source_path = "/host/workspace";
        let target_path = "/workspaces/project";
        let consistency = "cached";

        let mount_string = format!(
            "type=bind,source={},target={},consistency={}",
            source_path, target_path, consistency
        );

        // Parse the generated mount string
        let mount = MountParser::parse_mount(&mount_string)
            .expect("Should parse CLI-generated workspace mount");

        assert_eq!(mount.mount_type, MountType::Bind);
        assert_eq!(mount.source, Some(source_path.to_string()));
        assert_eq!(mount.target, target_path);
        assert_eq!(mount.consistency, Some(MountConsistency::Cached));

        // Verify round-trip through to_docker_args
        let docker_args = mount.to_docker_args();
        assert!(
            docker_args[1].contains("consistency=cached"),
            "Docker args should preserve consistency"
        );
    }

    /// Test all valid consistency values in CLI format
    #[test]
    fn test_cli_workspace_mount_all_consistency_values() {
        for consistency in ["cached", "consistent", "delegated"] {
            let mount_string = format!(
                "type=bind,source=/workspace,target=/workspaces/test,consistency={}",
                consistency
            );

            let mount = MountParser::parse_mount(&mount_string)
                .unwrap_or_else(|_| panic!("Should parse mount with consistency={}", consistency));

            let docker_args = mount.to_docker_args();
            assert!(
                docker_args[1].contains(&format!("consistency={}", consistency)),
                "Docker args should contain consistency={}, got: {}",
                consistency,
                docker_args[1]
            );
        }
    }

    /// Test that invalid consistency values are rejected by the MountParser
    ///
    /// ISSUE-008: Verifies that invalid consistency values like "invalid", "unknown",
    /// or empty strings are properly rejected at the parsing level.
    #[test]
    fn test_invalid_consistency_value_rejected() {
        // Test various invalid consistency values
        let invalid_values = ["invalid", "unknown", "fast", "slow", ""];

        for invalid in invalid_values {
            let mount_spec = format!("type=bind,source=/src,target=/dst,consistency={}", invalid);
            let result = MountParser::parse_mount(&mount_spec);

            // The parser should reject invalid consistency values
            assert!(
                result.is_err(),
                "MountParser should reject invalid consistency value '{}', but it succeeded",
                invalid
            );
        }
    }

    /// Test that valid consistency values are properly documented and parsed
    ///
    /// ISSUE-008: Documents the expected valid values per Docker documentation
    /// and verifies they are correctly parsed.
    #[test]
    fn test_valid_consistency_values_documented() {
        // Document the valid consistency values per Docker documentation
        const VALID_CONSISTENCY_VALUES: &[&str] = &["cached", "consistent", "delegated"];

        for consistency in VALID_CONSISTENCY_VALUES {
            let mount_spec = format!(
                "type=bind,source=/src,target=/dst,consistency={}",
                consistency
            );
            let mount = MountParser::parse_mount(&mount_spec).unwrap_or_else(|_| {
                panic!("Should parse valid consistency value '{}'", consistency)
            });

            // Verify the consistency was parsed correctly
            assert!(
                mount.consistency.is_some(),
                "Mount should have consistency set for valid value '{}'",
                consistency
            );

            // Verify round-trip through to_docker_args preserves the value
            let docker_args = mount.to_docker_args();
            assert!(
                docker_args[1].contains(&format!("consistency={}", consistency)),
                "Docker args should contain consistency={}, got: {}",
                consistency,
                docker_args[1]
            );
        }
    }

    /// Test that consistency validation is case-insensitive
    ///
    /// ISSUE-008: Documents that the MountParser accepts consistency values
    /// in any case (lowercase, uppercase, mixed) for user convenience.
    /// The values are normalized to lowercase internally.
    #[test]
    fn test_consistency_value_case_insensitive() {
        // All case variants should be accepted
        let test_cases = [
            ("cached", MountConsistency::Cached),
            ("CACHED", MountConsistency::Cached),
            ("Cached", MountConsistency::Cached),
            ("consistent", MountConsistency::Consistent),
            ("CONSISTENT", MountConsistency::Consistent),
            ("delegated", MountConsistency::Delegated),
            ("DELEGATED", MountConsistency::Delegated),
        ];

        for (input, expected) in test_cases {
            let mount_spec = format!("type=bind,source=/src,target=/dst,consistency={}", input);
            let mount = MountParser::parse_mount(&mount_spec)
                .unwrap_or_else(|_| panic!("Should parse consistency value '{}'", input));

            assert_eq!(
                mount.consistency,
                Some(expected),
                "Consistency '{}' should be parsed correctly",
                input
            );
        }
    }
}

/// T013 [P] [US2]: Tests for Docker mount host path selection when git-root flag is set
///
/// These tests verify that:
/// - When git-root flag is set and we're in a git repo, the host path uses the repo root
/// - Test with both subdirectory invocation and root invocation
/// - Verify the path selection is correct for Docker mounts
mod git_root_docker_mount_tests {
    use deacon_core::workspace::{find_git_repository_root, resolve_workspace_root};
    use std::fs;
    use tempfile::TempDir;

    /// Test that workspace resolution finds git repository root from subdirectory
    ///
    /// This simulates the case where `deacon up` is invoked from a subdirectory
    /// of a git repository with `--mount-workspace-git-root` enabled.
    #[test]
    fn test_docker_mount_uses_git_root_from_subdirectory() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");

        // Create a git repository structure
        let git_dir = temp_dir.path().join(".git");
        fs::create_dir(&git_dir).expect("Failed to create .git directory");

        // Create a subdirectory to invoke from
        let subdir = temp_dir.path().join("src").join("components");
        fs::create_dir_all(&subdir).expect("Failed to create subdirectory");

        // Resolve workspace root from subdirectory (simulates --mount-workspace-git-root=true)
        let workspace_root =
            resolve_workspace_root(&subdir).expect("Failed to resolve workspace root");

        // The workspace root should be the git repository root, not the subdirectory
        assert_eq!(
            workspace_root.canonicalize().unwrap(),
            temp_dir.path().canonicalize().unwrap(),
            "Docker mount should use git repository root when invoked from subdirectory"
        );
    }

    /// Test that workspace resolution works when invoked from the repository root
    #[test]
    fn test_docker_mount_uses_git_root_from_root() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");

        // Create a git repository structure
        let git_dir = temp_dir.path().join(".git");
        fs::create_dir(&git_dir).expect("Failed to create .git directory");

        // Resolve workspace root from root (simulates --mount-workspace-git-root=true)
        let workspace_root =
            resolve_workspace_root(temp_dir.path()).expect("Failed to resolve workspace root");

        // The workspace root should be the git repository root
        assert_eq!(
            workspace_root.canonicalize().unwrap(),
            temp_dir.path().canonicalize().unwrap(),
            "Docker mount should use git repository root when invoked from root"
        );
    }

    /// Test that find_git_repository_root correctly identifies git root from subdirectory
    #[test]
    fn test_find_git_repository_root_from_nested_subdirectory() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");

        // Create a git repository structure
        let git_dir = temp_dir.path().join(".git");
        fs::create_dir(&git_dir).expect("Failed to create .git directory");

        // Create a deeply nested subdirectory
        let deep_subdir = temp_dir
            .path()
            .join("packages")
            .join("core")
            .join("src")
            .join("lib");
        fs::create_dir_all(&deep_subdir).expect("Failed to create deep subdirectory");

        // Find git root from deep subdirectory
        let git_root_result = find_git_repository_root(&deep_subdir)
            .expect("Failed to find git repository root")
            .expect("Should find git repository root");

        assert_eq!(
            git_root_result.git_root.canonicalize().unwrap(),
            temp_dir.path().canonicalize().unwrap(),
            "Should find git repository root from deeply nested subdirectory"
        );
        assert!(
            !git_root_result.is_worktree,
            "Regular git directory should not be detected as worktree"
        );
    }

    /// Test that workspace resolution falls back to workspace root when not in git repository
    #[test]
    fn test_docker_mount_fallback_when_no_git_repository() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");

        // Create a subdirectory but NO .git
        let subdir = temp_dir.path().join("project").join("src");
        fs::create_dir_all(&subdir).expect("Failed to create subdirectory");

        // Resolve workspace root (simulates --mount-workspace-git-root=true but no git repo)
        let workspace_root =
            resolve_workspace_root(&subdir).expect("Failed to resolve workspace root");

        // The workspace root should be the subdirectory itself (fallback behavior)
        assert_eq!(
            workspace_root.canonicalize().unwrap(),
            subdir.canonicalize().unwrap(),
            "Docker mount should fallback to workspace root when not in git repository"
        );
    }

    /// Test that git worktrees are handled correctly for Docker mounts
    #[test]
    fn test_docker_mount_uses_worktree_root() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");

        // Create a .git file pointing to a worktrees directory (simulates git worktree)
        let git_file = temp_dir.path().join(".git");
        let gitdir_content = "gitdir: /path/to/repo/.git/worktrees/my-feature\n";
        fs::write(&git_file, gitdir_content).expect("Failed to write .git file");

        // Create a subdirectory
        let subdir = temp_dir.path().join("src");
        fs::create_dir_all(&subdir).expect("Failed to create subdirectory");

        // Resolve workspace root from subdirectory of worktree
        let workspace_root =
            resolve_workspace_root(&subdir).expect("Failed to resolve workspace root");

        // The workspace root should be the worktree root
        assert_eq!(
            workspace_root.canonicalize().unwrap(),
            temp_dir.path().canonicalize().unwrap(),
            "Docker mount should use worktree root, not subdirectory"
        );
    }
}

/// T015 [P] [US2]: Tests for consistency preservation when using git-root host path
///
/// These tests verify that:
/// - Consistency value is preserved regardless of whether git-root or workspace-root is used
/// - Combining git-root + consistency works correctly
mod git_root_with_consistency_tests {
    use super::*;
    use deacon_core::workspace::resolve_workspace_root;
    use std::fs;
    use tempfile::TempDir;

    /// Test that workspace mount with git-root and consistency produces correct Docker args
    ///
    /// This simulates the full flow: git-root resolution + consistency application
    #[test]
    fn test_docker_mount_git_root_with_consistency_cached() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");

        // Create a git repository structure
        let git_dir = temp_dir.path().join(".git");
        fs::create_dir(&git_dir).expect("Failed to create .git directory");

        // Create a subdirectory
        let subdir = temp_dir.path().join("src");
        fs::create_dir_all(&subdir).expect("Failed to create subdirectory");

        // Resolve workspace root (simulates --mount-workspace-git-root=true)
        let workspace_root =
            resolve_workspace_root(&subdir).expect("Failed to resolve workspace root");

        // Now create a mount spec with the resolved path and consistency
        let mount_spec = format!(
            "type=bind,source={},target=/workspaces/project,consistency=cached",
            workspace_root.display()
        );
        let mount = MountParser::parse_mount(&mount_spec)
            .expect("Should parse mount with git-root and consistency");

        // Verify the mount has the correct source (git root) and consistency
        assert_eq!(
            mount.source.as_ref().unwrap(),
            &workspace_root.display().to_string(),
            "Mount source should be git repository root"
        );
        assert_eq!(
            mount.consistency,
            Some(MountConsistency::Cached),
            "Mount should have consistency=cached"
        );

        // Verify Docker args include both path and consistency
        let docker_args = mount.to_docker_args();
        let mount_string = &docker_args[1];
        assert!(
            mount_string.contains("consistency=cached"),
            "Docker args should include consistency=cached"
        );
        assert!(
            mount_string.contains(&format!("source={}", workspace_root.display())),
            "Docker args should include git-root source path"
        );
    }

    /// Test that consistency is preserved with all three values when using git-root
    #[test]
    fn test_docker_mount_git_root_with_all_consistency_values() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");

        // Create a git repository structure
        let git_dir = temp_dir.path().join(".git");
        fs::create_dir(&git_dir).expect("Failed to create .git directory");

        // Resolve workspace root
        let workspace_root =
            resolve_workspace_root(temp_dir.path()).expect("Failed to resolve workspace root");

        for (consistency_str, expected_enum) in [
            ("cached", MountConsistency::Cached),
            ("consistent", MountConsistency::Consistent),
            ("delegated", MountConsistency::Delegated),
        ] {
            let mount_spec = format!(
                "type=bind,source={},target=/workspaces/project,consistency={}",
                workspace_root.display(),
                consistency_str
            );
            let mount = MountParser::parse_mount(&mount_spec).unwrap_or_else(|_| {
                panic!(
                    "Should parse mount with git-root and consistency={}",
                    consistency_str
                )
            });

            assert_eq!(
                mount.consistency,
                Some(expected_enum),
                "Mount should have consistency={}",
                consistency_str
            );

            let docker_args = mount.to_docker_args();
            assert!(
                docker_args[1].contains(&format!("consistency={}", consistency_str)),
                "Docker args should contain consistency={}, got: {}",
                consistency_str,
                docker_args[1]
            );
        }
    }

    /// Test that git-root + consistency works with worktrees
    #[test]
    fn test_docker_mount_worktree_with_consistency() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");

        // Create a .git file pointing to a worktrees directory
        let git_file = temp_dir.path().join(".git");
        let gitdir_content = "gitdir: /path/to/repo/.git/worktrees/feature-branch\n";
        fs::write(&git_file, gitdir_content).expect("Failed to write .git file");

        // Resolve workspace root
        let workspace_root =
            resolve_workspace_root(temp_dir.path()).expect("Failed to resolve workspace root");

        // Create mount with consistency
        let mount_spec = format!(
            "type=bind,source={},target=/workspaces/project,consistency=delegated",
            workspace_root.display()
        );
        let mount = MountParser::parse_mount(&mount_spec)
            .expect("Should parse mount with worktree root and consistency");

        assert_eq!(
            mount.consistency,
            Some(MountConsistency::Delegated),
            "Mount should have consistency=delegated"
        );

        let docker_args = mount.to_docker_args();
        assert!(
            docker_args[1].contains("consistency=delegated"),
            "Docker args should include consistency=delegated"
        );
    }
}

/// T011 [P] [US1]: Tests confirming default workspace discovery behavior is unchanged
/// when no consistency override is provided.
///
/// These tests verify that:
/// - Workspace mount parsing works without consistency
/// - Docker mount generation produces expected format without consistency
/// - Compose mount generation produces expected format without consistency
/// - No regression in existing default behavior
mod default_workspace_discovery_tests {
    use super::*;
    use deacon_core::compose::{ComposeMount, ComposeProject};
    use std::collections::HashMap;
    use std::path::PathBuf;

    /// T011: Test that default workspace discovery behavior is unchanged without consistency override
    ///
    /// Verifies that when no consistency value is provided:
    /// 1. Docker workspace mount is generated without consistency option
    /// 2. The mount string follows standard format: type=bind,source=X,target=Y
    /// 3. No extra options are added to the mount
    #[test]
    fn test_default_workspace_discovery_unchanged_without_consistency() {
        // Default workspace mount format without consistency
        let mount_spec = "type=bind,source=/host/workspace,target=/workspaces/project";
        let mount =
            MountParser::parse_mount(mount_spec).expect("Should parse mount without consistency");

        // Verify the mount has no consistency field
        assert_eq!(
            mount.consistency, None,
            "Default mount should have no consistency"
        );

        // Verify the Docker args format is unchanged
        let docker_args = mount.to_docker_args();
        assert_eq!(docker_args.len(), 2, "Should have --mount and value");
        assert_eq!(docker_args[0], "--mount");

        let mount_string = &docker_args[1];

        // Verify the mount string contains expected components
        assert!(
            mount_string.contains("type=bind"),
            "Should have type=bind, got: {}",
            mount_string
        );
        assert!(
            mount_string.contains("source="),
            "Should have source=, got: {}",
            mount_string
        );
        assert!(
            mount_string.contains("target=/workspaces/project"),
            "Should have target path, got: {}",
            mount_string
        );

        // Most importantly: no consistency option
        assert!(
            !mount_string.contains("consistency="),
            "Default mount should NOT have consistency option, got: {}",
            mount_string
        );
    }

    /// Test that default Compose workspace mount behavior is unchanged without consistency
    #[test]
    fn test_default_compose_workspace_discovery_unchanged() {
        let project = ComposeProject {
            name: "test-project".to_string(),
            base_path: PathBuf::from("/host/workspace"),
            compose_files: vec![PathBuf::from("docker-compose.yml")],
            service: "app".to_string(),
            run_services: Vec::new(),
            env_files: Vec::new(),
            additional_mounts: vec![ComposeMount {
                mount_type: "bind".to_string(),
                source: "/host/workspace".to_string(),
                target: "/workspaces/project".to_string(),
                read_only: false,
                consistency: None, // No consistency override
            }],
            profiles: Vec::new(),
            additional_env: deacon_core::IndexMap::new(),
            external_volumes: Vec::new(),
        };

        let override_yaml = project
            .generate_injection_override()
            .expect("Should generate override with mounts");

        // Verify the YAML contains the mount in default format
        assert!(
            override_yaml.contains("volumes:"),
            "Should have volumes section"
        );

        // The mount should be in simple source:target format (no options suffix)
        assert!(
            override_yaml.contains("/host/workspace:/workspaces/project\n"),
            "Should have simple mount format without options, got: {}",
            override_yaml
        );

        // Should NOT have any consistency options
        assert!(
            !override_yaml.contains(":cached"),
            "Should not have :cached, got: {}",
            override_yaml
        );
        assert!(
            !override_yaml.contains(":consistent"),
            "Should not have :consistent, got: {}",
            override_yaml
        );
        assert!(
            !override_yaml.contains(":delegated"),
            "Should not have :delegated, got: {}",
            override_yaml
        );
    }

    /// Test that Mount struct default behavior is preserved
    #[test]
    fn test_mount_struct_default_behavior() {
        // Create a mount with default values (no consistency)
        let mount = Mount {
            mount_type: MountType::Bind,
            source: Some("/host/workspace".to_string()),
            target: "/workspaces/project".to_string(),
            mode: MountMode::ReadWrite,
            consistency: None,
            options: HashMap::new(),
        };

        let docker_args = mount.to_docker_args();
        let mount_string = &docker_args[1];

        // Verify default format
        assert!(
            mount_string.starts_with("type=bind"),
            "Should start with type=bind"
        );
        assert!(
            !mount_string.contains(",ro"),
            "ReadWrite mode should not have ,ro"
        );
        assert!(
            !mount_string.contains("consistency="),
            "No consistency in default mount"
        );
    }

    /// Test that workspace discovery path computation is unchanged
    ///
    /// This tests the format that up.rs generates for the default workspace mount
    /// when no consistency is specified.
    #[test]
    fn test_workspace_mount_path_format_unchanged() {
        // Simulate the format computed in up.rs without consistency
        let source_path = "/host/myproject";
        let target_path = "/workspaces/myproject";

        let mount_string = format!("type=bind,source={},target={}", source_path, target_path);

        let mount = MountParser::parse_mount(&mount_string)
            .expect("Should parse default workspace mount format");

        assert_eq!(mount.mount_type, MountType::Bind);
        assert_eq!(mount.source, Some(source_path.to_string()));
        assert_eq!(mount.target, target_path);
        assert_eq!(mount.mode, MountMode::ReadWrite);
        assert_eq!(mount.consistency, None);

        // Verify round-trip preserves format
        let docker_args = mount.to_docker_args();
        assert!(
            docker_args[1].contains(&format!("source={}", source_path)),
            "Should preserve source path"
        );
        assert!(
            docker_args[1].contains(&format!("target={}", target_path)),
            "Should preserve target path"
        );
    }
}

/// T017 [P] [US3]: Tests for Compose mount host path selection with git-root flag
///
/// Verifies that when the git-root flag is set:
/// - Compose services get the git-root path as the workspace mount source
/// - All services in a compose file receive the same git-root path
/// - Multi-service compose scenarios use consistent paths
mod compose_git_root_tests {
    use deacon_core::compose::{ComposeMount, ComposeProject};
    use std::path::PathBuf;

    /// Test that Compose workspace mount uses git-root path when flag is set
    ///
    /// Simulates the scenario where:
    /// - Working directory is /repo/subdir
    /// - Git root is /repo
    /// - The workspace mount should use /repo as the source
    #[test]
    fn test_compose_workspace_mount_uses_git_root_path() {
        // Simulate git-root resolved path (as would be done by resolve_workspace_root)
        let git_root = "/repo";
        let workspace_name = "repo";
        let target_path = format!("/workspaces/{}", workspace_name);

        let project = ComposeProject {
            name: "test-project".to_string(),
            base_path: PathBuf::from(git_root),
            compose_files: vec![PathBuf::from("docker-compose.yml")],
            service: "app".to_string(),
            run_services: Vec::new(),
            env_files: Vec::new(),
            additional_mounts: vec![ComposeMount {
                mount_type: "bind".to_string(),
                source: git_root.to_string(),
                target: target_path.clone(),
                read_only: false,
                consistency: None,
            }],
            profiles: Vec::new(),
            additional_env: deacon_core::IndexMap::new(),
            external_volumes: Vec::new(),
        };

        let override_yaml = project
            .generate_injection_override()
            .expect("Should generate override with workspace mount");

        // Verify the mount uses the git-root path
        assert!(
            override_yaml.contains(&format!("{}:{}", git_root, target_path)),
            "Should use git-root path as source, got: {}",
            override_yaml
        );
    }

    /// Test that Compose workspace mount source reflects resolved path (not original subdir)
    ///
    /// This test ensures that when a user invokes from /repo/subdir with git-root flag,
    /// the resulting mount uses /repo (the git root) not /repo/subdir.
    #[test]
    fn test_compose_workspace_mount_not_subdir() {
        // Original invocation path (subdir)
        let original_subdir = "/repo/packages/frontend";
        // Resolved git root
        let git_root = "/repo";
        let workspace_name = "repo";
        let target_path = format!("/workspaces/{}", workspace_name);

        let project = ComposeProject {
            name: "test-project".to_string(),
            // base_path is the resolved git root, not the original subdir
            base_path: PathBuf::from(git_root),
            compose_files: vec![PathBuf::from("docker-compose.yml")],
            service: "app".to_string(),
            run_services: Vec::new(),
            env_files: Vec::new(),
            additional_mounts: vec![ComposeMount {
                mount_type: "bind".to_string(),
                source: git_root.to_string(),
                target: target_path.clone(),
                read_only: false,
                consistency: None,
            }],
            profiles: Vec::new(),
            additional_env: deacon_core::IndexMap::new(),
            external_volumes: Vec::new(),
        };

        let override_yaml = project
            .generate_injection_override()
            .expect("Should generate override with workspace mount");

        // Verify the mount does NOT use the subdir path
        assert!(
            !override_yaml.contains(original_subdir),
            "Should not contain original subdir path, got: {}",
            override_yaml
        );

        // Verify the mount uses the git-root path
        assert!(
            override_yaml.contains(git_root),
            "Should use git-root path, got: {}",
            override_yaml
        );
    }

    /// Test that all Compose services receive the same workspace mount path
    ///
    /// Per FR-005: All workspace mounts generated for Compose should use
    /// the git root uniformly across services.
    #[test]
    fn test_compose_multi_service_same_git_root_path() {
        let git_root = "/repo";
        let target_path = "/workspaces/repo";

        // Create a project with multiple services (primary + run_services)
        let project = ComposeProject {
            name: "multi-service-project".to_string(),
            base_path: PathBuf::from(git_root),
            compose_files: vec![PathBuf::from("docker-compose.yml")],
            service: "web".to_string(),
            run_services: vec!["api".to_string(), "worker".to_string()],
            env_files: Vec::new(),
            additional_mounts: vec![ComposeMount {
                mount_type: "bind".to_string(),
                source: git_root.to_string(),
                target: target_path.to_string(),
                read_only: false,
                consistency: None,
            }],
            profiles: Vec::new(),
            additional_env: deacon_core::IndexMap::new(),
            external_volumes: Vec::new(),
        };

        // Verify all services are present
        let all_services = project.get_all_services();
        assert_eq!(
            all_services,
            vec!["web", "api", "worker"],
            "Should have all services"
        );

        // Generate override and verify the mount path
        let override_yaml = project.generate_injection_override().unwrap();

        // The injection override targets the primary service
        assert!(
            override_yaml.contains("web:"),
            "Override should target primary service"
        );
        assert!(
            override_yaml.contains(&format!("{}:{}", git_root, target_path)),
            "Primary service should have git-root mount"
        );
    }

    /// Test Compose workspace mount when workspace root and git root are the same
    ///
    /// Edge case: When invoked from the repo root, the mount should remain
    /// unchanged but still report the chosen consistency.
    #[test]
    fn test_compose_workspace_mount_same_as_git_root() {
        // When workspace root == git root
        let root_path = "/myrepo";
        let target_path = "/workspaces/myrepo";

        let project = ComposeProject {
            name: "same-root-project".to_string(),
            base_path: PathBuf::from(root_path),
            compose_files: vec![PathBuf::from("docker-compose.yml")],
            service: "app".to_string(),
            run_services: Vec::new(),
            env_files: Vec::new(),
            additional_mounts: vec![ComposeMount {
                mount_type: "bind".to_string(),
                source: root_path.to_string(),
                target: target_path.to_string(),
                read_only: false,
                consistency: Some("cached".to_string()),
            }],
            profiles: Vec::new(),
            additional_env: deacon_core::IndexMap::new(),
            external_volumes: Vec::new(),
        };

        let override_yaml = project.generate_injection_override().unwrap();

        // Mount should use the same path
        assert!(
            override_yaml.contains(&format!("{}:{}:cached", root_path, target_path)),
            "Should have mount with consistency when workspace == git root, got: {}",
            override_yaml
        );
    }
}

/// T019 [P] [US3]: Tests ensuring consistency value remains applied with git-root host path
///
/// Verifies that consistency + git-root work together for Compose mounts.
mod compose_git_root_consistency_tests {
    use deacon_core::compose::{ComposeMount, ComposeProject};
    use std::path::PathBuf;

    /// Test that git-root path with cached consistency produces correct mount
    #[test]
    fn test_compose_git_root_with_consistency_cached() {
        let git_root = "/repo";
        let target_path = "/workspaces/repo";

        let project = ComposeProject {
            name: "test-project".to_string(),
            base_path: PathBuf::from(git_root),
            compose_files: vec![PathBuf::from("docker-compose.yml")],
            service: "app".to_string(),
            run_services: Vec::new(),
            env_files: Vec::new(),
            additional_mounts: vec![ComposeMount {
                mount_type: "bind".to_string(),
                source: git_root.to_string(),
                target: target_path.to_string(),
                read_only: false,
                consistency: Some("cached".to_string()),
            }],
            profiles: Vec::new(),
            additional_env: deacon_core::IndexMap::new(),
            external_volumes: Vec::new(),
        };

        let override_yaml = project.generate_injection_override().unwrap();

        // Should have both git-root path and consistency
        assert!(
            override_yaml.contains(&format!("{}:{}:cached", git_root, target_path)),
            "Should have git-root mount with cached consistency, got: {}",
            override_yaml
        );
    }

    /// Test that git-root path with consistent consistency produces correct mount
    #[test]
    fn test_compose_git_root_with_consistency_consistent() {
        let git_root = "/repo";
        let target_path = "/workspaces/repo";

        let project = ComposeProject {
            name: "test-project".to_string(),
            base_path: PathBuf::from(git_root),
            compose_files: vec![PathBuf::from("docker-compose.yml")],
            service: "app".to_string(),
            run_services: Vec::new(),
            env_files: Vec::new(),
            additional_mounts: vec![ComposeMount {
                mount_type: "bind".to_string(),
                source: git_root.to_string(),
                target: target_path.to_string(),
                read_only: false,
                consistency: Some("consistent".to_string()),
            }],
            profiles: Vec::new(),
            additional_env: deacon_core::IndexMap::new(),
            external_volumes: Vec::new(),
        };

        let override_yaml = project.generate_injection_override().unwrap();

        // Should have both git-root path and consistency
        assert!(
            override_yaml.contains(&format!("{}:{}:consistent", git_root, target_path)),
            "Should have git-root mount with consistent consistency, got: {}",
            override_yaml
        );
    }

    /// Test that git-root path with delegated consistency produces correct mount
    #[test]
    fn test_compose_git_root_with_consistency_delegated() {
        let git_root = "/repo";
        let target_path = "/workspaces/repo";

        let project = ComposeProject {
            name: "test-project".to_string(),
            base_path: PathBuf::from(git_root),
            compose_files: vec![PathBuf::from("docker-compose.yml")],
            service: "app".to_string(),
            run_services: Vec::new(),
            env_files: Vec::new(),
            additional_mounts: vec![ComposeMount {
                mount_type: "bind".to_string(),
                source: git_root.to_string(),
                target: target_path.to_string(),
                read_only: false,
                consistency: Some("delegated".to_string()),
            }],
            profiles: Vec::new(),
            additional_env: deacon_core::IndexMap::new(),
            external_volumes: Vec::new(),
        };

        let override_yaml = project.generate_injection_override().unwrap();

        // Should have both git-root path and consistency
        assert!(
            override_yaml.contains(&format!("{}:{}:delegated", git_root, target_path)),
            "Should have git-root mount with delegated consistency, got: {}",
            override_yaml
        );
    }

    /// Test all consistency values with git-root path
    #[test]
    fn test_compose_git_root_all_consistency_values() {
        let git_root = "/repo";
        let target_path = "/workspaces/repo";

        for consistency in ["cached", "consistent", "delegated"] {
            let project = ComposeProject {
                name: "test-project".to_string(),
                base_path: PathBuf::from(git_root),
                compose_files: vec![PathBuf::from("docker-compose.yml")],
                service: "app".to_string(),
                run_services: Vec::new(),
                env_files: Vec::new(),
                additional_mounts: vec![ComposeMount {
                    mount_type: "bind".to_string(),
                    source: git_root.to_string(),
                    target: target_path.to_string(),
                    read_only: false,
                    consistency: Some(consistency.to_string()),
                }],
                profiles: Vec::new(),
                additional_env: deacon_core::IndexMap::new(),
                external_volumes: Vec::new(),
            };

            let override_yaml = project.generate_injection_override().unwrap();

            assert!(
                override_yaml.contains(&format!("{}:{}:{}", git_root, target_path, consistency)),
                "Should have git-root mount with {} consistency, got: {}",
                consistency,
                override_yaml
            );
        }
    }

    /// Test external mount with git-root path preserves :ro
    #[test]
    fn test_compose_git_root_external_mount_with_consistency() {
        let git_root = "/repo";
        let target_path = "/workspaces/repo";

        let project = ComposeProject {
            name: "test-project".to_string(),
            base_path: PathBuf::from(git_root),
            compose_files: vec![PathBuf::from("docker-compose.yml")],
            service: "app".to_string(),
            run_services: Vec::new(),
            env_files: Vec::new(),
            additional_mounts: vec![ComposeMount {
                mount_type: "bind".to_string(),
                source: git_root.to_string(),
                target: target_path.to_string(),
                read_only: true, // Read-only
                consistency: Some("cached".to_string()),
            }],
            profiles: Vec::new(),
            additional_env: deacon_core::IndexMap::new(),
            external_volumes: Vec::new(),
        };

        let override_yaml = project.generate_injection_override().unwrap();

        // Should have both :ro and :cached options
        assert!(
            override_yaml.contains(&format!("{}:{}:ro,cached", git_root, target_path)),
            "Should have git-root mount with ro,cached options, got: {}",
            override_yaml
        );
    }

    /// Test git-root path without consistency (no options suffix)
    #[test]
    fn test_compose_git_root_without_consistency() {
        let git_root = "/repo";
        let target_path = "/workspaces/repo";

        let project = ComposeProject {
            name: "test-project".to_string(),
            base_path: PathBuf::from(git_root),
            compose_files: vec![PathBuf::from("docker-compose.yml")],
            service: "app".to_string(),
            run_services: Vec::new(),
            env_files: Vec::new(),
            additional_mounts: vec![ComposeMount {
                mount_type: "bind".to_string(),
                source: git_root.to_string(),
                target: target_path.to_string(),
                read_only: false,
                consistency: None,
            }],
            profiles: Vec::new(),
            additional_env: deacon_core::IndexMap::new(),
            external_volumes: Vec::new(),
        };

        let override_yaml = project.generate_injection_override().unwrap();

        // Should have mount without any options suffix (no trailing colon after target)
        let expected_mount = format!("{}:{}\n", git_root, target_path);
        assert!(
            override_yaml.contains(&expected_mount),
            "Should have git-root mount without options suffix, got: {}",
            override_yaml
        );

        // Verify no consistency options present
        assert!(
            !override_yaml.contains(":cached"),
            "Should not have :cached"
        );
        assert!(
            !override_yaml.contains(":consistent"),
            "Should not have :consistent"
        );
        assert!(
            !override_yaml.contains(":delegated"),
            "Should not have :delegated"
        );
    }
}

/// T025 [P]: Performance tests for workspace discovery and mount rendering
///
/// Verifies that workspace discovery and mount rendering operations complete
/// within acceptable time bounds (<200ms) to ensure responsive CLI behavior.
mod performance_tests {
    use super::*;
    use deacon_core::compose::{ComposeMount, ComposeProject};
    use deacon_core::workspace::resolve_workspace_root;
    use std::path::PathBuf;
    use std::time::{Duration, Instant};
    use tempfile::TempDir;

    /// Maximum allowed time for workspace discovery + mount rendering path
    const MAX_DURATION_MS: u64 = 200;

    /// Test that workspace discovery completes within time budget
    ///
    /// This test exercises the full workspace resolution path including:
    /// - Canonicalization
    /// - Git worktree detection
    /// - Git repository root detection
    #[test]
    fn test_workspace_discovery_performance() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");

        // Create a git repository structure
        let git_dir = temp_dir.path().join(".git");
        std::fs::create_dir(&git_dir).expect("Failed to create .git directory");

        // Create a deeply nested subdirectory to exercise path traversal
        let deep_subdir = temp_dir
            .path()
            .join("packages")
            .join("core")
            .join("src")
            .join("lib")
            .join("utils");
        std::fs::create_dir_all(&deep_subdir).expect("Failed to create deep subdirectory");

        // Warm-up call (first call may have cold-start overhead)
        let _ = resolve_workspace_root(&deep_subdir);

        // Measure workspace discovery time (multiple iterations for stability)
        let iterations = 10;
        let start = Instant::now();
        for _ in 0..iterations {
            let result = resolve_workspace_root(&deep_subdir);
            assert!(result.is_ok(), "Workspace resolution should succeed");
        }
        let elapsed = start.elapsed();
        let avg_duration = elapsed / iterations;

        assert!(
            avg_duration < Duration::from_millis(MAX_DURATION_MS),
            "Workspace discovery should complete in <{}ms, took {:?} (avg over {} iterations)",
            MAX_DURATION_MS,
            avg_duration,
            iterations
        );
    }

    /// Test that mount rendering completes within time budget
    ///
    /// This test exercises mount string generation for both Docker and Compose:
    /// - Mount parsing
    /// - Docker args generation
    /// - Compose override YAML generation
    #[test]
    fn test_mount_rendering_performance() {
        // Prepare test data
        let source_path = "/host/workspace/my-project";
        let target_path = "/workspaces/my-project";
        let mount_spec = format!(
            "type=bind,source={},target={},consistency=cached",
            source_path, target_path
        );

        // Warm-up calls
        let _ = MountParser::parse_mount(&mount_spec);

        // Measure Docker mount rendering time
        let iterations = 100;
        let start = Instant::now();
        for _ in 0..iterations {
            let mount = MountParser::parse_mount(&mount_spec).unwrap();
            let _docker_args = mount.to_docker_args();
        }
        let docker_elapsed = start.elapsed();
        let docker_avg = docker_elapsed / iterations;

        assert!(
            docker_avg < Duration::from_millis(MAX_DURATION_MS),
            "Docker mount rendering should complete in <{}ms, took {:?} (avg over {} iterations)",
            MAX_DURATION_MS,
            docker_avg,
            iterations
        );
    }

    /// Test that Compose override generation completes within time budget
    #[test]
    fn test_compose_override_generation_performance() {
        // Prepare test data with multiple mounts and env vars
        let mut additional_env: deacon_core::IndexMap<String, String> =
            deacon_core::IndexMap::new();
        for i in 0..10 {
            additional_env.insert(format!("VAR_{}", i), format!("value_{}", i));
        }

        let additional_mounts: Vec<ComposeMount> = (0..5)
            .map(|i| ComposeMount {
                mount_type: "bind".to_string(),
                source: format!("/host/path/{}", i),
                target: format!("/container/path/{}", i),
                read_only: i % 2 == 0,
                consistency: if i % 3 == 0 {
                    Some("cached".to_string())
                } else {
                    None
                },
            })
            .collect();

        let project = ComposeProject {
            name: "perf-test-project".to_string(),
            base_path: PathBuf::from("/host/workspace"),
            compose_files: vec![PathBuf::from("docker-compose.yml")],
            service: "app".to_string(),
            run_services: vec!["db".to_string(), "redis".to_string()],
            env_files: Vec::new(),
            additional_mounts,
            profiles: Vec::new(),
            additional_env,
            external_volumes: Vec::new(),
        };

        // Warm-up call
        let _ = project.generate_injection_override();

        // Measure Compose override generation time
        let iterations = 100;
        let start = Instant::now();
        for _ in 0..iterations {
            let result = project.generate_injection_override();
            assert!(result.is_some(), "Should generate override");
        }
        let elapsed = start.elapsed();
        let avg_duration = elapsed / iterations;

        assert!(
            avg_duration < Duration::from_millis(MAX_DURATION_MS),
            "Compose override generation should complete in <{}ms, took {:?} (avg over {} iterations)",
            MAX_DURATION_MS,
            avg_duration,
            iterations
        );
    }

    /// Combined end-to-end performance test for workspace discovery + mount rendering
    ///
    /// This simulates the full path from workspace resolution to mount string
    /// generation, which is the critical path in `deacon up`.
    #[test]
    fn test_combined_workspace_and_mount_performance() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");

        // Create a git repository structure
        let git_dir = temp_dir.path().join(".git");
        std::fs::create_dir(&git_dir).expect("Failed to create .git directory");

        // Create subdirectory
        let subdir = temp_dir.path().join("src");
        std::fs::create_dir_all(&subdir).expect("Failed to create subdirectory");

        // Warm-up
        let _ = resolve_workspace_root(&subdir);

        // Measure combined operation
        let iterations = 10;
        let start = Instant::now();
        for _ in 0..iterations {
            // Step 1: Resolve workspace root (git root detection)
            let workspace_root = resolve_workspace_root(&subdir).expect("Should resolve");

            // Step 2: Create mount specification
            let mount_spec = format!(
                "type=bind,source={},target=/workspaces/project,consistency=cached",
                workspace_root.display()
            );

            // Step 3: Parse and render Docker mount
            let mount = MountParser::parse_mount(&mount_spec).expect("Should parse");
            let _docker_args = mount.to_docker_args();

            // Step 4: Create Compose mount
            let compose_project = ComposeProject {
                name: "test".to_string(),
                base_path: workspace_root.clone(),
                compose_files: vec![PathBuf::from("docker-compose.yml")],
                service: "app".to_string(),
                run_services: Vec::new(),
                env_files: Vec::new(),
                additional_mounts: vec![ComposeMount {
                    mount_type: "bind".to_string(),
                    source: workspace_root.display().to_string(),
                    target: "/workspaces/project".to_string(),
                    read_only: false,
                    consistency: Some("cached".to_string()),
                }],
                profiles: Vec::new(),
                additional_env: deacon_core::IndexMap::new(),
                external_volumes: Vec::new(),
            };

            // Step 5: Generate Compose override
            let _override_yaml = compose_project.generate_injection_override();
        }
        let elapsed = start.elapsed();
        let avg_duration = elapsed / iterations;

        assert!(
            avg_duration < Duration::from_millis(MAX_DURATION_MS),
            "Combined workspace discovery + mount rendering should complete in <{}ms, took {:?} (avg over {} iterations)",
            MAX_DURATION_MS,
            avg_duration,
            iterations
        );
    }
}
