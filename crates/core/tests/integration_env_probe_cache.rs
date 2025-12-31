//! Integration tests validating environment probe caching behavior.

use deacon_core::container_env_probe::{ContainerEnvironmentProber, ContainerProbeMode};
use deacon_core::docker::mock::{MockContainer, MockDocker, MockExecResponse};
use std::collections::HashMap;
use tempfile::TempDir;

/// Helper function to create mock environment output for probe
fn create_mock_env_output(vars: &HashMap<String, String>) -> String {
    vars.iter()
        .map(|(k, v)| format!("{}={}", k, v))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Test that cache miss creates cache file on first probe.
///
/// This test verifies:
/// 1. First probe with no existing cache executes fresh shell probe
/// 2. Cache file is created with correct naming format (env_probe_{container_id}_{user}.json)
/// 3. Cache file contains valid JSON with captured environment variables
/// 4. Cache content matches the probe result exactly
#[tokio::test]
async fn test_cache_miss_creates_cache_file() {
    // Create temporary cache directory
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let cache_folder = temp_dir.path();

    // Create a mock Docker client with a test container
    let docker = MockDocker::new();

    let container = MockContainer::new(
        "test-cache-miss".to_string(),
        "test-cache-miss".to_string(),
        "alpine:latest".to_string(),
    )
    .with_env({
        let mut env = HashMap::new();
        env.insert(
            "PATH".to_string(),
            "/usr/local/bin:/usr/bin:/bin".to_string(),
        );
        env.insert("HOME".to_string(), "/root".to_string());
        env.insert("USER".to_string(), "root".to_string());
        env.insert("SHELL".to_string(), "/bin/bash".to_string());
        env
    });

    docker.add_container(container);
    let container_id = "test-cache-miss";

    // Configure mock responses for shell detection and environment probe
    let env_vars = {
        let mut env = HashMap::new();
        env.insert(
            "PATH".to_string(),
            "/usr/local/bin:/usr/bin:/bin".to_string(),
        );
        env.insert("HOME".to_string(), "/root".to_string());
        env.insert("USER".to_string(), "root".to_string());
        env.insert("SHELL".to_string(), "/bin/bash".to_string());
        env
    };

    // Mock response for $SHELL check
    docker.set_exec_response(
        "sh -c echo $SHELL 2>/dev/null".to_string(),
        MockExecResponse {
            exit_code: 0,
            success: true,
            stdout: Some("/bin/bash\n".to_string()),
            stderr: None,
            delay: None,
        },
    );

    // Mock response for shell existence check
    docker.set_exec_response(
        "sh -c test -x /bin/bash 2>/dev/null".to_string(),
        MockExecResponse {
            exit_code: 0,
            success: true,
            stdout: None,
            stderr: None,
            delay: None,
        },
    );

    // Mock response for env probe command
    docker.set_exec_response(
        "sh -c /bin/bash -lc 'env 2>/dev/null'".to_string(),
        MockExecResponse {
            exit_code: 0,
            success: true,
            stdout: Some(create_mock_env_output(&env_vars)),
            stderr: None,
            delay: None,
        },
    );

    let prober = ContainerEnvironmentProber::new();

    // Verify cache file does not exist before probe
    let expected_cache_file = cache_folder.join(format!("env_probe_{}_root.json", container_id));
    assert!(
        !expected_cache_file.exists(),
        "Cache file should not exist before first probe"
    );

    // First probe - cache miss (should execute shell and create cache)
    let result = prober
        .probe_container_environment(
            &docker,
            container_id,
            ContainerProbeMode::LoginShell,
            Some("root"),
            Some(cache_folder),
        )
        .await
        .expect("First probe should succeed on cache miss");

    // Verify probe executed fresh shell (not from cache)
    assert_ne!(
        result.shell_used, "cache",
        "Cache miss should execute fresh shell probe, not load from cache"
    );

    // Verify environment variables were captured
    assert!(
        !result.env_vars.is_empty(),
        "Cache miss should capture environment variables from shell execution"
    );
    assert!(
        result.var_count > 0,
        "Variable count should be greater than zero"
    );
    assert_eq!(
        result.var_count,
        result.env_vars.len(),
        "Variable count should match environment variables map size"
    );

    // Verify cache file was created with correct naming format
    assert!(
        expected_cache_file.exists(),
        "Cache file should be created at {:?} after cache miss",
        expected_cache_file
    );

    // Verify cache file contains valid JSON
    let cache_content =
        std::fs::read_to_string(&expected_cache_file).expect("Should be able to read cache file");
    let cached_env: HashMap<String, String> = serde_json::from_str(&cache_content)
        .expect("Cache file should contain valid JSON in HashMap<String, String> format");

    // Verify cached data matches probe result exactly
    assert_eq!(
        cached_env, result.env_vars,
        "Cache file content should match probe result environment variables exactly"
    );

    // Verify cache contains expected environment variables
    assert!(
        cached_env.contains_key("PATH"),
        "Cache should contain PATH environment variable"
    );
    assert_eq!(
        cached_env.get("PATH"),
        Some(&"/usr/local/bin:/usr/bin:/bin".to_string()),
        "PATH value should match mocked container environment"
    );
}

/// Test that second probe loads from cache without shell execution.
///
/// This test verifies:
/// 1. First probe creates cache file
/// 2. Second probe loads from cache (shell_used = "cache")
/// 3. Environment variables are identical between runs
/// 4. Cache file exists on disk with correct content
#[tokio::test]
async fn test_cache_hit() {
    // Create temporary cache directory
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let cache_folder = temp_dir.path();

    // Create a mock Docker client with a test container
    let docker = MockDocker::new();

    let container = MockContainer::new(
        "test-cache-container".to_string(),
        "test-cache-container".to_string(),
        "alpine:latest".to_string(),
    )
    .with_env({
        let mut env = HashMap::new();
        env.insert(
            "PATH".to_string(),
            "/usr/local/bin:/usr/bin:/bin".to_string(),
        );
        env.insert("HOME".to_string(), "/root".to_string());
        env.insert("USER".to_string(), "root".to_string());
        env
    });

    docker.add_container(container);
    let container_id = "test-cache-container";

    let prober = ContainerEnvironmentProber::new();

    // Configure mock responses for shell detection and environment probe
    let env_vars = {
        let mut env = HashMap::new();
        env.insert(
            "PATH".to_string(),
            "/usr/local/bin:/usr/bin:/bin".to_string(),
        );
        env.insert("HOME".to_string(), "/root".to_string());
        env.insert("USER".to_string(), "root".to_string());
        env
    };

    // Mock response for $SHELL check
    docker.set_exec_response(
        "sh -c echo $SHELL 2>/dev/null".to_string(),
        MockExecResponse {
            exit_code: 0,
            success: true,
            stdout: Some("/bin/bash\n".to_string()),
            stderr: None,
            delay: None,
        },
    );

    // Mock response for shell existence check
    docker.set_exec_response(
        "sh -c test -x /bin/bash 2>/dev/null".to_string(),
        MockExecResponse {
            exit_code: 0,
            success: true,
            stdout: None,
            stderr: None,
            delay: None,
        },
    );

    // Mock response for env probe command - create output in env format
    let env_output = env_vars
        .iter()
        .map(|(k, v)| format!("{}={}", k, v))
        .collect::<Vec<_>>()
        .join("\n");

    docker.set_exec_response(
        "sh -c /bin/bash -lc 'env 2>/dev/null'".to_string(),
        MockExecResponse {
            exit_code: 0,
            success: true,
            stdout: Some(env_output),
            stderr: None,
            delay: None,
        },
    );

    // First probe - cache miss (should execute shell)
    let result1 = prober
        .probe_container_environment(
            &docker,
            container_id,
            ContainerProbeMode::LoginShell,
            Some("root"),
            Some(cache_folder),
        )
        .await
        .expect("First probe failed");

    // Verify first probe executed shell (not from cache)
    assert_ne!(
        result1.shell_used, "cache",
        "First probe should execute shell, not load from cache"
    );
    assert!(
        !result1.env_vars.is_empty(),
        "First probe should capture environment variables"
    );

    // Verify cache file was created
    let cache_path = cache_folder.join(format!("env_probe_{}_root.json", container_id));
    assert!(
        cache_path.exists(),
        "Cache file should exist after first probe"
    );

    // Verify cache file contains valid JSON
    let cache_content = std::fs::read_to_string(&cache_path).expect("Failed to read cache file");
    let cached_env: HashMap<String, String> =
        serde_json::from_str(&cache_content).expect("Cache file should contain valid JSON");
    assert_eq!(
        cached_env, result1.env_vars,
        "Cache file should contain same env vars as probe result"
    );

    // Second probe - cache hit (should load from cache without shell execution)
    let result2 = prober
        .probe_container_environment(
            &docker,
            container_id,
            ContainerProbeMode::LoginShell,
            Some("root"),
            Some(cache_folder),
        )
        .await
        .expect("Second probe failed");

    // Verify second probe loaded from cache
    assert_eq!(
        result2.shell_used, "cache",
        "Second probe should load from cache (shell_used = 'cache')"
    );

    // Verify environment variables are identical
    assert_eq!(
        result1.env_vars, result2.env_vars,
        "Cached environment should match original probe"
    );

    // Verify variable counts match
    assert_eq!(
        result1.var_count, result2.var_count,
        "Variable count should be identical"
    );
}

/// Test that no caching occurs when cache_folder is None.
///
/// This test verifies:
/// 1. When cache_folder=None, no cache file is created
/// 2. Multiple probes execute fresh each time (not cached)
/// 3. Results are still valid
#[tokio::test]
async fn test_no_caching_when_none() {
    // Create temporary directory to verify nothing is written
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let cache_folder = temp_dir.path();

    // Create a mock Docker client with a test container
    let docker = MockDocker::new();

    let container = MockContainer::new(
        "test-no-cache-container".to_string(),
        "test-no-cache-container".to_string(),
        "alpine:latest".to_string(),
    )
    .with_env({
        let mut env = HashMap::new();
        env.insert(
            "PATH".to_string(),
            "/usr/local/bin:/usr/bin:/bin".to_string(),
        );
        env.insert("HOME".to_string(), "/root".to_string());
        env.insert("USER".to_string(), "root".to_string());
        env
    });

    docker.add_container(container);
    let container_id = "test-no-cache-container";

    // Configure mock responses for shell detection and environment probe
    let env_vars = {
        let mut env = HashMap::new();
        env.insert(
            "PATH".to_string(),
            "/usr/local/bin:/usr/bin:/bin".to_string(),
        );
        env.insert("HOME".to_string(), "/root".to_string());
        env.insert("USER".to_string(), "root".to_string());
        env
    };

    // Mock response for $SHELL check
    docker.set_exec_response(
        "sh -c echo $SHELL 2>/dev/null".to_string(),
        MockExecResponse {
            exit_code: 0,
            success: true,
            stdout: Some("/bin/bash\n".to_string()),
            stderr: None,
            delay: None,
        },
    );

    // Mock response for shell existence check
    docker.set_exec_response(
        "sh -c test -x /bin/bash 2>/dev/null".to_string(),
        MockExecResponse {
            exit_code: 0,
            success: true,
            stdout: None,
            stderr: None,
            delay: None,
        },
    );

    // Mock response for environment probe command
    docker.set_exec_response(
        "sh -c /bin/bash -lc 'env 2>/dev/null'".to_string(),
        MockExecResponse {
            exit_code: 0,
            success: true,
            stdout: Some(create_mock_env_output(&env_vars)),
            stderr: None,
            delay: None,
        },
    );

    let prober = ContainerEnvironmentProber::new();

    // First probe with cache_folder=None (no caching)
    let result1 = prober
        .probe_container_environment(
            &docker,
            container_id,
            ContainerProbeMode::LoginShell,
            Some("root"),
            None, // cache_folder is None - no caching
        )
        .await
        .expect("First probe failed");

    // Verify probe executed shell (not from cache)
    assert_ne!(
        result1.shell_used, "cache",
        "Probe should execute shell, not load from cache"
    );
    assert!(
        !result1.env_vars.is_empty(),
        "Probe should capture environment variables"
    );

    // Verify NO cache file was created in temp directory
    let would_be_cache_path = cache_folder.join(format!("env_probe_{}_root.json", container_id));
    assert!(
        !would_be_cache_path.exists(),
        "No cache file should be created when cache_folder=None"
    );

    // Verify no other cache files were created in temp directory
    let entries = std::fs::read_dir(cache_folder)
        .expect("Failed to read temp dir")
        .count();
    assert_eq!(
        entries, 0,
        "Temp directory should remain empty when cache_folder=None"
    );

    // Second probe with cache_folder=None (should also execute fresh)
    let result2 = prober
        .probe_container_environment(
            &docker,
            container_id,
            ContainerProbeMode::LoginShell,
            Some("root"),
            None, // cache_folder is still None
        )
        .await
        .expect("Second probe failed");

    // Verify second probe also executed shell (not cached)
    assert_ne!(
        result2.shell_used, "cache",
        "Second probe should also execute shell when cache_folder=None"
    );

    // Environment variables should be consistent between probes
    assert_eq!(
        result1.env_vars, result2.env_vars,
        "Environment variables should be consistent across probes"
    );

    // Verify still no cache files created
    assert_eq!(
        std::fs::read_dir(cache_folder)
            .expect("Failed to read temp dir")
            .count(),
        0,
        "Temp directory should remain empty after multiple probes with cache_folder=None"
    );
}

/// Test that non-existent cache folder is created automatically.
///
/// This test verifies:
/// 1. Probe works even when cache folder doesn't exist
/// 2. Cache folder is created automatically via std::fs::create_dir_all
/// 3. Cache file is written to newly created folder
/// 4. Subsequent probe can read from the cache
#[tokio::test]
async fn test_cache_folder_creation() {
    // Create temporary directory as parent
    let temp_dir = TempDir::new().expect("Failed to create temp dir");

    // Use a non-existent subdirectory as cache folder
    let cache_folder = temp_dir.path().join("nonexistent_cache_dir");

    // Verify cache folder doesn't exist yet
    assert!(
        !cache_folder.exists(),
        "Cache folder should not exist before test"
    );

    // Create a mock Docker client with a test container
    let docker = MockDocker::new();

    let container = MockContainer::new(
        "test-folder-container".to_string(),
        "test-folder-container".to_string(),
        "alpine:latest".to_string(),
    )
    .with_env({
        let mut env = HashMap::new();
        env.insert(
            "PATH".to_string(),
            "/usr/local/bin:/usr/bin:/bin".to_string(),
        );
        env.insert("HOME".to_string(), "/root".to_string());
        env.insert("USER".to_string(), "root".to_string());
        env
    });

    docker.add_container(container);
    let container_id = "test-folder-container";

    // Configure mock responses for shell detection and environment probe
    let env_vars = {
        let mut env = HashMap::new();
        env.insert(
            "PATH".to_string(),
            "/usr/local/bin:/usr/bin:/bin".to_string(),
        );
        env.insert("HOME".to_string(), "/root".to_string());
        env.insert("USER".to_string(), "root".to_string());
        env
    };

    // Mock response for $SHELL check
    docker.set_exec_response(
        "sh -c echo $SHELL 2>/dev/null".to_string(),
        MockExecResponse {
            exit_code: 0,
            success: true,
            stdout: Some("/bin/bash\n".to_string()),
            stderr: None,
            delay: None,
        },
    );

    // Mock response for shell existence check
    docker.set_exec_response(
        "sh -c test -x /bin/bash 2>/dev/null".to_string(),
        MockExecResponse {
            exit_code: 0,
            success: true,
            stdout: None,
            stderr: None,
            delay: None,
        },
    );

    // Mock response for environment probe command
    docker.set_exec_response(
        "sh -c /bin/bash -lc 'env 2>/dev/null'".to_string(),
        MockExecResponse {
            exit_code: 0,
            success: true,
            stdout: Some(create_mock_env_output(&env_vars)),
            stderr: None,
            delay: None,
        },
    );

    let prober = ContainerEnvironmentProber::new();

    // First probe with non-existent cache folder
    let result = prober
        .probe_container_environment(
            &docker,
            container_id,
            ContainerProbeMode::LoginShell,
            Some("root"),
            Some(&cache_folder),
        )
        .await
        .expect("Probe should succeed even with non-existent cache folder");

    // Verify probe succeeded and captured environment
    assert!(
        !result.env_vars.is_empty(),
        "Probe should capture environment variables"
    );
    assert_ne!(
        result.shell_used, "cache",
        "First probe should execute shell, not load from cache"
    );

    // Verify cache folder was created
    assert!(
        cache_folder.exists(),
        "Cache folder should be created automatically"
    );
    assert!(cache_folder.is_dir(), "Cache folder should be a directory");

    // Verify cache file was created inside the new folder
    let cache_path = cache_folder.join(format!("env_probe_{}_root.json", container_id));
    assert!(
        cache_path.exists(),
        "Cache file should be created in the new folder"
    );

    // Verify cache file contains valid JSON
    let cache_content =
        std::fs::read_to_string(&cache_path).expect("Should be able to read cache file");
    let cached_env: HashMap<String, String> =
        serde_json::from_str(&cache_content).expect("Cache file should contain valid JSON");

    // Verify cached environment matches what we probed
    assert_eq!(
        cached_env, result.env_vars,
        "Cache file should contain same env vars as probe result"
    );

    // Second probe - verify cache can be read from newly created folder
    let result2 = prober
        .probe_container_environment(
            &docker,
            container_id,
            ContainerProbeMode::LoginShell,
            Some("root"),
            Some(&cache_folder),
        )
        .await
        .expect("Second probe should succeed");

    // Verify second probe loaded from cache
    assert_eq!(
        result2.shell_used, "cache",
        "Second probe should load from cache in newly created folder"
    );
    assert_eq!(
        result.env_vars, result2.env_vars,
        "Cached environment should match original probe"
    );
}

/// Test that per-user cache isolation creates separate cache files.
///
/// This test verifies:
/// 1. Probing as user "alice" creates cache file with alice in the name
/// 2. Probing as user "bob" creates separate cache file with bob in the name
/// 3. Both cache files exist with correct naming format: {container_id}_alice.json and {container_id}_bob.json
/// 4. Cache files contain different data for each user
#[tokio::test]
async fn test_per_user_cache_isolation() {
    // Create temporary cache directory
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let cache_folder = temp_dir.path();

    // Create a mock Docker client with a test container
    let docker = MockDocker::new();

    let container = MockContainer::new(
        "test-user-isolation".to_string(),
        "test-user-isolation".to_string(),
        "alpine:latest".to_string(),
    )
    .with_env({
        let mut env = HashMap::new();
        env.insert(
            "PATH".to_string(),
            "/usr/local/bin:/usr/bin:/bin".to_string(),
        );
        env.insert("HOME".to_string(), "/root".to_string());
        env
    });

    docker.add_container(container);
    let container_id = "test-user-isolation";

    let prober = ContainerEnvironmentProber::new();

    // Configure mock responses for alice's environment
    let alice_env_vars = {
        let mut env = HashMap::new();
        env.insert(
            "PATH".to_string(),
            "/usr/local/bin:/usr/bin:/bin".to_string(),
        );
        env.insert("HOME".to_string(), "/home/alice".to_string());
        env.insert("USER".to_string(), "alice".to_string());
        env
    };

    // Mock response for $SHELL check (alice)
    docker.set_exec_response(
        "sh -c echo $SHELL 2>/dev/null".to_string(),
        MockExecResponse {
            exit_code: 0,
            success: true,
            stdout: Some("/bin/bash\n".to_string()),
            stderr: None,
            delay: None,
        },
    );

    // Mock response for shell existence check
    docker.set_exec_response(
        "sh -c test -x /bin/bash 2>/dev/null".to_string(),
        MockExecResponse {
            exit_code: 0,
            success: true,
            stdout: None,
            stderr: None,
            delay: None,
        },
    );

    // Mock response for env probe command (alice)
    docker.set_exec_response(
        "sh -c /bin/bash -lc 'env 2>/dev/null'".to_string(),
        MockExecResponse {
            exit_code: 0,
            success: true,
            stdout: Some(create_mock_env_output(&alice_env_vars)),
            stderr: None,
            delay: None,
        },
    );

    // Probe as user "alice"
    let alice_result = prober
        .probe_container_environment(
            &docker,
            container_id,
            ContainerProbeMode::LoginShell,
            Some("alice"),
            Some(cache_folder),
        )
        .await
        .expect("Alice probe should succeed");

    // Verify alice's probe executed fresh shell
    assert_ne!(
        alice_result.shell_used, "cache",
        "Alice's first probe should execute shell, not load from cache"
    );
    assert!(
        !alice_result.env_vars.is_empty(),
        "Alice's probe should capture environment variables"
    );

    // Verify alice's cache file was created with correct naming
    let alice_cache_file = cache_folder.join(format!("env_probe_{}_alice.json", container_id));
    assert!(
        alice_cache_file.exists(),
        "Alice's cache file should exist at {:?}",
        alice_cache_file
    );

    // Configure mock responses for bob's environment
    let bob_env_vars = {
        let mut env = HashMap::new();
        env.insert(
            "PATH".to_string(),
            "/usr/local/bin:/usr/bin:/bin:/usr/local/go/bin".to_string(),
        );
        env.insert("HOME".to_string(), "/home/bob".to_string());
        env.insert("USER".to_string(), "bob".to_string());
        env.insert("GOPATH".to_string(), "/home/bob/go".to_string());
        env
    };

    // Update mock response for env probe command (bob has different env)
    docker.set_exec_response(
        "sh -c /bin/bash -lc 'env 2>/dev/null'".to_string(),
        MockExecResponse {
            exit_code: 0,
            success: true,
            stdout: Some(create_mock_env_output(&bob_env_vars)),
            stderr: None,
            delay: None,
        },
    );

    // Probe as user "bob"
    let bob_result = prober
        .probe_container_environment(
            &docker,
            container_id,
            ContainerProbeMode::LoginShell,
            Some("bob"),
            Some(cache_folder),
        )
        .await
        .expect("Bob probe should succeed");

    // Verify bob's probe executed fresh shell
    assert_ne!(
        bob_result.shell_used, "cache",
        "Bob's first probe should execute shell, not load from cache"
    );
    assert!(
        !bob_result.env_vars.is_empty(),
        "Bob's probe should capture environment variables"
    );

    // Verify bob's cache file was created with correct naming
    let bob_cache_file = cache_folder.join(format!("env_probe_{}_bob.json", container_id));
    assert!(
        bob_cache_file.exists(),
        "Bob's cache file should exist at {:?}",
        bob_cache_file
    );

    // Verify both cache files exist simultaneously
    assert!(
        alice_cache_file.exists() && bob_cache_file.exists(),
        "Both alice and bob cache files should exist simultaneously"
    );

    // Verify alice's cache file contains alice's environment
    let alice_cache_content =
        std::fs::read_to_string(&alice_cache_file).expect("Should read alice's cache file");
    let alice_cached_env: HashMap<String, String> = serde_json::from_str(&alice_cache_content)
        .expect("Alice's cache file should contain valid JSON");
    assert_eq!(
        alice_cached_env, alice_result.env_vars,
        "Alice's cache file should match her probe result"
    );

    // Verify bob's cache file contains bob's environment
    let bob_cache_content =
        std::fs::read_to_string(&bob_cache_file).expect("Should read bob's cache file");
    let bob_cached_env: HashMap<String, String> = serde_json::from_str(&bob_cache_content)
        .expect("Bob's cache file should contain valid JSON");
    assert_eq!(
        bob_cached_env, bob_result.env_vars,
        "Bob's cache file should match his probe result"
    );

    // Verify alice and bob have different environments
    assert_ne!(
        alice_result.env_vars, bob_result.env_vars,
        "Alice and bob should have different environment variables"
    );

    // Verify HOME is different for alice vs bob
    assert_eq!(
        alice_cached_env.get("HOME"),
        Some(&"/home/alice".to_string()),
        "Alice's HOME should be /home/alice"
    );
    assert_eq!(
        bob_cached_env.get("HOME"),
        Some(&"/home/bob".to_string()),
        "Bob's HOME should be /home/bob"
    );

    // Verify bob has GOPATH but alice doesn't
    assert!(
        !alice_cached_env.contains_key("GOPATH"),
        "Alice should not have GOPATH in her environment"
    );
    assert!(
        bob_cached_env.contains_key("GOPATH"),
        "Bob should have GOPATH in his environment"
    );
}

/// Test that root user handling defaults to "root" when user is None.
///
/// This test verifies:
/// 1. Probing with user=None creates cache file with "root" as user component
/// 2. Cache file naming format is {container_id}_root.json (not {container_id}_.json)
/// 3. Cache file is created and can be read back correctly
/// 4. Subsequent probe with user=None loads from the same root cache file
#[tokio::test]
async fn test_root_user_handling_with_user_none() {
    // Create temporary cache directory
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let cache_folder = temp_dir.path();

    // Create a mock Docker client with a test container
    let docker = MockDocker::new();

    let container = MockContainer::new(
        "test-root-user".to_string(),
        "test-root-user".to_string(),
        "alpine:latest".to_string(),
    )
    .with_env({
        let mut env = HashMap::new();
        env.insert(
            "PATH".to_string(),
            "/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin".to_string(),
        );
        env.insert("HOME".to_string(), "/root".to_string());
        env.insert("USER".to_string(), "root".to_string());
        env.insert("SHELL".to_string(), "/bin/bash".to_string());
        env
    });

    docker.add_container(container);
    let container_id = "test-root-user";

    let prober = ContainerEnvironmentProber::new();

    // Configure mock responses for root environment
    let root_env_vars = {
        let mut env = HashMap::new();
        env.insert(
            "PATH".to_string(),
            "/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin".to_string(),
        );
        env.insert("HOME".to_string(), "/root".to_string());
        env.insert("USER".to_string(), "root".to_string());
        env.insert("SHELL".to_string(), "/bin/bash".to_string());
        env
    };

    // Mock response for $SHELL check
    docker.set_exec_response(
        "sh -c echo $SHELL 2>/dev/null".to_string(),
        MockExecResponse {
            exit_code: 0,
            success: true,
            stdout: Some("/bin/bash\n".to_string()),
            stderr: None,
            delay: None,
        },
    );

    // Mock response for shell existence check
    docker.set_exec_response(
        "sh -c test -x /bin/bash 2>/dev/null".to_string(),
        MockExecResponse {
            exit_code: 0,
            success: true,
            stdout: None,
            stderr: None,
            delay: None,
        },
    );

    // Mock response for env probe command
    docker.set_exec_response(
        "sh -c /bin/bash -lc 'env 2>/dev/null'".to_string(),
        MockExecResponse {
            exit_code: 0,
            success: true,
            stdout: Some(create_mock_env_output(&root_env_vars)),
            stderr: None,
            delay: None,
        },
    );

    // Probe with user=None (should default to "root")
    let result = prober
        .probe_container_environment(
            &docker,
            container_id,
            ContainerProbeMode::LoginShell,
            None, // user is None - should default to "root" in cache filename
            Some(cache_folder),
        )
        .await
        .expect("Probe with user=None should succeed");

    // Verify probe executed fresh shell
    assert_ne!(
        result.shell_used, "cache",
        "First probe with user=None should execute shell, not load from cache"
    );
    assert!(
        !result.env_vars.is_empty(),
        "Probe should capture environment variables"
    );

    // Verify cache file was created with "root" as user component
    let expected_cache_file = cache_folder.join(format!("env_probe_{}_root.json", container_id));
    assert!(
        expected_cache_file.exists(),
        "Cache file should exist at {:?} with 'root' as user component",
        expected_cache_file
    );

    // Verify no cache file with empty user component exists
    let incorrect_cache_file = cache_folder.join(format!("env_probe_{}_.json", container_id));
    assert!(
        !incorrect_cache_file.exists(),
        "Cache file should NOT exist with empty user component at {:?}",
        incorrect_cache_file
    );

    // Verify cache file contains valid JSON with root environment
    let cache_content =
        std::fs::read_to_string(&expected_cache_file).expect("Should read root cache file");
    let cached_env: HashMap<String, String> =
        serde_json::from_str(&cache_content).expect("Root cache file should contain valid JSON");
    assert_eq!(
        cached_env, result.env_vars,
        "Cache file should match probe result for root user"
    );

    // Verify root-specific environment variables
    assert_eq!(
        cached_env.get("HOME"),
        Some(&"/root".to_string()),
        "Root HOME should be /root"
    );
    assert_eq!(
        cached_env.get("USER"),
        Some(&"root".to_string()),
        "USER should be root"
    );

    // Second probe with user=None - should load from same root cache file
    let result2 = prober
        .probe_container_environment(
            &docker,
            container_id,
            ContainerProbeMode::LoginShell,
            None, // user is still None
            Some(cache_folder),
        )
        .await
        .expect("Second probe with user=None should succeed");

    // Verify second probe loaded from cache
    assert_eq!(
        result2.shell_used, "cache",
        "Second probe with user=None should load from root cache file"
    );
    assert_eq!(
        result.env_vars, result2.env_vars,
        "Cached root environment should match original probe"
    );

    // Verify only one cache file exists (the root one)
    let cache_files: Vec<_> = std::fs::read_dir(cache_folder)
        .expect("Should read cache folder")
        .filter_map(|entry| entry.ok())
        .filter(|entry| {
            entry
                .file_name()
                .to_string_lossy()
                .starts_with("env_probe_")
        })
        .collect();

    assert_eq!(
        cache_files.len(),
        1,
        "Should have exactly one cache file for root user"
    );
    assert_eq!(
        cache_files[0].file_name().to_string_lossy(),
        format!("env_probe_{}_root.json", container_id),
        "The single cache file should be the root user cache file"
    );
}

/// Test that cache is not reused across different users.
///
/// This test verifies:
/// 1. Probing as user "alice" creates alice's cache file
/// 2. Probing as user "bob" does NOT load alice's cache
/// 3. Bob's probe executes fresh shell (not from cache)
/// 4. Bob's probe creates separate cache file for bob
#[tokio::test]
async fn test_cache_non_reuse_across_users() {
    // Create temporary cache directory
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let cache_folder = temp_dir.path();

    // Create a mock Docker client with a test container
    let docker = MockDocker::new();

    let container = MockContainer::new(
        "test-cross-user".to_string(),
        "test-cross-user".to_string(),
        "alpine:latest".to_string(),
    )
    .with_env({
        let mut env = HashMap::new();
        env.insert(
            "PATH".to_string(),
            "/usr/local/bin:/usr/bin:/bin".to_string(),
        );
        env.insert("HOME".to_string(), "/root".to_string());
        env
    });

    docker.add_container(container);
    let container_id = "test-cross-user";

    let prober = ContainerEnvironmentProber::new();

    // Configure mock responses for alice's environment
    let alice_env_vars = {
        let mut env = HashMap::new();
        env.insert(
            "PATH".to_string(),
            "/usr/local/bin:/usr/bin:/bin".to_string(),
        );
        env.insert("HOME".to_string(), "/home/alice".to_string());
        env.insert("USER".to_string(), "alice".to_string());
        env
    };

    // Mock response for $SHELL check (alice)
    docker.set_exec_response(
        "sh -c echo $SHELL 2>/dev/null".to_string(),
        MockExecResponse {
            exit_code: 0,
            success: true,
            stdout: Some("/bin/bash\n".to_string()),
            stderr: None,
            delay: None,
        },
    );

    // Mock response for shell existence check
    docker.set_exec_response(
        "sh -c test -x /bin/bash 2>/dev/null".to_string(),
        MockExecResponse {
            exit_code: 0,
            success: true,
            stdout: None,
            stderr: None,
            delay: None,
        },
    );

    // Mock response for env probe command (alice)
    docker.set_exec_response(
        "sh -c /bin/bash -lc 'env 2>/dev/null'".to_string(),
        MockExecResponse {
            exit_code: 0,
            success: true,
            stdout: Some(create_mock_env_output(&alice_env_vars)),
            stderr: None,
            delay: None,
        },
    );

    // Probe as user "alice" (first probe - creates alice's cache)
    let alice_result = prober
        .probe_container_environment(
            &docker,
            container_id,
            ContainerProbeMode::LoginShell,
            Some("alice"),
            Some(cache_folder),
        )
        .await
        .expect("Alice probe should succeed");

    // Verify alice's probe executed fresh shell
    assert_ne!(
        alice_result.shell_used, "cache",
        "Alice's probe should execute shell, not load from cache"
    );

    // Verify alice's cache file was created
    let alice_cache_file = cache_folder.join(format!("env_probe_{}_alice.json", container_id));
    assert!(
        alice_cache_file.exists(),
        "Alice's cache file should exist after probe"
    );

    // Configure mock responses for bob's environment (different from alice)
    let bob_env_vars = {
        let mut env = HashMap::new();
        env.insert(
            "PATH".to_string(),
            "/usr/local/bin:/usr/bin:/bin:/opt/bin".to_string(),
        );
        env.insert("HOME".to_string(), "/home/bob".to_string());
        env.insert("USER".to_string(), "bob".to_string());
        env.insert("CUSTOM_VAR".to_string(), "bob_value".to_string());
        env
    };

    // Update mock response for env probe command (bob has different env)
    docker.set_exec_response(
        "sh -c /bin/bash -lc 'env 2>/dev/null'".to_string(),
        MockExecResponse {
            exit_code: 0,
            success: true,
            stdout: Some(create_mock_env_output(&bob_env_vars)),
            stderr: None,
            delay: None,
        },
    );

    // Probe as user "bob" (alice's cache already exists)
    let bob_result = prober
        .probe_container_environment(
            &docker,
            container_id,
            ContainerProbeMode::LoginShell,
            Some("bob"),
            Some(cache_folder),
        )
        .await
        .expect("Bob probe should succeed");

    // CRITICAL ASSERTION: Bob's probe should NOT load from alice's cache
    // It should execute a fresh shell probe instead
    assert_ne!(
        bob_result.shell_used, "cache",
        "Bob's probe should execute fresh shell, NOT load from alice's cache"
    );

    // Verify bob's probe captured environment (not from alice's cache)
    assert!(
        !bob_result.env_vars.is_empty(),
        "Bob's probe should capture environment variables"
    );

    // Verify bob's environment is different from alice's
    assert_ne!(
        alice_result.env_vars, bob_result.env_vars,
        "Bob's environment should be different from alice's (proves no cache reuse)"
    );

    // Verify bob-specific environment variables are present
    assert_eq!(
        bob_result.env_vars.get("HOME"),
        Some(&"/home/bob".to_string()),
        "Bob's HOME should be /home/bob (not alice's)"
    );
    assert_eq!(
        bob_result.env_vars.get("USER"),
        Some(&"bob".to_string()),
        "Bob's USER should be bob (not alice)"
    );
    assert_eq!(
        bob_result.env_vars.get("CUSTOM_VAR"),
        Some(&"bob_value".to_string()),
        "Bob should have his custom variable (not in alice's cache)"
    );

    // Verify bob's cache file was created separately
    let bob_cache_file = cache_folder.join(format!("env_probe_{}_bob.json", container_id));
    assert!(
        bob_cache_file.exists(),
        "Bob's cache file should be created after probe"
    );

    // Verify both cache files exist (alice's and bob's)
    assert!(
        alice_cache_file.exists() && bob_cache_file.exists(),
        "Both alice and bob cache files should exist simultaneously"
    );

    // Verify alice's cache still contains alice's data (unchanged)
    let alice_cache_content =
        std::fs::read_to_string(&alice_cache_file).expect("Should read alice's cache file");
    let alice_cached_env: HashMap<String, String> = serde_json::from_str(&alice_cache_content)
        .expect("Alice's cache file should contain valid JSON");
    assert_eq!(
        alice_cached_env, alice_result.env_vars,
        "Alice's cache should remain unchanged after bob's probe"
    );

    // Verify bob's cache contains bob's data (not alice's)
    let bob_cache_content =
        std::fs::read_to_string(&bob_cache_file).expect("Should read bob's cache file");
    let bob_cached_env: HashMap<String, String> = serde_json::from_str(&bob_cache_content)
        .expect("Bob's cache file should contain valid JSON");
    assert_eq!(
        bob_cached_env, bob_result.env_vars,
        "Bob's cache should match his probe result"
    );
    assert_ne!(
        bob_cached_env, alice_cached_env,
        "Bob's cache should be different from alice's cache"
    );
}

/// Test that cache is invalidated when container ID changes (container rebuild scenario).
///
/// This test verifies:
/// 1. Probing container A creates cache file with container A's ID
/// 2. Simulating container rebuild (new container ID) invalidates old cache
/// 3. Probing new container B does NOT load from container A's cache
/// 4. New cache file is created for container B with new container ID
/// 5. Both cache files exist on disk (old cache is not deleted, just not reused)
#[tokio::test]
async fn test_container_id_invalidation_on_rebuild() {
    // Create temporary cache directory
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let cache_folder = temp_dir.path();

    // Create a mock Docker client
    let docker = MockDocker::new();

    // Container A - original container before rebuild
    let container_a_id = "test-container-original-abc123";
    let container_a = MockContainer::new(
        container_a_id.to_string(),
        "test-rebuild-container".to_string(),
        "alpine:latest".to_string(),
    )
    .with_env({
        let mut env = HashMap::new();
        env.insert(
            "PATH".to_string(),
            "/usr/local/bin:/usr/bin:/bin".to_string(),
        );
        env.insert("HOME".to_string(), "/home/vscode".to_string());
        env.insert("USER".to_string(), "vscode".to_string());
        env.insert("VERSION".to_string(), "1.0.0".to_string());
        env
    });

    docker.add_container(container_a);

    // Configure mock responses for container A's environment probe
    let container_a_env_vars = {
        let mut env = HashMap::new();
        env.insert(
            "PATH".to_string(),
            "/usr/local/bin:/usr/bin:/bin".to_string(),
        );
        env.insert("HOME".to_string(), "/home/vscode".to_string());
        env.insert("USER".to_string(), "vscode".to_string());
        env.insert("VERSION".to_string(), "1.0.0".to_string());
        env
    };

    // Mock response for $SHELL check
    docker.set_exec_response(
        "sh -c echo $SHELL 2>/dev/null".to_string(),
        MockExecResponse {
            exit_code: 0,
            success: true,
            stdout: Some("/bin/bash\n".to_string()),
            stderr: None,
            delay: None,
        },
    );

    // Mock response for shell existence check
    docker.set_exec_response(
        "sh -c test -x /bin/bash 2>/dev/null".to_string(),
        MockExecResponse {
            exit_code: 0,
            success: true,
            stdout: None,
            stderr: None,
            delay: None,
        },
    );

    // Mock response for env probe command (container A)
    docker.set_exec_response(
        "sh -c /bin/bash -lc 'env 2>/dev/null'".to_string(),
        MockExecResponse {
            exit_code: 0,
            success: true,
            stdout: Some(create_mock_env_output(&container_a_env_vars)),
            stderr: None,
            delay: None,
        },
    );

    let prober = ContainerEnvironmentProber::new();

    // Probe container A (first time - creates cache)
    let result_a = prober
        .probe_container_environment(
            &docker,
            container_a_id,
            ContainerProbeMode::LoginShell,
            Some("vscode"),
            Some(cache_folder),
        )
        .await
        .expect("Container A probe should succeed");

    // Verify container A probe executed fresh shell
    assert_ne!(
        result_a.shell_used, "cache",
        "Container A's first probe should execute shell, not load from cache"
    );
    assert!(
        !result_a.env_vars.is_empty(),
        "Container A should capture environment variables"
    );

    // Verify container A's cache file was created
    let container_a_cache_file =
        cache_folder.join(format!("env_probe_{}_vscode.json", container_a_id));
    assert!(
        container_a_cache_file.exists(),
        "Container A's cache file should exist at {:?}",
        container_a_cache_file
    );

    // Verify container A's cache contains correct data
    let cache_a_content =
        std::fs::read_to_string(&container_a_cache_file).expect("Should read container A cache");
    let cached_a_env: HashMap<String, String> = serde_json::from_str(&cache_a_content)
        .expect("Container A cache should contain valid JSON");
    assert_eq!(
        cached_a_env, result_a.env_vars,
        "Container A cache should match probe result"
    );
    assert_eq!(
        cached_a_env.get("VERSION"),
        Some(&"1.0.0".to_string()),
        "Container A should have VERSION=1.0.0"
    );

    // --- Simulate container rebuild: new container with different ID ---

    // Container B - rebuilt container with new ID (simulates docker rebuild)
    let container_b_id = "test-container-rebuilt-xyz789";
    let container_b = MockContainer::new(
        container_b_id.to_string(),
        "test-rebuild-container".to_string(), // Same name, different ID
        "alpine:latest".to_string(),
    )
    .with_env({
        let mut env = HashMap::new();
        env.insert(
            "PATH".to_string(),
            "/usr/local/bin:/usr/bin:/bin:/usr/local/go/bin".to_string(),
        );
        env.insert("HOME".to_string(), "/home/vscode".to_string());
        env.insert("USER".to_string(), "vscode".to_string());
        env.insert("VERSION".to_string(), "2.0.0".to_string()); // Different version
        env.insert("GOPATH".to_string(), "/home/vscode/go".to_string()); // New variable
        env
    });

    docker.add_container(container_b);

    // Configure mock responses for container B's environment (different from A)
    let container_b_env_vars = {
        let mut env = HashMap::new();
        env.insert(
            "PATH".to_string(),
            "/usr/local/bin:/usr/bin:/bin:/usr/local/go/bin".to_string(),
        );
        env.insert("HOME".to_string(), "/home/vscode".to_string());
        env.insert("USER".to_string(), "vscode".to_string());
        env.insert("VERSION".to_string(), "2.0.0".to_string());
        env.insert("GOPATH".to_string(), "/home/vscode/go".to_string());
        env
    };

    // Update mock response for env probe command (container B has different env)
    docker.set_exec_response(
        "sh -c /bin/bash -lc 'env 2>/dev/null'".to_string(),
        MockExecResponse {
            exit_code: 0,
            success: true,
            stdout: Some(create_mock_env_output(&container_b_env_vars)),
            stderr: None,
            delay: None,
        },
    );

    // Probe container B (after rebuild with new ID)
    let result_b = prober
        .probe_container_environment(
            &docker,
            container_b_id,
            ContainerProbeMode::LoginShell,
            Some("vscode"),
            Some(cache_folder),
        )
        .await
        .expect("Container B probe should succeed");

    // CRITICAL ASSERTION: Container B should NOT load from container A's cache
    // It should execute a fresh shell probe because container ID changed
    assert_ne!(
        result_b.shell_used, "cache",
        "Container B probe should execute fresh shell, NOT load from container A's cache (different container ID)"
    );

    // Verify container B captured environment (not from container A's cache)
    assert!(
        !result_b.env_vars.is_empty(),
        "Container B should capture environment variables"
    );

    // Verify container B's environment is different from container A's
    assert_ne!(
        result_a.env_vars, result_b.env_vars,
        "Container B's environment should differ from container A (proves cache invalidation)"
    );

    // Verify container B has updated/new environment variables
    assert_eq!(
        result_b.env_vars.get("VERSION"),
        Some(&"2.0.0".to_string()),
        "Container B should have VERSION=2.0.0 (not 1.0.0 from container A)"
    );
    assert_eq!(
        result_b.env_vars.get("GOPATH"),
        Some(&"/home/vscode/go".to_string()),
        "Container B should have GOPATH (new variable not in container A)"
    );
    assert!(
        !result_a.env_vars.contains_key("GOPATH"),
        "Container A should not have GOPATH (verifies environments are different)"
    );

    // Verify container B's cache file was created with NEW container ID
    let container_b_cache_file =
        cache_folder.join(format!("env_probe_{}_vscode.json", container_b_id));
    assert!(
        container_b_cache_file.exists(),
        "Container B's cache file should be created at {:?}",
        container_b_cache_file
    );

    // Verify both cache files exist (old cache not deleted, just not reused)
    assert!(
        container_a_cache_file.exists() && container_b_cache_file.exists(),
        "Both container A and B cache files should exist (old cache persists but is not reused)"
    );

    // Verify container B's cache contains container B's data
    let cache_b_content =
        std::fs::read_to_string(&container_b_cache_file).expect("Should read container B cache");
    let cached_b_env: HashMap<String, String> = serde_json::from_str(&cache_b_content)
        .expect("Container B cache should contain valid JSON");
    assert_eq!(
        cached_b_env, result_b.env_vars,
        "Container B cache should match its probe result"
    );

    // Verify container A's cache remains unchanged (not overwritten)
    let cache_a_content_after =
        std::fs::read_to_string(&container_a_cache_file).expect("Should read container A cache");
    assert_eq!(
        cache_a_content, cache_a_content_after,
        "Container A's cache should remain unchanged after container B probe"
    );

    // Verify cache files are truly different
    assert_ne!(
        cached_a_env, cached_b_env,
        "Container A and B cache files should contain different data"
    );

    // Verify second probe of container B loads from its own cache (not fresh)
    let result_b_second = prober
        .probe_container_environment(
            &docker,
            container_b_id,
            ContainerProbeMode::LoginShell,
            Some("vscode"),
            Some(cache_folder),
        )
        .await
        .expect("Container B second probe should succeed");

    // Verify second probe of container B loads from cache
    assert_eq!(
        result_b_second.shell_used, "cache",
        "Container B's second probe should load from its own cache"
    );
    assert_eq!(
        result_b.env_vars, result_b_second.env_vars,
        "Container B's cached environment should match its original probe"
    );
}

/// Test that corrupted JSON cache falls back to fresh probe and logs warning.
///
/// This test verifies:
/// 1. When cache file contains invalid/corrupted JSON, parse fails gracefully
/// 2. System logs WARN message about cache parse failure
/// 3. System falls back to executing fresh probe (not cached)
/// 4. Fresh probe succeeds and returns valid environment data
/// 5. Corrupted cache is overwritten with valid data on successful probe
#[tokio::test]
async fn test_corrupted_json_fallback() {
    // Create temporary cache directory
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let cache_folder = temp_dir.path();

    // Create a mock Docker client with a test container
    let docker = MockDocker::new();

    let container = MockContainer::new(
        "test-corrupted-cache".to_string(),
        "test-corrupted-cache".to_string(),
        "alpine:latest".to_string(),
    )
    .with_env({
        let mut env = HashMap::new();
        env.insert(
            "PATH".to_string(),
            "/usr/local/bin:/usr/bin:/bin".to_string(),
        );
        env.insert("HOME".to_string(), "/home/vscode".to_string());
        env.insert("USER".to_string(), "vscode".to_string());
        env
    });

    docker.add_container(container);
    let container_id = "test-corrupted-cache";

    // Configure mock responses for environment probe
    let valid_env_vars = {
        let mut env = HashMap::new();
        env.insert(
            "PATH".to_string(),
            "/usr/local/bin:/usr/bin:/bin".to_string(),
        );
        env.insert("HOME".to_string(), "/home/vscode".to_string());
        env.insert("USER".to_string(), "vscode".to_string());
        env
    };

    // Mock response for $SHELL check
    docker.set_exec_response(
        "sh -c echo $SHELL 2>/dev/null".to_string(),
        MockExecResponse {
            exit_code: 0,
            success: true,
            stdout: Some("/bin/bash\n".to_string()),
            stderr: None,
            delay: None,
        },
    );

    // Mock response for shell existence check
    docker.set_exec_response(
        "sh -c test -x /bin/bash 2>/dev/null".to_string(),
        MockExecResponse {
            exit_code: 0,
            success: true,
            stdout: None,
            stderr: None,
            delay: None,
        },
    );

    // Mock response for env probe command
    docker.set_exec_response(
        "sh -c /bin/bash -lc 'env 2>/dev/null'".to_string(),
        MockExecResponse {
            exit_code: 0,
            success: true,
            stdout: Some(create_mock_env_output(&valid_env_vars)),
            stderr: None,
            delay: None,
        },
    );

    // Manually create corrupted cache file
    // This simulates a scenario where cache file was truncated, corrupted by disk error,
    // or manually edited incorrectly
    std::fs::create_dir_all(cache_folder).expect("Should create cache folder");
    let cache_path = cache_folder.join(format!("env_probe_{}_vscode.json", container_id));

    // Write various types of corrupted JSON to test robustness
    let corrupted_json_examples = [
        "{invalid json}",            // Malformed JSON syntax
        "{\"KEY\": }",               // Incomplete JSON
        "[\"not\", \"a\", \"map\"]", // Wrong JSON type (array instead of object)
        "null",                      // Valid JSON but wrong type
        "\"just a string\"",         // String instead of object
        "{\"KEY\": 123}",            // Wrong value type (number instead of string)
        "{ broken",                  // Truncated JSON
        "",                          // Empty file
    ];

    for (index, corrupted_json) in corrupted_json_examples.iter().enumerate() {
        // Write corrupted JSON to cache file
        std::fs::write(&cache_path, corrupted_json)
            .expect("Should write corrupted cache file for testing");

        // Verify cache file exists and contains corrupted data
        assert!(
            cache_path.exists(),
            "Corrupted cache file should exist before probe (example {})",
            index
        );
        let cached_content = std::fs::read_to_string(&cache_path).expect("Should read cache file");
        assert_eq!(
            cached_content, *corrupted_json,
            "Cache file should contain corrupted JSON (example {})",
            index
        );

        let prober = ContainerEnvironmentProber::new();

        // Probe with corrupted cache present
        // Expected behavior: Parse fails, logs WARN, falls back to fresh probe
        let result = prober
            .probe_container_environment(
                &docker,
                container_id,
                ContainerProbeMode::LoginShell,
                Some("vscode"),
                Some(cache_folder),
            )
            .await
            .expect("Probe should succeed even with corrupted cache");

        // CRITICAL ASSERTION: Probe should NOT load from cache
        // It should execute fresh shell probe due to parse failure
        assert_ne!(
            result.shell_used, "cache",
            "Probe should execute fresh shell, NOT load from corrupted cache (example {})",
            index
        );

        // Verify probe captured valid environment (not corrupted data)
        assert!(
            !result.env_vars.is_empty(),
            "Probe should capture valid environment variables despite corrupted cache (example {})",
            index
        );
        assert!(
            result.var_count > 0,
            "Variable count should be greater than zero (example {})",
            index
        );

        // Verify environment contains expected variables
        assert!(
            result.env_vars.contains_key("PATH"),
            "Environment should contain PATH (example {})",
            index
        );
        assert_eq!(
            result.env_vars.get("HOME"),
            Some(&"/home/vscode".to_string()),
            "HOME should be /home/vscode (example {})",
            index
        );

        // Verify cache file was overwritten with valid data after successful probe
        let updated_cache_content =
            std::fs::read_to_string(&cache_path).expect("Should read updated cache file");

        // Should no longer contain corrupted data
        assert_ne!(
            updated_cache_content, *corrupted_json,
            "Cache file should be overwritten with valid JSON (example {})",
            index
        );

        // Parse to verify it's now valid JSON
        let updated_cached_env: HashMap<String, String> =
            serde_json::from_str(&updated_cache_content).unwrap_or_else(|_| {
                panic!(
                    "Updated cache file should contain valid JSON (example {})",
                    index
                )
            });

        // Verify updated cache matches probe result
        assert_eq!(
            updated_cached_env, result.env_vars,
            "Updated cache should match probe result (example {})",
            index
        );

        // Second probe - should now load from newly written valid cache
        let result2 = prober
            .probe_container_environment(
                &docker,
                container_id,
                ContainerProbeMode::LoginShell,
                Some("vscode"),
                Some(cache_folder),
            )
            .await
            .expect("Second probe should succeed");

        // Verify second probe loaded from cache (now valid)
        assert_eq!(
            result2.shell_used, "cache",
            "Second probe should load from newly written cache (example {})",
            index
        );
        assert_eq!(
            result.env_vars, result2.env_vars,
            "Cached environment should match original probe (example {})",
            index
        );
    }

    // Note: This test doesn't directly verify WARN log output because capturing
    // tracing logs in tests requires additional test infrastructure (tracing-subscriber).
    // However, the behavior is verified: corrupted cache is detected, probe falls back
    // to fresh execution, and cache is repaired with valid data.
    // Manual verification of WARN logs can be done with: RUST_LOG=warn cargo test
}
