# Tasks: Remove Feature Authoring Commands

**Input**: Design documents from `/specs/010-remove-feature-authoring/`
**Prerequisites**: plan.md (required), spec.md (required for user stories), research.md, data-model.md, contracts/

**Tests**: Not explicitly requested. Verification tasks confirm retained tests pass; no new tests are written.

**Organization**: Tasks are grouped by user story to enable independent implementation and testing of each story.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependencies)
- **[Story]**: Which user story this task belongs to (e.g., US1, US2, US3)
- Include exact file paths in descriptions

## Path Conventions

- **Workspace root**: Rust workspace with `crates/deacon/` (binary) + `crates/core/` (library)
- **CLI source**: `crates/deacon/src/`
- **Core library**: `crates/core/src/`
- **Tests**: `crates/deacon/tests/`, `crates/core/tests/`
- **Docs**: `docs/`, `examples/`
- **Config**: `.config/nextest.toml`, `Cargo.toml`

## Phase 1: Setup

**Purpose**: Verify the build is green before making any changes

- [X] T001 Verify current build is green by running `cargo fmt --all -- --check && cargo clippy --all-targets -- -D warnings && make test-nextest-fast`

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: N/A — This is a pure removal feature. There are no shared infrastructure changes that block all user stories. The user stories proceed in priority order.

**Note**: No foundational tasks. Proceed directly to User Story 1.

---

## Phase 3: User Story 1 — CLI No Longer Offers Authoring Commands (Priority: P1) :dart: MVP

**Goal**: Remove all six `features` subcommands (the entire `features` group), three `templates` authoring subcommands (`publish`, `metadata`, `generate-docs`), and all supporting implementation code. The CLI presents a clean, consumer-only command surface.

**Independent Test**: Run `deacon --help` and verify the `features` group does not appear. Run `deacon templates --help` and verify only `pull` and `apply` appear. Attempt each removed command and confirm unrecognized command errors.

### Implementation for User Story 1

> **Execution order matters**: CLI registration changes (T002-T003) must precede module declaration removal (T004), which must precede file deletion (T005-T007). Core library changes (T009) must precede core file deletion (T010-T011). Templates simplification (T008) depends on T003. Build verification (T012) is the final gate.

- [X] T002 [US1] Remove `FeatureCommands` enum, `Commands::Features` variant, and features dispatch block from `crates/deacon/src/cli.rs`
- [X] T003 [US1] Remove authoring variants (`Publish`, `Metadata`, `GenerateDocs`) from `TemplateCommands` enum and their dispatch logic from `crates/deacon/src/cli.rs`
- [X] T004 [US1] Remove `pub mod features`, `pub mod features_monolith`, `pub mod features_publish_output` declarations from `crates/deacon/src/commands/mod.rs`
- [X] T005 [P] [US1] Delete `crates/deacon/src/commands/features/` directory (mod.rs, plan.rs, package.rs, publish.rs, test.rs, shared.rs, unit_features_package.rs)
- [X] T006 [P] [US1] Delete `crates/deacon/src/commands/features_monolith.rs` (~2,800 lines)
- [X] T007 [P] [US1] Delete `crates/deacon/src/commands/features_publish_output.rs`
- [X] T008 [US1] Remove authoring functions (`execute_templates_metadata`, `execute_templates_publish`, `execute_templates_generate_docs`, `create_template_package`, `generate_readme_fragment`, `output_result`), simplify dispatcher, and clean up unused imports in `crates/deacon/src/commands/templates.rs`
- [X] T009 [US1] Remove `pub mod features_info;` and `pub mod features_test;` declarations from `crates/core/src/lib.rs`
- [X] T010 [P] [US1] Delete `crates/core/src/features_info.rs`
- [X] T011 [P] [US1] Delete `crates/core/src/features_test/` directory (mod.rs, discovery.rs, errors.rs, model.rs, runner.rs)
- [X] T012 [US1] Verify build compiles with `cargo build --all-features` and zero errors after all removals

**Checkpoint**: At this point, the `features` subcommand group is gone, `templates` only has `pull`/`apply`, and the project compiles. User Story 1 is functionally complete.

---

## Phase 4: User Story 2 — Feature Installation During Container Startup Remains Functional (Priority: P1)

**Goal**: Confirm that the removal of authoring commands has zero impact on consumer-side feature installation during `deacon up` and on `templates pull`/`templates apply`.

**Independent Test**: Run `cargo nextest run` targeting consumer feature installation tests and verify they all pass. Verify preserved core modules are unchanged.

### Implementation for User Story 2

- [X] T013 [US2] Verify consumer core modules are preserved and unchanged: `crates/core/src/features.rs`, `crates/core/src/feature_installer.rs`, `crates/core/src/feature_ref.rs`, `crates/core/src/oci/` (entire directory)
- [X] T014 [US2] Run consumer feature installation tests (`integration_feature_dependencies`, `integration_feature_installation`, `integration_features`, `integration_parallel_feature_installation`) and verify all pass
- [X] T015 [US2] Verify `templates pull` and `templates apply` functions in `crates/deacon/src/commands/templates.rs` compile and their retained tests in `crates/deacon/tests/test_templates_cli.rs` pass

**Checkpoint**: Consumer functionality verified — feature installation and template consumer commands work exactly as before.

---

## Phase 5: User Story 3 — Dead Code Fully Removed (Priority: P2)

**Goal**: Remove all orphaned test files, spec documentation, example directories, and configuration references for removed commands. The codebase compiles with zero warnings and passes all tests.

**Independent Test**: Run `cargo clippy --all-targets -- -D warnings` and verify zero warnings. Run `make test-nextest-fast` and verify all tests pass. Search for orphaned references to removed modules.

### Implementation for User Story 3

#### Test File Cleanup

- [X] T016 [P] [US3] Delete 12 authoring test files from `crates/deacon/tests/`: `test_features_cli.rs`, `cli_flags_features_info.rs`, `integration_features_info_auth.rs`, `integration_features_info_dependencies.rs`, `integration_features_info_local.rs`, `integration_features_info_manifest.rs`, `integration_features_info_tags.rs`, `integration_features_info_verbose.rs`, `integration_features_package.rs`, `integration_features_publish.rs`, `integration_features_test_json.rs`, `unit_features_package.rs`
- [X] T017 [P] [US3] Delete 4 authoring test files from `crates/core/tests/`: `features_info_models.rs`, `features_test_discovery.rs`, `features_test_paths.rs`, `features_test_scenarios.rs`
- [X] T018 [US3] Remove authoring tests (publish, metadata, generate-docs) from `crates/deacon/tests/test_templates_cli.rs`, keeping only pull and apply tests
- [X] T019 [US3] Remove deleted test binary references from `.config/nextest.toml` across all profiles (default, dev-fast, full, ci, docker) per research.md Decision 10

#### Spec Documentation Cleanup

- [X] T020 [P] [US3] Delete 5 spec documentation directories from `docs/subcommand-specs/completed-specs/`: `features-info/`, `features-package/`, `features-plan/`, `features-publish/`, `features-test/`

#### Example Directory Cleanup

- [X] T021 [P] [US3] Delete 6 feature authoring example directories from `examples/`: `feature-management/`, `feature-package/`, `feature-plan/`, `feature-publish/`, `features-info/`, `features-test/`
- [X] T022 [P] [US3] Delete `examples/template-management/metadata-and-docs/` directory (templates authoring example)
- [X] T023 [P] [US3] Delete `examples/registry/dry-run-publish/` directory (authoring publish dry-run example)

#### Documentation Updates

- [X] T024 [US3] Update `README.md` to remove feature authoring references: Feature Management examples, Features Test/Info command examples, `features plan --json` output example in Output Streams section, and spec roadmap entries for features-test/package/publish/info/plan
- [X] T025 [P] [US3] Update `docs/CLI_PARITY.md` to remove all references to removed commands (features info, features publish, templates publish, templates metadata, templates generate-docs)
- [X] T026 [P] [US3] Update `examples/README.md` to remove authoring example references from index and quick start sections
- [X] T027 [US3] Review and update `docs/ARCHITECTURE.md` to remove or update stale references to removed features and templates authoring commands

**Checkpoint**: All dead code, orphaned tests, stale documentation, and unused configuration removed. Codebase compiles cleanly.

---

## Phase 6: User Story 4 — License Alignment (Priority: P2)

**Goal**: The Cargo.toml license field reads `MIT`, matching the LICENSE file and README badge.

**Independent Test**: Run `grep 'license' Cargo.toml` and verify it reads `MIT`.

### Implementation for User Story 4

- [X] T028 [US4] Change `license = "Apache-2.0"` to `license = "MIT"` in workspace root `Cargo.toml` (line 10) per research.md Decision 12

**Checkpoint**: License metadata aligned across LICENSE file, README badge, and Cargo.toml.

---

## Phase 7: Polish & Cross-Cutting Concerns

**Purpose**: Final verification that all changes are consistent and the build is fully green.

- [X] T029 Run `cargo fmt --all && cargo clippy --all-targets -- -D warnings` to verify formatting and zero lint warnings
- [X] T030 Run `make test-nextest-fast` to verify all retained tests pass with no failures
- [X] T031 Search for orphaned references to removed modules (`features_info`, `features_test`, `features_monolith`, `features_publish_output`, `FeatureCommands`) across `crates/` and `docs/`
- [X] T032 Verify `cargo run -- --help` does not list the `features` subcommand group and `cargo run -- templates --help` shows only `pull` and `apply`

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (Phase 1)**: No dependencies — verify green build before starting
- **US1 (Phase 3)**: Depends on Setup — core compilation-breaking removal
- **US2 (Phase 4)**: Depends on US1 — verifies consumer paths survive removal
- **US3 (Phase 5)**: Depends on US1 — cleanup of dead tests/docs/examples
- **US4 (Phase 6)**: No dependency on other user stories — can run in parallel with US3
- **Polish (Phase 7)**: Depends on ALL user stories being complete

### User Story Dependencies

- **User Story 1 (P1)**: Can start after Setup — No dependencies on other stories. **This is the MVP.**
- **User Story 2 (P1)**: Depends on US1 — Verification checkpoint that consumer paths work post-removal
- **User Story 3 (P2)**: Depends on US1 — Dead code cleanup cannot start until the code making it "alive" is removed
- **User Story 4 (P2)**: Independent — Can start after Setup (or in parallel with US3)

### Within Each User Story

**US1 (strict execution order)**:
1. T002 + T003 (cli.rs changes — remove enum variants and dispatch)
2. T004 (commands/mod.rs — remove module declarations)
3. T005 + T006 + T007 (delete command implementation files — parallel)
4. T008 (templates.rs simplification)
5. T009 (core lib.rs — remove module declarations)
6. T010 + T011 (delete core authoring modules — parallel)
7. T012 (build verification)

**US3 (flexible order)**:
- T016 + T017 (delete test files — parallel, any order)
- T018 (modify test_templates_cli.rs — independent)
- T019 (nextest config — after T016/T017 to know which binaries were deleted)
- T020 + T021 + T022 + T023 (delete doc/example dirs — all parallel)
- T024 + T025 + T026 + T027 (update docs — T024/T027 sequential, T025/T026 parallel)

### Parallel Opportunities

- **Within US1**: T005 + T006 + T007 (delete feature command files); T010 + T011 (delete core modules)
- **Within US3**: T016 + T017 (delete test files); T020-T023 (delete doc/example dirs); T025 + T026 (update CLI_PARITY + examples/README)
- **Across stories**: US3 and US4 can execute in parallel once US1 completes

---

## Parallel Example: User Story 1

```bash
# After T002-T004 complete (cli.rs and mod.rs changes):
# Launch file deletions in parallel:
Task: "Delete crates/deacon/src/commands/features/ directory"
Task: "Delete crates/deacon/src/commands/features_monolith.rs"
Task: "Delete crates/deacon/src/commands/features_publish_output.rs"

# After T009 completes (core lib.rs changes):
# Launch core module deletions in parallel:
Task: "Delete crates/core/src/features_info.rs"
Task: "Delete crates/core/src/features_test/ directory"
```

## Parallel Example: User Story 3

```bash
# All test file deletions in parallel:
Task: "Delete 12 authoring test files from crates/deacon/tests/"
Task: "Delete 4 authoring test files from crates/core/tests/"

# All doc/example directory deletions in parallel:
Task: "Delete 5 spec documentation directories"
Task: "Delete 6 feature authoring example directories"
Task: "Delete examples/template-management/metadata-and-docs/"
Task: "Delete examples/registry/dry-run-publish/"

# Documentation updates in parallel:
Task: "Update docs/CLI_PARITY.md"
Task: "Update examples/README.md"
```

---

## Implementation Strategy

### MVP First (User Story 1 Only)

1. Complete Phase 1: Setup (verify green build)
2. Complete Phase 3: User Story 1 (core command removal)
3. Complete Phase 4: User Story 2 (verify consumer functionality)
4. **STOP and VALIDATE**: Build compiles, consumer tests pass, CLI help shows no authoring commands
5. This is a deployable state — authoring commands are gone, consumer paths work

### Incremental Delivery

1. Setup → Verify green build
2. Add US1 → Core removal, build compiles → Verify consumer paths (US2) → **MVP!**
3. Add US3 → Dead code cleanup → Build still green, all tests pass
4. Add US4 → License aligned → Metadata consistent
5. Polish → Final verification sweep → Ready for PR

### Parallel Team Strategy

With multiple developers:

1. Team verifies Setup together (green build)
2. Developer A: User Story 1 (core removal — must complete first)
3. Once US1 done:
   - Developer A: User Story 2 (consumer verification)
   - Developer B: User Story 3 (dead code cleanup)
   - Developer C: User Story 4 (license fix)
4. Polish phase after all stories complete

---

## Notes

- [P] tasks = different files, no dependencies
- [Story] label maps task to specific user story for traceability
- This is a **removal feature** — no new code is written, only deleted or simplified
- US1 has strict internal ordering due to Rust compilation dependencies
- US3 tasks are mostly parallel (deleting independent files/directories)
- US4 is a single-task story (metadata fix)
- Commit after each phase or logical group for easy revert if needed
- Stop at any checkpoint to validate independently
