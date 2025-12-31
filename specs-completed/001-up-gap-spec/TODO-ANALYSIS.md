# TODO Analysis - Up Gap Spec Implementation

**Generated**: 2025-11-20  
**Spec**: specs/001-up-gap-spec  
**Status**: Phase 5 incomplete, multiple TODOs across all phases

---

## Executive Summary

**Critical Finding**: While tasks.md showed all tasks marked complete [X], **15 integration tests are disabled** and **significant functionality is not implemented**.

**Updated Status**:
- **Phase 1-4 (Setup, Foundational, US1, US2)**: Mostly complete but with gaps
- **Phase 5 (US3)**: Largely incomplete - 8 tasks marked incomplete
- **Total Incomplete Tasks**: 8 out of 31 tasks (26% incomplete)
- **Disabled Tests**: 15+ integration tests marked `#[ignore]`

---

## Phase 2 (US2) - Incomplete Tasks

### T015: Dotfiles Integration ❌
**Status**: Partially implemented - host-side only
**Location**: `crates/deacon/src/commands/up.rs`

**TODOs Found**:
- Line 2063: `// TODO T015: Re-enable when container-side dotfiles installation is implemented`
- Line 2117: `// TODO T015: Implement container-side dotfiles installation`
- Line 2114: Placeholder message about incomplete implementation

**Missing**:
- Container-side dotfiles installation
- Full idempotency guarantees
- Proper lifecycle integration

**Tests**: T013 test exists but may not cover container-side installation

---

### T016: Feature-Driven Image Extension ❌
**Status**: Foundation in place, core logic missing
**Location**: `crates/deacon/src/commands/up.rs:1444-1464`

**TODOs Found**:
- Line 1461: `// TODO T016: Implement feature installation before docker.up()`

**Missing**:
1. Check if config.features is non-empty
2. Build extended image with features using BuildKit
3. Apply cache options (cache_from, cache_to) to build process
4. Update config.image to use extended image
5. Merge feature metadata into configuration

**Impact**: Features are merged into config but never actually installed

**Related**: build/mod.rs line 1291 also has TODO for feature application

---

### T017: UID Update & Security Options ❌
**Status**: Partially implemented
**Location**: `crates/deacon/src/commands/up.rs:1529-1550`

**TODOs Found**:
- Line 1544: `// TODO T017: Complete UID update flow and security options application`
- Line 1850: `// TODO: Implement user mapping application using UserMappingService`

**Missing**:
1. UID update: usermod/groupmod commands in container
2. Security options: privileged, capAdd, securityOpt not wired to docker run/create
3. Init process: config.init not applied
4. Entrypoint override: security-related overrides not handled

**Tests**: T012 test exists but may not fully validate all aspects

---

## Phase 3 (US1) - Incomplete Tasks

### T029: ID Label Discovery & Image Metadata Merge ❌
**Status**: Stub/placeholder implementation
**Location**: `crates/deacon/src/commands/up.rs`

**TODOs Found**:
- Line 914: `// TODO: Implement actual disallowed features list`
- Line 951: `#[allow(dead_code)] // TODO: Wire into execute_up_with_runtime for automatic label discovery`
- Line 1010: `// TODO: Implement full image metadata merge`

**Missing**:
1. Actual disallowed features list (currently empty)
2. Automatic id-label discovery from container
3. Full image metadata merge into configuration
4. Wire discovery function into main execution flow

**Tests Disabled**:
- `up_config_resolution.rs` - 2 tests disabled waiting for T029

---

## Phase 5 (US3) - Incomplete Tasks

### T018: Compose Profiles Tests ✅ (Tests exist but all disabled)
**Status**: Test file created, no implementation
**Location**: `crates/deacon/tests/up_compose_profiles.rs`

**Tests Disabled** (7 total):
1. `test_compose_mount_conversion_bind_to_volume` - Line 20
2. `test_compose_profile_selection` - Line 70
3. `test_compose_project_name_from_env_file` - Line 132
4. `test_compose_mount_conversion_with_fixture` - Line 186
5. `test_compose_external_volume_conversion` - Line 213
6. `test_compose_multiple_profiles_per_service` - Line 258
7. `test_compose_project_name_fallback_without_env` - Line 303

**All waiting for**: T020 implementation

---

### T019: Reconnect Tests ✅ (Tests exist but all disabled)
**Status**: Test file created, no implementation
**Location**: `crates/deacon/tests/up_reconnect.rs`

**Tests Disabled** (8 total):
1. `test_expect_existing_fails_fast_with_id_label` - Line 20 (T023)
2. `test_remote_env_redaction_in_logs` - Line 60 (T021)
3. `test_secrets_file_redaction` - Line 110 (T021)
4. `test_expect_existing_compose_with_id_labels` - Line 170 (T023)
5. `test_multiple_secrets_files_merge_and_redaction` - Line 216 (T021)
6. `test_secrets_never_in_json_output` - Line 268 (T021)
7. `test_expect_existing_with_remove_existing_conflict` - Line 312 (T023)
8. `test_expect_existing_with_id_label_discovery` - Line 342 (T023, T029)

**Waiting for**: T021 (secrets), T023 (expect-existing), T029 (id-label)

---

### T020: Compose Mount Conversion & Profiles ❌
**Status**: Basic compose exists, advanced features missing
**Location**: `crates/deacon/src/commands/up.rs:1161+` (execute_compose_up)

**Missing**:
1. Mount-to-volume conversion for additional CLI mounts
2. Profile selection and application from compose files
3. .env file parsing for COMPOSE_PROJECT_NAME
4. External volume handling (external=true flag)
5. Multi-profile per service support

**Current State**:
- Basic compose project start/stop works
- No mount conversion logic found
- No profile detection code
- No .env parsing for project name

**Tests**: All 7 tests disabled

---

### T021: Remote Env & Secrets Redaction ❌
**Status**: Data structures exist, no implementation
**Location**: `crates/deacon/src/commands/up.rs`

**Found**:
- Line 377-406: `NormalizedRemoteEnv` struct and parser ✅
- Line 472: `remote_env` field in NormalizedUpInput ✅
- Line 492: `secrets_files` field in NormalizedUpInput ✅
- Line 497: `omit_config_remote_env_from_metadata` flag ✅

**Missing**:
1. Secrets file loading (read files from secrets_files paths)
2. Parse secrets files (KEY=value format)
3. Redaction mechanism for logs (integrate with RedactionConfig/SecretRegistry)
4. Merge remote-env and secrets into runtime environment
5. Ensure secrets never appear in JSON output
6. Support multiple secrets files

**Tests**: 5 tests disabled waiting for implementation

---

### T022: Docker/Compose Path Overrides ✅ (Appears complete)
**Status**: Likely complete, needs verification
**Location**: CLI and compose integration

**Found**:
- `docker_path` and `compose_path` CLI flags exist
- ComposeManager accepts custom docker_path
- BuildKit/cache flags exist

**No TODOs found for this task** - may be complete

---

### T023: Expect-Existing Fast-Fail Logic ❌
**Status**: Flag exists, no validation logic
**Location**: `crates/deacon/src/commands/up.rs`

**Found**:
- Line 464: `expect_existing_container` field ✅
- Line 662: Default value (false) ✅

**Missing**:
1. Fast-fail check before any docker operations
2. Container lookup by id-labels
3. Error JSON output when container not found
4. Conflict detection (expect + remove flags)
5. Compose flow support for expect-existing

**Tests**: 4 tests disabled waiting for implementation

---

### T030: User-Data/Container-Session Folders ❌
**Status**: Unknown/not found
**Location**: Unknown

**No clear implementation found**

**Missing**:
1. User-data folder creation/usage
2. Container-session folder handling
3. Probe caching hooks
4. Compose fixture validation

**Needs**: Code search and investigation to determine current state

---

## Other Notable TODOs (Cross-Cutting)

### High Priority

#### CLI Output Flags Not Wired
**Location**: `crates/deacon/src/cli.rs:1142`, `crates/deacon/src/commands/up.rs:139-217`

**TODOs**:
- Line 139: `includeConfiguration` flag structure defined but not wired
- Line 148: `includeMergedConfiguration` flag structure defined but not wired
- Line 1142 (cli.rs): `// TODO: Add configuration and merged_configuration if flags are set`

**Impact**: `--include-configuration` and `--include-merged-configuration` flags accepted but don't add data to JSON output

---

#### Advanced Validation Not Wired
**Location**: `crates/deacon/src/commands/up.rs:542-614`

Multiple validation functions defined but marked as `#[allow(dead_code)]`:
- Line 542: Advanced validation for T011
- Line 587: Additional validation for T009
- Line 612-614: More T009 validation functions

**Status**: Defined but never called

---

### Medium Priority

#### Container Selection for run-user-commands
**Location**: `crates/deacon/src/commands/run_user_commands.rs`

**TODOs**:
- Line 31: `TODO(#269): Implement container selection`
- Line 35: `TODO(#269): Implement container selection`

**Status**: Tracked in GitHub issue #269

---

#### Workspace-Based Container Resolution
**Location**: `crates/core/src/container.rs`

**TODOs**:
- Line 923: `TODO: implement when workspace labels defined - see issue #270`
- Line 928: `TODO(#270) - When implemented, will query containers with workspace-specific labels`
- Line 993: `TODO(#270): Implement workspace-based container resolution`

**Status**: Tracked in GitHub issue #270

---

#### Container-Based Config Reading
**Location**: `crates/deacon/src/commands/read_configuration.rs`

**TODOs**:
- Line 32: `TODO(#268): Implement container-based config reading`

**Status**: Tracked in GitHub issue #268

---

#### OCI Credential Management
**Locations**: Multiple files

**TODOs**:
- `crates/deacon/src/commands/templates.rs:171` - Set credentials in OCI client
- `crates/deacon/src/commands/templates.rs:175` - Read password from stdin
- `crates/deacon/src/commands/features.rs:1950` - Set credentials in OCI client
- `crates/deacon/src/commands/features.rs:1954` - Read password from stdin

**Status**: Authentication flow needs completion

---

### Low Priority (Future Enhancements)

#### Lifecycle Timeout Enforcement
**Location**: `crates/core/src/lifecycle.rs:143`
```rust
timeout: None, // TODO: Implement timeout enforcement
```

#### TTY Spinner Output
**Location**: `crates/core/src/progress.rs:1542`
```rust
// TODO: Add TTY spinner/text output
```

#### Command Output Capture
**Location**: `crates/core/src/container_lifecycle.rs:691-692`
```rust
stdout: String::new(), // TODO: Capture output when docker exec supports it
stderr: String::new(), // TODO: Capture output when docker exec supports it
```

#### Feature Install Logs
**Location**: `crates/core/src/feature_installer.rs:324`
```rust
logs: String::new(), // TODO: Capture logs from exec
```

#### OCI Upload Flow Mocking
**Location**: `crates/core/tests/integration_oci_enhancements.rs:439`
```rust
#[ignore = "Requires full upload flow mocking - TODO: implement mock responses"]
```

---

## Summary Statistics

### By Phase
- **Phase 1 (Setup)**: ✅ Complete
- **Phase 2 (US2 - Foundational)**: ⚠️ 3/4 incomplete (T015, T016, T017)
- **Phase 3 (US1 - MVP)**: ⚠️ 1/7 incomplete (T029)
- **Phase 5 (US3 - Compose)**: ❌ 4/5 incomplete (T020, T021, T023, T030)
- **Phase 6 (Polish)**: ✅ Complete

### By Category
- **Total Tasks**: 31
- **Marked Complete**: 23 (74%)
- **Actually Incomplete**: 8 (26%)
- **Disabled Tests**: 15+ integration tests
- **TODO Comments**: 60+ across codebase

### By Priority
- **Critical (Blocking)**: 8 tasks (T015, T016, T017, T020, T021, T023, T029, T030)
- **High (Feature gaps)**: 3 areas (CLI output flags, validation wiring, metadata merge)
- **Medium (Tracked issues)**: 3 GitHub issues (#268, #269, #270)
- **Low (Future)**: 5+ enhancements (timeouts, output capture, TTY UI)

---

## Recommended Action Plan

### Immediate (Critical Path)

1. **T029** - ID label discovery & disallowed features (blocks US1 completion)
   - Implement actual disallowed features list
   - Wire id-label discovery into main flow
   - Complete image metadata merge
   - Enable 2 disabled tests

2. **T021** - Secrets & remote-env redaction (blocks US3 completion)
   - Implement secrets file loading/parsing
   - Integrate redaction with SecretRegistry
   - Merge into runtime environment
   - Enable 5 disabled tests

3. **T023** - Expect-existing validation (blocks US3 completion)
   - Implement fast-fail container lookup
   - Add id-label matching logic
   - Validate flag conflicts
   - Enable 4 disabled tests

### Phase 2 Completion

4. **T015** - Dotfiles container-side installation
   - Implement container exec-based dotfiles setup
   - Add idempotency checks
   - Verify T013 test coverage

5. **T016** - Feature installation in up flow
   - Wire feature build before docker.up()
   - Apply BuildKit/cache options
   - Merge feature metadata
   - Test with fixtures

6. **T017** - Complete UID/security options
   - Implement usermod/groupmod in container
   - Wire security options to docker create
   - Apply init process handling
   - Verify T012 test coverage

### Phase 3 Completion

7. **T020** - Compose mount conversion & profiles
   - Implement mount-to-volume conversion
   - Add profile detection and application
   - Parse .env for COMPOSE_PROJECT_NAME
   - Enable 7 disabled tests

8. **T030** - User-data/session folders
   - Investigate current state
   - Implement if missing
   - Add compose fixture validation

### Polish

9. **Wire CLI output flags**
   - Add includeConfiguration to JSON output
   - Add includeMergedConfiguration to JSON output
   - Enable related tests

10. **Enable validation functions**
    - Wire T009 validation helpers
    - Wire T011 error scenario functions
    - Remove `#[allow(dead_code)]` attributes

---

## Test Enablement Checklist

- [ ] Enable 7 tests in `up_compose_profiles.rs` (after T020)
- [ ] Enable 8 tests in `up_reconnect.rs` (after T021, T023, T029)
- [ ] Enable 2 tests in `up_config_resolution.rs` (after T029)
- [ ] Enable mount/remote-env tests in `up_json_output.rs` (after T007 verification)
- [ ] Verify all 15+ disabled tests pass
- [ ] Run full test suite: `make release-check`

---

## Notes

1. **Test-Driven Development**: Tests were written first (TDD approach) but implementations weren't completed
2. **False Completion**: Tasks marked [X] based on test file creation, not actual functionality
3. **Technical Debt**: ~60 TODO comments across codebase indicate deferred work
4. **Foundation Solid**: Data structures and CLI parsing largely complete; execution logic missing
5. **Incremental Path**: Can complete in priority order (T029 → T021 → T023 → others)

**Last Updated**: 2025-11-20
