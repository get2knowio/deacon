# Implementation Plan: Remove Feature Authoring Commands

**Branch**: `010-remove-feature-authoring` | **Date**: 2026-02-19 | **Spec**: [spec.md](spec.md)
**Input**: Feature specification from `/specs/010-remove-feature-authoring/spec.md`

## Summary

Remove all DevContainer Feature authoring commands from Deacon CLI, narrowing scope to consumer-only commands. This involves removing the entire `features` subcommand group (6 subcommands), 3 `templates` authoring subcommands, associated core library modules, tests, spec documentation, examples, and fixing the Cargo.toml license metadata. Consumer functionality (feature installation during `deacon up`, `templates pull`, `templates apply`) must remain fully intact.

## Technical Context

**Language/Version**: Rust 1.70+ (Edition 2021)
**Primary Dependencies**: clap (CLI), serde (serialization), tokio (async), reqwest (HTTP/OCI), tracing (logging)
**Storage**: N/A
**Testing**: cargo-nextest with test groups (`.config/nextest.toml`)
**Target Platform**: Linux (primary), macOS, Windows
**Project Type**: Rust workspace (deacon binary + core library)
**Performance Goals**: N/A — removal feature, no new functionality
**Constraints**: Consumer feature installation and templates pull/apply must not regress
**Scale/Scope**: ~50+ files affected (delete ~35, modify ~15); ~5,000+ lines of code removed

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

| Principle | Status | Notes |
|-----------|--------|-------|
| I. Spec-Parity | PASS | Removal reduces scope; no spec-defined behavior is being added or changed. Consumer paths preserved. |
| II. Keep Build Green | PASS | Removal must maintain zero warnings, zero test failures. Verified via `cargo clippy --all-targets -- -D warnings` + `make test-nextest-fast`. |
| III. No Silent Fallbacks | PASS | N/A — no new fallback behavior introduced. |
| IV. Idiomatic, Safe Rust | PASS | N/A — no new Rust code written. Existing consumer code untouched. |
| V. Observability & Output | PASS | CLI help output updated to reflect reduced command set. No JSON contract changes for retained commands. |
| VI. Testing Completeness | PASS | All tests for removed commands are deleted. All tests for retained commands must pass. |
| VII. Subcommand Consistency | PASS | Templates module simplified; consumer commands retain shared abstractions. |
| VIII. Executable Examples | PASS | Authoring examples deleted. Consumer examples preserved. README updated. |

**Post-Design Re-Check**: All gates remain PASS. No new complexity introduced. The removal is straightforward surgical deletion with reference cleanup.

## Project Structure

### Documentation (this feature)

```text
specs/010-remove-feature-authoring/
├── plan.md              # This file
├── spec.md              # Feature specification
├── research.md          # Phase 0 research decisions
├── data-model.md        # Phase 1 data model (deletion inventory)
├── quickstart.md        # Phase 1 implementation guide
├── contracts/
│   └── removal-inventory.md  # Complete file-level removal contract
└── tasks.md             # Phase 2 output (/speckit.tasks command)
```

### Source Code Impact (repository root)

```text
crates/deacon/src/
├── cli.rs                          # MODIFY: Remove FeatureCommands enum, Commands::Features variant,
│                                   #         TemplateCommands authoring variants, dispatch logic
├── commands/
│   ├── mod.rs                      # MODIFY: Remove features/features_monolith/features_publish_output mods
│   ├── features/                   # DELETE: Entire directory (7 files)
│   ├── features_monolith.rs        # DELETE: ~2,800 lines
│   ├── features_publish_output.rs  # DELETE
│   └── templates.rs                # MODIFY: Remove authoring functions, simplify dispatcher

crates/core/src/
├── lib.rs                          # MODIFY: Remove features_info and features_test mod declarations
├── features_info.rs                # DELETE
├── features_test/                  # DELETE: Entire directory (5 files)
├── features.rs                     # PRESERVE: Shared with FeatureInstaller
├── feature_installer.rs            # PRESERVE: Consumer feature installation
└── oci/                            # PRESERVE: Shared OCI client

crates/deacon/tests/                # DELETE: 12 test files for removed commands
                                    # MODIFY: test_templates_cli.rs (keep pull/apply only)

crates/core/tests/                  # DELETE: 4 test files for removed commands

docs/subcommand-specs/completed-specs/
├── features-info/                  # DELETE
├── features-package/               # DELETE
├── features-plan/                  # DELETE
├── features-publish/               # DELETE
└── features-test/                  # DELETE

examples/
├── feature-management/             # DELETE
├── feature-package/                # DELETE
├── feature-plan/                   # DELETE
├── feature-publish/                # DELETE
├── features-info/                  # DELETE
├── features-test/                  # DELETE
├── template-management/
│   └── metadata-and-docs/          # DELETE (authoring example)
├── registry/
│   └── dry-run-publish/            # DELETE (authoring publish example)
├── features/                       # PRESERVE: Consumer feature examples
└── template-management/
    ├── minimal-template/           # PRESERVE: Fixture data
    ├── template-with-options/      # PRESERVE: Fixture for apply
    └── templates-apply/            # PRESERVE: Consumer example
```

**Structure Decision**: This is a removal feature — no new directories are created. The existing workspace structure (`crates/deacon/` binary + `crates/core/` library) remains unchanged. All modifications are deletions or simplifications within the existing structure.

## Complexity Tracking

No constitution violations. This is a pure removal feature that reduces complexity.

## Design Artifacts

| Artifact | Path | Status |
|----------|------|--------|
| Research | [research.md](research.md) | Complete — 12 decisions documented |
| Data Model | [data-model.md](data-model.md) | Complete — deletion/preservation inventory |
| Removal Contract | [contracts/removal-inventory.md](contracts/removal-inventory.md) | Complete — file-level action contract |
| Quickstart | [quickstart.md](quickstart.md) | Complete — implementation guide with verification |
