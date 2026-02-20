# Research: Remove Feature Authoring Commands

**Feature Branch**: `010-remove-feature-authoring`
**Date**: 2026-02-19

## Decision 1: Feature Flag Impact on Removal

**Decision**: Remove both the `#[cfg(feature = "full")]`-gated code blocks and the module declarations for features/templates authoring. No feature flag changes needed.

**Rationale**: All six `features` subcommands and three `templates` authoring subcommands are gated behind `#[cfg(feature = "full")]` in `cli.rs` and `commands/mod.rs`. Removing the code entirely is cleaner than adding new feature flags. The `"full"` feature flag will continue to gate other commands (`build`, `config`, `outdated`, `run-user-commands`) that remain.

**Alternatives considered**:
- Gate authoring commands behind a separate feature flag (e.g., `"authoring"`) — rejected because the goal is permanent removal, not optional inclusion.
- Remove the `"full"` feature flag entirely — rejected because other non-authoring commands still use it.

## Decision 2: OCI Module Preservation

**Decision**: The `crates/core/src/oci/` directory and all its sub-modules are **fully preserved**. They are shared between authoring commands (publish) and consumer commands (feature installation via `FeatureInstaller`, `templates pull`).

**Rationale**: The OCI client, auth, semver, and install modules serve the consumer feature installation path (`deacon up` → `FeatureInstaller`) and `templates pull`. Removing them would break consumer functionality.

**Alternatives considered**: None — preservation is mandatory per spec FR-021 and FR-012.

## Decision 3: Core `features.rs` vs `features_info.rs` vs `features_test/`

**Decision**:
- **Preserve** `crates/core/src/features.rs` — shared feature model used by `FeatureInstaller`
- **Delete** `crates/core/src/features_info.rs` — exclusively serves `features info` command
- **Delete** `crates/core/src/features_test/` — exclusively serves `features test` command

**Rationale**: `features.rs` contains the `Feature`, `FeatureSet`, and related types consumed by the feature installer during `deacon up`. The `features_info.rs` module exports `VerboseJson`, `PublishedTagsJson`, `ManifestJson` types used only by `features info`. The `features_test/` module exports test discovery and execution functions used only by `features test`.

**Alternatives considered**: None — the module boundaries are clean and well-defined.

## Decision 4: Template Examples Disposition

**Decision**:
- **Delete** `examples/template-management/metadata-and-docs/` — demonstrates `templates metadata` and `templates generate-docs` (authoring)
- **Preserve** `examples/template-management/minimal-template/` — serves as template fixture data
- **Preserve** `examples/template-management/template-with-options/` — serves as fixture for `templates apply` examples
- **Preserve** `examples/template-management/templates-apply/` — demonstrates consumer `templates apply`

**Rationale**: The `metadata-and-docs` directory exclusively documents authoring commands. The other directories either serve as fixture data or demonstrate the preserved `templates apply` consumer command.

**Alternatives considered**: Delete all template management examples — rejected because the consumer `templates apply` example and its fixture data must remain.

## Decision 5: Registry Examples Disposition

**Decision**:
- **Delete** `examples/registry/dry-run-publish/` — demonstrates `features publish` and `templates publish` dry-run workflows
- **Preserve** `examples/registry/authentication/` — demonstrates OCI auth used by consumer commands too

**Rationale**: The dry-run-publish directory exclusively demonstrates authoring publish workflows. The authentication directory demonstrates patterns shared with consumer OCI operations.

**Alternatives considered**: Modify dry-run-publish to keep only consumer examples — rejected because no consumer commands use the publish dry-run pattern.

## Decision 6: `features_monolith.rs` Complete Removal

**Decision**: Delete `crates/deacon/src/commands/features_monolith.rs` entirely (~2,800 lines).

**Rationale**: This monolith file implements all six features subcommands. With the entire `features` subcommand group removed, every function in this file becomes dead code. No consumer command imports from this module.

**Alternatives considered**: Extract shared helpers — rejected because no shared helpers are used by consumer commands. The OCI publishing helpers are authoring-specific.

## Decision 7: `features_publish_output.rs` Complete Removal

**Decision**: Delete `crates/deacon/src/commands/features_publish_output.rs` entirely.

**Rationale**: Contains `PublishOutput`, `PublishSummary`, `PublishFeatureResult`, `PublishCollectionResult` types used exclusively by `features publish` command output formatting.

**Alternatives considered**: None — types are exclusively authoring-related.

## Decision 8: Templates Module Simplification Strategy

**Decision**: Significantly simplify `crates/deacon/src/commands/templates.rs` by removing authoring functions (`execute_templates_metadata`, `execute_templates_publish`, `execute_templates_generate_docs`, `create_template_package`, `generate_readme_fragment`, `output_result`) and their associated imports. Simplify `TemplatesResult` if authoring-only fields become unused.

**Rationale**: The file currently mixes consumer and authoring functionality. After removal, only `execute_templates_pull` and `execute_templates_apply` remain, along with the simplified dispatcher.

**Alternatives considered**: Split into separate files — rejected as over-engineering for two remaining functions.

## Decision 9: `features/` Subcommand Module Directory Complete Removal

**Decision**: Delete `crates/deacon/src/commands/features/` entire directory (mod.rs, plan.rs, package.rs, publish.rs, test.rs, shared.rs, unit_features_package.rs).

**Rationale**: This module directory contains the subcommand implementations that dispatch to `features_monolith.rs`. With the entire features subcommand group removed, the module serves no purpose.

**Alternatives considered**: None — all files in this directory exclusively serve removed commands.

## Decision 10: Nextest Configuration Cleanup

**Decision**: Remove all test binary references for deleted test files from `.config/nextest.toml` across all profiles (default, dev-fast, full, ci, docker).

**Rationale**: Deleted test binaries (`test_features_cli`, `integration_features_test_json`, `integration_features_publish`, `integration_features_info_*`, `integration_features_package`, `unit_features_package`, `features_info_models`, `features_test_discovery`, `features_test_paths`, `features_test_scenarios`) will no longer exist. References to them in nextest config would cause warnings or confusion.

**Alternatives considered**: Leave references in place and let nextest ignore missing binaries — rejected because it violates code hygiene principles.

## Decision 11: README.md and Documentation Updates

**Decision**: Update `README.md` to remove:
- Feature Management example references and commands
- Features Test example references and commands
- Features Info example references and commands
- Output Streams section reference to `deacon features plan --json`
- Roadmap spec references to features-test, features-package, features-publish, features-info, features-plan

Update `docs/CLI_PARITY.md` to remove all references to removed features and templates authoring commands.

Update `examples/README.md` to remove all authoring example references.

**Rationale**: Documentation must reflect the actual command surface. Stale references to removed commands would confuse users and violate the constitution's documentation-code sync requirement.

**Alternatives considered**: None — documentation must be current.

## Decision 12: License Field Update

**Decision**: Change `license = "Apache-2.0"` to `license = "MIT"` in workspace `Cargo.toml` (line 10).

**Rationale**: The LICENSE file already contains MIT license text. The README badge already says MIT. Only the Cargo.toml is mismatched. This is a spec requirement (FR-015).

**Alternatives considered**: None — straightforward metadata fix per spec.
