---
work-unit: run-args-passthrough
flight-plan: consumer-core-completion
sequence: 2
depends-on: []
parallel-group: alpha
---

## Task

Wire runArgs array from devcontainer config through to docker create/run after Deacon flags and before image name, ignore in Compose mode, with unit and integration tests

## Acceptance Criteria

- runArgs values from devcontainer config are forwarded to docker create command, positioned after Deacon flags and before image name [SC-011]
- runArgs is ignored in Compose mode with a user-visible warning logged [SC-012]
- Unit tests verify runArgs passthrough ordering and empty-args behavior [SC-019]
- All changes pass cargo clippy and cargo test [SC-019]

## File Scope

### Create


### Modify

- crates/core/src/docker.rs
- crates/core/src/config.rs
- crates/deacon/src/commands/up/container.rs

### Protect


## Procedure

### Step 1: Read current runArgs implementation
- MUST Read crates/core/src/docker.rs lines 1748-1770 to confirm runArgs passthrough at line 1756
- MUST Read crates/core/src/config.rs and search for run_args field definition (around line 520-523) and merge logic (around line 1182)
- MUST Read crates/deacon/src/commands/up/container.rs to understand how config flows to create_container

### Step 2: Verify Docker mode passthrough
- The runArgs passthrough in crates/core/src/docker.rs line 1756 (args.extend(config.run_args.iter().cloned())) MUST be present and positioned AFTER Deacon flags (security opts, mounts, env, GPU) and BEFORE the image name argument
- IF runArgs is not in the correct position, MUST move it to after GPU flags and before the image push

### Step 3: Verify Compose mode ignores runArgs
- MUST Read crates/core/src/compose.rs method up_with_injection() (around lines 301-332)
- MUST verify that run_args is NOT passed to any compose command builder
- SHOULD add a tracing warn! log in the compose up path (in crates/deacon/src/commands/up/compose.rs) when config.run_args is non-empty, warning that runArgs is ignored in Compose mode
- MUST Read crates/core/src/compose.rs function warn_security_options_for_compose() (around lines 335-370) and add runArgs to the set of warnings if not already present

### Step 4: Add unit tests
- MUST add a test in crates/core/src/docker.rs (in the existing test module) that constructs a DevContainerConfig with run_args containing memory and cpu flags and verifies they appear in the generated docker create command after Deacon flags and before the image name
- MUST add a test verifying empty run_args produces no extra arguments

### Step 5: Build and lint
- MUST run cargo fmt --all and cargo clippy --all-targets -- -D warnings
- MUST run cargo test -p deacon-core

## Test Specification

#[test]
fn test_run_args_forwarded_to_docker_create() {
    let mut config = DevContainerConfig::default();
    config.image = Some("ubuntu:22.04".to_string());
    config.run_args = vec!["--memory=2g".to_string(), "--cpus=2".to_string()];
    let args = build_create_args(&config);
    let image_pos = args.iter().position(|a| a == "ubuntu:22.04").unwrap();
    let mem_pos = args.iter().position(|a| a == "--memory=2g").unwrap();
    assert!(mem_pos < image_pos);
}

#[test]
fn test_empty_run_args_no_extra_args() {
    let mut config = DevContainerConfig::default();
    config.image = Some("ubuntu:22.04".to_string());
    config.run_args = vec![];
    let args = build_create_args(&config);
    let image_pos = args.iter().position(|a| a == "ubuntu:22.04").unwrap();
    assert!(image_pos > 0);
}

## Verification

- cargo fmt --all -- --check
- cargo clippy --all-targets -- -D warnings 2>&1 | tail -3
- cargo test -p deacon-core 2>&1 | tail -3
