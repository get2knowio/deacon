---
work-unit: update-remote-user-uid
flight-plan: consumer-core-completion
sequence: 4
depends-on: []
parallel-group: alpha
---

## Task

Complete updateRemoteUserUID implementation: host UID/GID detection, usermod/groupmod execution in container, skip conditions (non-Linux, root, already matching, explicitly false), graceful fallback on failure, with unit and integration tests

## Acceptance Criteria

- updateRemoteUserUID=true updates container user UID/GID to match host on Linux via usermod/groupmod [SC-001]
- updateRemoteUserUID=false skips the update entirely [SC-002]
- Root user (UID 0) is never modified [SC-003]
- Non-Linux platforms skip the update entirely [SC-004]
- UID already matching skips the update [SC-005]
- Failure to update UID logs warning but does not abort up [SC-006]
- All changes pass cargo clippy and cargo test [SC-019]

## File Scope

### Create


### Modify

- crates/core/src/user_mapping.rs
- crates/deacon/src/commands/up/helpers.rs
- crates/deacon/src/commands/up/mod.rs
- crates/core/src/docker.rs

### Protect


## Procedure

### Step 1: Read current implementation state
- MUST Read crates/core/src/user_mapping.rs full file. Note UserMapper trait (lines 177-230), UserMappingService (lines 232-443), mock (lines 517+), get_host_user_info() (lines 445-515)
- MUST Read crates/deacon/src/commands/up/helpers.rs lines 121-204. Note apply_user_mapping() only validates/logs, does NOT execute UserMappingService
- MUST Read crates/deacon/src/commands/up/container.rs lines 470-482. Note T017 TODO
- MUST Read crates/core/src/docker.rs and search for exec method on Docker struct

### Step 2: Create Docker UserMapper implementation
- MUST create concrete UserMapper trait impl that executes commands in container via docker exec
- SHOULD live in crates/deacon/src/commands/up/helpers.rs or new docker_user_mapper.rs
- get_current_user() MUST execute id in container and parse output
- get_user_info() MUST execute id username or parse /etc/passwd
- update_user_uid() MUST execute usermod -u uid username and groupmod -g gid group. MUST handle command-not-found gracefully
- create_home_directory() MUST execute mkdir -p and chown
- set_workspace_ownership() MUST execute chown -R uid:gid path
- All methods MUST use async tokio::process::Command, never panic

### Step 3: Wire UserMappingService into apply_user_mapping
- MUST Edit helpers.rs apply_user_mapping() lines 164-188
- MUST change function signature to accept runtime/docker reference
- MUST replace comment block with: create DockerUserMapper, create UserMappingService, call service.apply_user_mapping()
- On failure log WARNING but do NOT return Err (non-fatal per spec)
- MUST update call site in container.rs line 481

### Step 4: Implement skip conditions
- MUST verify needs_uid_mapping() returns false when update_remote_user_uid is false
- MUST verify get_host_user_info() returns error on non-Linux
- MUST add check: IF host UID is 0 (root), skip update and log why
- MUST verify already-matching UIDs skip update (line 311 in user_mapping.rs)

### Step 5: Add unit tests
- MUST add tests: root UID never modified, UID matching skips, disabled skips, graceful failure

### Step 6: Build and lint
- MUST run cargo fmt --all and cargo clippy --all-targets -- -D warnings
- MUST run make test-nextest-fast

## Test Specification

#[tokio::test]
async fn test_uid_update_skipped_for_root() {
    let mock = MockUserMapper::new();
    mock.set_current_user(UserInfo::new("root".into(), 0, 0, "/root".into(), "/bin/sh".into()));
    let service = UserMappingService::new(mock);
    let config = UserMappingConfig::new(Some("root".into()), None, true).with_host_user(0, 0);
    let result = service.apply_user_mapping("c1", &config).await.unwrap();
    assert!(!result.uid_updated);
}

#[tokio::test]
async fn test_uid_update_skipped_when_matching() {
    let mock = MockUserMapper::new();
    mock.set_user_info("vscode", UserInfo::new("vscode".into(), 1000, 1000, "/home/vscode".into(), "/bin/bash".into()));
    let service = UserMappingService::new(mock);
    let config = UserMappingConfig::new(Some("vscode".into()), None, true).with_host_user(1000, 1000);
    let result = service.apply_user_mapping("c1", &config).await.unwrap();
    assert!(!result.uid_updated);
}

#[tokio::test]
async fn test_uid_update_skipped_when_disabled() {
    let mock = MockUserMapper::new();
    let service = UserMappingService::new(mock);
    let config = UserMappingConfig::new(Some("vscode".into()), None, false);
    let result = service.apply_user_mapping("c1", &config).await.unwrap();
    assert!(!result.uid_updated);
}

## Verification

- cargo fmt --all -- --check
- cargo clippy --all-targets -- -D warnings 2>&1 | tail -3
- cargo test -p deacon-core -- user_mapping 2>&1 | tail -3
- make test-nextest-fast 2>&1 | tail -5
