# Removal Inventory Contract

**Feature Branch**: `010-remove-feature-authoring`
**Date**: 2026-02-19

This contract defines the complete inventory of files and directories to delete, modify, or preserve. Every item in the "Delete" and "Modify" sections is a required action; every item in the "Preserve" section is a constraint that must NOT be violated.

## Files and Directories to DELETE

### CLI Command Modules (`crates/deacon/src/commands/`)
| Path | Description |
|------|-------------|
| `features/` (entire directory) | Features subcommand implementations (mod.rs, plan.rs, package.rs, publish.rs, test.rs, shared.rs, unit_features_package.rs) |
| `features_monolith.rs` | Features monolith implementation (~2,800 lines) |
| `features_publish_output.rs` | Publish output types |

### Core Library Modules (`crates/core/src/`)
| Path | Description |
|------|-------------|
| `features_info.rs` | Features info types (VerboseJson, PublishedTagsJson, ManifestJson) |
| `features_test/` (entire directory) | Features test infrastructure (mod.rs, discovery.rs, errors.rs, model.rs, runner.rs) |

### CLI Tests (`crates/deacon/tests/`)
| Path | Description |
|------|-------------|
| `test_features_cli.rs` | Features CLI integration tests |
| `cli_flags_features_info.rs` | Features info CLI flag tests |
| `integration_features_info_auth.rs` | Features info auth integration |
| `integration_features_info_dependencies.rs` | Features info dependencies integration |
| `integration_features_info_local.rs` | Features info local integration |
| `integration_features_info_manifest.rs` | Features info manifest integration |
| `integration_features_info_tags.rs` | Features info tags integration |
| `integration_features_info_verbose.rs` | Features info verbose integration |
| `integration_features_package.rs` | Features package integration |
| `integration_features_publish.rs` | Features publish integration |
| `integration_features_test_json.rs` | Features test JSON integration |
| `unit_features_package.rs` | Features package unit tests |

### Core Library Tests (`crates/core/tests/`)
| Path | Description |
|------|-------------|
| `features_info_models.rs` | Features info model tests |
| `features_test_discovery.rs` | Features test discovery tests |
| `features_test_paths.rs` | Features test path tests |
| `features_test_scenarios.rs` | Features test scenario tests |

### Spec Documentation (`docs/subcommand-specs/completed-specs/`)
| Path | Description |
|------|-------------|
| `features-info/` (entire directory) | Features info spec (SPEC.md, DATA-STRUCTURES.md, etc.) |
| `features-package/` (entire directory) | Features package spec |
| `features-plan/` (entire directory) | Features plan spec |
| `features-publish/` (entire directory) | Features publish spec |
| `features-test/` (entire directory) | Features test spec |

### Example Directories (`examples/`)
| Path | Description |
|------|-------------|
| `feature-management/` (entire directory) | Feature authoring examples (minimal-feature, feature-with-options) |
| `feature-package/` (entire directory) | Feature packaging examples |
| `feature-plan/` (entire directory) | Feature plan examples |
| `feature-publish/` (entire directory) | Feature publish examples |
| `features-info/` (entire directory) | Features info examples (manifest, tags, dependencies, verbose) |
| `features-test/` (entire directory) | Features test examples (basic, filtering, custom env, etc.) |
| `template-management/metadata-and-docs/` | Templates metadata/generate-docs example |
| `registry/dry-run-publish/` | Dry-run publish example (features + templates) |

## Files to MODIFY

### CLI Registration (`crates/deacon/src/`)
| Path | Change |
|------|--------|
| `cli.rs` | Remove `Commands::Features` variant, `FeatureCommands` enum, dispatch block. Remove `TemplateCommands::Publish/Metadata/GenerateDocs` variants, simplify dispatch. |
| `commands/mod.rs` | Remove `pub mod features`, `pub mod features_monolith`, `pub mod features_publish_output` declarations |

### Core Library (`crates/core/src/`)
| Path | Change |
|------|--------|
| `lib.rs` | Remove `pub mod features_info;` and `pub mod features_test;` declarations |

### Templates Module (`crates/deacon/src/commands/`)
| Path | Change |
|------|--------|
| `templates.rs` | Remove authoring functions (metadata, publish, generate_docs, create_template_package, generate_readme_fragment, output_result). Simplify dispatcher and imports. Simplify or remove authoring-only fields from TemplatesResult. |

### Test Files
| Path | Change |
|------|--------|
| `crates/deacon/tests/test_templates_cli.rs` | Remove tests for templates publish, metadata, generate-docs. Keep tests for templates pull and apply. |

### Configuration
| Path | Change |
|------|--------|
| `Cargo.toml` (workspace root) | Change `license = "Apache-2.0"` to `license = "MIT"` |
| `.config/nextest.toml` | Remove test group assignments for deleted test binaries across all profiles |

### Documentation
| Path | Change |
|------|--------|
| `README.md` | Remove feature authoring examples, spec references, features plan output example |
| `docs/CLI_PARITY.md` | Remove references to features info, features publish, templates publish, templates metadata, templates generate-docs |
| `examples/README.md` | Remove authoring example references from index and quick start |

## Files to PRESERVE (Hard Constraints)

### Core Library — Consumer Feature Path
| Path | Reason |
|------|--------|
| `crates/core/src/features.rs` | Shared feature model for FeatureInstaller |
| `crates/core/src/feature_installer.rs` | Consumer feature installation during `deacon up` |
| `crates/core/src/feature_ref.rs` | OCI feature reference parsing |
| `crates/core/src/oci/` (entire directory) | OCI registry client shared by consumer commands |
| `crates/core/src/templates.rs` | Template parsing used by templates apply/pull |
| `crates/core/src/registry_parser.rs` | Registry reference parsing |

### CLI — Consumer Commands
| Path | Reason |
|------|--------|
| `crates/deacon/src/commands/up.rs` | Consumer `deacon up` command |
| `crates/deacon/src/commands/templates.rs` (modified) | Consumer templates pull/apply |

### Tests — Consumer Feature Tests
| Path | Reason |
|------|--------|
| `crates/core/tests/integration_feature_dependencies.rs` | Consumer feature dependency tests |
| `crates/core/tests/integration_feature_installation.rs` | Consumer feature installation tests |
| `crates/core/tests/integration_features.rs` | Consumer feature core tests |
| `crates/core/tests/integration_parallel_feature_installation.rs` | Consumer parallel installation tests |
| `crates/core/tests/integration_templates.rs` | Consumer template tests |

### Example Directories — Consumer
| Path | Reason |
|------|--------|
| `examples/features/` | Consumer feature system examples (dependencies, caching, lockfile) |
| `examples/template-management/minimal-template/` | Template fixture data |
| `examples/template-management/template-with-options/` | Template fixture for apply example |
| `examples/template-management/templates-apply/` | Consumer templates apply example |
| `examples/registry/authentication/` | OCI auth shared by consumer commands |
