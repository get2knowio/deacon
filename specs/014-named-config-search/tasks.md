# Tasks: Named Config Folder Search

**Input**: Design documents from `/specs/014-named-config-search/`
**Prerequisites**: plan.md (required), spec.md (required), research.md, data-model.md, contracts/config-discovery.md

**Tests**: Included — the plan specifies ~15 new unit tests and existing tests require updating for the new return type.

**Organization**: Tasks are grouped by user story to enable independent implementation and testing. US1 and US2 (both P1) share an implementation phase because the same `discover_config()` rewrite serves both.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependencies)
- **[Story]**: Which user story this task belongs to (e.g., US1, US2, US3)
- Include exact file paths in descriptions

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: No setup needed — existing Rust workspace, no new dependencies per plan.md.

*(No tasks — project structure and dependencies already exist.)*

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: New data types and helpers that ALL user stories depend on. These are pure additions with no behavior changes to existing code.

**CRITICAL**: No user story work can begin until this phase is complete.

- [ ] T001 Add `ConfigError::MultipleConfigs { paths: Vec<String> }` error variant to `ConfigError` enum in `crates/core/src/errors.rs`. Use `#[error]` format per data-model.md: `"Multiple devcontainer configurations found. Use --config to specify one:\n{}"` with paths joined by newlines. Add a display test in the existing `test_config_error_display` test.
- [ ] T002 [P] Add `DiscoveryResult` enum (`Single(PathBuf)`, `Multiple(Vec<PathBuf>)`, `None(PathBuf)`) to `crates/core/src/config.rs` near the existing `ConfigLocation` struct (around line 204). Derive `Debug, Clone, PartialEq`. Add doc comments per data-model.md invariants.
- [ ] T003 [P] Add `fn check_config_file(dir: &Path) -> Option<PathBuf>` helper to `crates/core/src/config.rs` that checks for `devcontainer.json` then `devcontainer.jsonc` in a directory, returning the first found path. Per research.md D3: prefer `.json` over `.jsonc` when both exist.
- [ ] T004 Add `fn enumerate_named_configs(devcontainer_dir: &Path) -> Result<Vec<PathBuf>>` helper to `crates/core/src/config.rs`. Reads direct child directories of `.devcontainer/` using `std::fs::read_dir`, filters for directories only, checks each with `check_config_file()`, sorts results alphabetically by subdirectory name per FR-005/D4. Skips subdirs without config files per FR-006.

**Checkpoint**: Foundation ready — new types and helpers compiled, no existing behavior changed yet.

---

## Phase 3: User Story 1 + User Story 2 — Single Named Config Discovery & Backward Compatibility (Priority: P1) MVP

**Goal**: Rewrite `discover_config()` to search all three priority locations and return `DiscoveryResult`. Existing search locations continue to work identically (US2), and new named config folders are discovered (US1).

**Independent Test**: Create temp workspaces with various config layouts and verify `discover_config()` returns the correct `DiscoveryResult` variant for each.

### Implementation for US1 + US2

- [ ] T005 [US1] [US2] Rewrite `ConfigLoader::discover_config()` in `crates/core/src/config.rs:1382` to return `DiscoveryResult` instead of `ConfigLocation`. Implement three-tier search per contracts/config-discovery.md: (1) check `.devcontainer/` with `check_config_file()`, (2) check root `.devcontainer.json` then `.devcontainer.jsonc`, (3) enumerate named configs. Short-circuit on priority 1/2 match per FR-009. Update the function's doc comment and doctest to reflect the new return type.
- [ ] T006 [US1] [US2] Update the 5 existing `discover_config` unit tests in `crates/core/src/config.rs` (`test_discover_config_devcontainer_dir`, `test_discover_config_root_file`, `test_discover_config_preference_order`, `test_discover_config_no_file_exists`, `test_discover_config_workspace_not_exists`) to assert against `DiscoveryResult` variants instead of `ConfigLocation`.
- [ ] T007 [P] [US1] Add unit test `test_discover_config_single_named_config` in `crates/core/src/config.rs`: workspace with only `.devcontainer/python/devcontainer.json` → returns `DiscoveryResult::Single` pointing to that file.
- [ ] T008 [P] [US1] Add unit tests for `.jsonc` support in `crates/core/src/config.rs`: (a) `test_discover_config_jsonc_priority1` — `.devcontainer/devcontainer.jsonc` found at priority 1, (b) `test_discover_config_json_preferred_over_jsonc` — when both `.json` and `.jsonc` exist in same dir, `.json` wins, (c) `test_discover_config_jsonc_named` — named config with only `.jsonc` is discovered. Per research.md D2/D3.
- [ ] T009 [P] [US2] Add unit tests for short-circuit behavior in `crates/core/src/config.rs`: (a) `test_discover_config_priority1_overrides_named` — `.devcontainer/devcontainer.json` exists alongside `.devcontainer/python/devcontainer.json` → returns priority 1 path, (b) `test_discover_config_priority2_overrides_named` — `.devcontainer.json` exists alongside named configs → returns priority 2 path. Per FR-009.
- [ ] T010 [P] [US1] Add unit tests for edge cases in `crates/core/src/config.rs`: (a) `test_discover_config_skip_non_dir_entries` — files in `.devcontainer/` alongside named subdirs are ignored, (b) `test_discover_config_deep_nesting_ignored` — `.devcontainer/a/b/devcontainer.json` NOT found (one level only per FR-005), (c) `test_discover_config_empty_devcontainer_dir` — `.devcontainer/` exists but has no subdirs with configs → returns `None`, (d) `test_discover_config_subdir_without_config_skipped` — subdir exists but has no devcontainer.json → skipped per FR-006.

**Checkpoint**: `discover_config()` now supports all three search locations. Existing behavior preserved (US2). Single named configs auto-discovered (US1). All unit tests pass.

---

## Phase 4: User Story 3 — Multiple Named Configs Require Explicit Selection (Priority: P2)

**Goal**: All 4 call sites that invoke `discover_config()` handle `DiscoveryResult::Multiple` by returning a clear error listing available configs.

**Independent Test**: Create a workspace with `.devcontainer/python/devcontainer.json` and `.devcontainer/node/devcontainer.json`, invoke each call site without `--config`, verify `MultipleConfigs` error with both paths listed.

### Implementation for US3

- [ ] T011 [US3] Update shared `load_config()` in `crates/deacon/src/commands/shared/config_loader.rs:69-73` to match on `DiscoveryResult` instead of calling `.path().to_path_buf()`. Handle `Single` → use path, `Multiple` → return `ConfigError::MultipleConfigs` with workspace-relative display paths, `None` → existing fallback behavior. Import `DiscoveryResult` from core.
- [ ] T012 [P] [US3] Update `down` command in `crates/deacon/src/commands/down.rs:62-68` to match on `DiscoveryResult`. Handle `Single` → load from path, `Multiple` → return `ConfigError::MultipleConfigs`, `None` → preserve existing auto-discovery-from-state fallback per research.md D7.
- [ ] T013 [P] [US3] Update `read-configuration` command in `crates/deacon/src/commands/config.rs:144-150` to match on `DiscoveryResult`. Handle `Single` → load from path (existing), `Multiple` → return `ConfigError::MultipleConfigs`, `None` → return `ConfigError::NotFound` (existing).
- [ ] T014 [P] [US3] Update `outdated` command's `resolve_config_path()` in `crates/deacon/src/commands/outdated.rs:414-420` to match on `DiscoveryResult`. Handle `Single` → return location, `Multiple` → return `ConfigError::MultipleConfigs`, `None` → existing error behavior. Update return type from `ConfigLocation` to accommodate `DiscoveryResult`.
- [ ] T015 [US3] Add unit tests for multiple configs in `crates/core/src/config.rs`: (a) `test_discover_config_multiple_named_configs` — two+ named subdirs with configs → returns `DiscoveryResult::Multiple` with all paths, (b) `test_discover_config_multiple_sorted_alphabetically` — verify paths in `Multiple` are sorted by subdirectory name per FR-005, (c) `test_multiple_configs_error_display` — verify `ConfigError::MultipleConfigs` error message format matches contracts/config-discovery.md (each path on its own line, indented with two spaces).

**Checkpoint**: All 4 call sites (shared load_config, down, read-configuration, outdated) handle the multiple-configs case with a clear, actionable error.

---

## Phase 5: User Story 4 — Explicit Config Path Override (Priority: P2)

**Goal**: Verify `--config` flag bypasses all auto-discovery, working correctly with the new `DiscoveryResult` return type.

**Independent Test**: Provide `--config .devcontainer/rust/devcontainer.json` to a workspace with multiple named configs and verify it uses the specified config without error.

### Implementation for US4

- [ ] T016 [US4] Add unit tests for `--config` bypass in `crates/deacon/src/commands/shared/config_loader.rs` (existing test module): (a) verify `--config` path is used directly when provided, skipping `discover_config()` entirely, (b) verify `--config` to a specific named config works even when multiple named configs exist, (c) verify `--config` to non-existent file returns appropriate error. These tests validate FR-004 — no code change needed since the bypass already exists in the `if let Some(path)` branch.

**Checkpoint**: `--config` override works correctly with no regressions. US4 acceptance scenarios validated.

---

## Phase 6: Polish & Cross-Cutting Concerns

**Purpose**: Integration test updates, full validation, and cleanup.

- [ ] T017 Update integration tests that use `discover_config()` or `ConfigLocation` in `crates/core/tests/integration_variable_substitution.rs` (lines 14, 151) and `crates/core/tests/integration_worktree.rs` (line 192) to work with new `DiscoveryResult` return type. Update assertions from `ConfigLocation` field access to `DiscoveryResult` pattern matching.
- [ ] T018 [P] Add tracing debug/info logs for named config discovery steps in `crates/core/src/config.rs`: info-level log when named config enumeration finds configs, debug-level logs for each subdirectory examined. Per spec contracts/config-discovery.md tracing requirements.
- [ ] T019 Run `cargo fmt --all && cargo clippy --all-targets -- -D warnings && make test-nextest-fast` to verify zero warnings, correct formatting, and all tests pass.
- [ ] T020 Run `make test-nextest` for full validation before PR — ensure no regressions in docker, smoke, or integration test suites.

---

## Dependencies & Execution Order

### Phase Dependencies

- **Foundational (Phase 2)**: No dependencies — can start immediately. BLOCKS all user stories.
- **US1 + US2 (Phase 3)**: Depends on Phase 2 completion (needs DiscoveryResult, check_config_file, enumerate_named_configs)
- **US3 (Phase 4)**: Depends on Phase 3 completion (needs discover_config() returning DiscoveryResult)
- **US4 (Phase 5)**: Depends on Phase 4 completion (tests validate --config bypass works with updated call sites)
- **Polish (Phase 6)**: Depends on all user story phases complete

### User Story Dependencies

- **US1 + US2 (P1)**: Can start after Foundational (Phase 2) — core implementation
- **US3 (P2)**: Depends on US1+US2 — caller updates require the new return type to exist
- **US4 (P2)**: Depends on US3 — tests need updated call sites to verify override works end-to-end

### Within Each Phase

- Foundational: T001, T002, T003 can run in parallel; T004 depends on T003
- US1+US2: T005 first (rewrites function), then T006 (updates existing tests), then T007-T010 in parallel (new tests)
- US3: T011 first, then T012-T014 in parallel, then T015 (tests)
- US4: T016 standalone
- Polish: T017, T018 in parallel, then T019, then T020

### Critical Path

```
T001 ──┐
T002 ──┤
T003 ──┼──→ T004 ──→ T005 ──→ T006 ──→ T011 ──→ T015 ──→ T016 ──→ T017 ──→ T019 ──→ T020
       │                  ├──→ T007    ├──→ T012            ├──→ T018
       │                  ├──→ T008    ├──→ T013
       │                  ├──→ T009    └──→ T014
       │                  └──→ T010
```

### Parallel Opportunities

**Phase 2** (3 parallel): T001, T002, T003 can run simultaneously (different sections of different files)

**Phase 3** (4 parallel): T007, T008, T009, T010 are independent test additions after T005/T006

**Phase 4** (3 parallel): T012, T013, T014 update different command files independently after T011

**Phase 6** (2 parallel): T017, T018 touch different files

---

## Parallel Example: Phase 2 (Foundational)

```
# These three tasks touch independent code sections:
Task T001: "Add MultipleConfigs variant in crates/core/src/errors.rs"
Task T002: "Add DiscoveryResult enum in crates/core/src/config.rs"
Task T003: "Add check_config_file() helper in crates/core/src/config.rs"
# Then sequentially:
Task T004: "Add enumerate_named_configs() helper (uses check_config_file from T003)"
```

## Parallel Example: Phase 4 (US3 — Caller Updates)

```
# After T011 (shared load_config), these three are independent files:
Task T012: "Update down.rs"
Task T013: "Update config.rs (read-configuration)"
Task T014: "Update outdated.rs"
```

---

## Implementation Strategy

### MVP First (US1 + US2 Only)

1. Complete Phase 2: Foundational types and helpers
2. Complete Phase 3: Rewrite discover_config() + all tests
3. **STOP and VALIDATE**: `make test-nextest-fast` — single named config works, existing behavior preserved
4. At this point, named config discovery works but callers haven't been updated for `Multiple` case

### Incremental Delivery

1. Phase 2 → Foundation ready
2. Phase 3 (US1 + US2) → Core discovery works → `make test-nextest-fast` (MVP!)
3. Phase 4 (US3) → All callers handle multiple configs → `make test-nextest-fast`
4. Phase 5 (US4) → Override validated → `make test-nextest-fast`
5. Phase 6 → Polish, integration tests, full validation → `make test-nextest`

### Call Site Coverage (FR-007)

The 4 `discover_config()` call sites cover all commands specified in FR-007:
- **shared `load_config()`** → `up`, `exec`, `build`, `run-user-commands`
- **`down.rs`** → `down`
- **`config.rs`** → `read-configuration`
- **`outdated.rs`** → `outdated` (bonus — not in FR-007 but discovered during research)

---

## Notes

- [P] tasks = different files, no dependencies
- [Story] label maps task to specific user story for traceability
- Plan identified 2 call sites; codebase grep found 4 — tasks cover all 4
- No new dependencies needed (serde, tracing, thiserror, clap all existing)
- All new tests are unit tests — no nextest group config changes needed
- Research.md D2 broadened scope: `.jsonc` support added to ALL priority levels, not just priority 3
