---
work-unit: compose-profile-selection
flight-plan: consumer-core-completion
sequence: 3
depends-on: []
parallel-group: alpha
---

## Task

Parse and forward --profile flags to docker compose up/down/exec commands, support multiple profiles, wire from CLI args through ComposeProject to ComposeCommand

## Acceptance Criteria

- Compose profiles can be specified via --compose-profile CLI flag and forwarded to docker compose commands [SC-007]
- Multiple profiles can be activated simultaneously via repeated --compose-profile flags [SC-008]
- docker compose down also receives profile flags from persisted state or CLI override [SC-009]
- Default behavior (no profiles) is unchanged - no --profile flags emitted [SC-010]
- ComposeState persists profiles for down command reconstruction [SC-009]
- All changes pass cargo clippy and cargo test [SC-019]

## File Scope

### Create


### Modify

- crates/core/src/compose.rs
- crates/deacon/src/commands/up/compose.rs
- crates/deacon/src/commands/up/args.rs
- crates/deacon/src/cli.rs
- crates/deacon/src/commands/down.rs

### Protect


## Procedure

### Step 1: Read current profile infrastructure
- MUST Read crates/core/src/compose.rs lines 25-41 (ComposeProject.profiles field)
- MUST Read crates/core/src/compose.rs lines 125-156 (ComposeCommand.with_profiles and build_command profile flag injection)
- MUST Read crates/core/src/compose.rs lines 660-666 (ComposeManager.get_command threading profiles)
- MUST Read crates/deacon/src/cli.rs lines 270-300 (Up CLI args area)
- MUST Read crates/deacon/src/commands/up/args.rs for UpArgs struct
- MUST Read crates/deacon/src/commands/up/compose.rs lines 40-130 (execute_compose_up)
- MUST Read crates/deacon/src/commands/down.rs lines 330-422 (execute_compose_down)
- MUST Read crates/core/src/state.rs lines 39-51 (ComposeState struct)

### Step 2: Add CLI flag for profiles
- MUST add --compose-profile flag to the Up variant in crates/deacon/src/cli.rs near existing compose flags. Use #[arg(long)] with type Vec<String>
- MUST add compose_profiles: Vec<String> field to UpArgs in crates/deacon/src/commands/up/args.rs
- MUST wire the CLI field to UpArgs in the From/mapping implementation

### Step 3: Wire profiles to ComposeProject
- MUST Edit crates/deacon/src/commands/up/compose.rs in execute_compose_up() to set project.profiles = args.compose_profiles.clone() after project creation around line 47-50

### Step 4: Persist profiles in ComposeState
- MUST add pub profiles: Vec<String> field to ComposeState struct in crates/core/src/state.rs around line 50
- MUST update all locations that create ComposeState to include the profiles field
- MUST update execute_compose_down() in crates/deacon/src/commands/down.rs line 360 to use profiles: compose_state.profiles.clone()

### Step 5: Add Down CLI flag
- SHOULD add --compose-profile flag to the Down variant in crates/deacon/src/cli.rs
- IF added, MUST wire through to execute_compose_down() and prefer CLI profiles over saved state

### Step 6: Add tests
- MUST verify existing test at compose.rs:1306-1330 covers multiple profiles
- MUST add a test that empty profiles produces no --profile flags
- MUST add a test for CLI arg parsing of --compose-profile dev --compose-profile debug

### Step 7: Build and lint
- MUST run cargo fmt --all and cargo clippy --all-targets -- -D warnings
- MUST run make test-nextest-fast

## Test Specification

#[test]
fn test_compose_profiles_forwarded() {
    let cmd = ComposeCommand::new(PathBuf::from("/tmp"), vec![PathBuf::from("docker-compose.yml")])
        .with_profiles(vec!["dev".to_string(), "debug".to_string()]);
    let built = cmd.build_command("up");
    let args: Vec<String> = built.get_args().map(|a| a.to_string_lossy().to_string()).collect();
    let count = args.iter().filter(|a| a.as_str() == "--profile").count();
    assert_eq!(count, 2);
}

#[test]
fn test_compose_no_profiles_no_flags() {
    let cmd = ComposeCommand::new(PathBuf::from("/tmp"), vec![PathBuf::from("docker-compose.yml")])
        .with_profiles(vec![]);
    let built = cmd.build_command("up");
    let args: Vec<String> = built.get_args().map(|a| a.to_string_lossy().to_string()).collect();
    assert!(!args.contains(&"--profile".to_string()));
}

## Verification

- cargo fmt --all -- --check
- cargo clippy --all-targets -- -D warnings 2>&1 | tail -3
- cargo test -p deacon-core -- compose 2>&1 | tail -3
- make test-nextest-fast 2>&1 | tail -5
