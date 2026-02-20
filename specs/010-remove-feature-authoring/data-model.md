# Data Model: Remove Feature Authoring Commands

**Feature Branch**: `010-remove-feature-authoring`
**Date**: 2026-02-19

## Overview

This feature is a **removal** — no new data models are introduced. This document catalogs the data models being **deleted** and the models that must be **preserved** to ensure consumer functionality remains intact.

## Models Being Deleted

### CLI Layer (`crates/deacon/`)

#### `FeatureCommands` enum (cli.rs)
The entire enum is removed. Variants:
- `Test` — test subcommand args
- `Package` — package subcommand args
- `Pull` — pull subcommand args (stub)
- `Publish` — publish subcommand args
- `Info` — info subcommand args
- `Plan` — plan subcommand args

#### `TemplateCommands` variants (cli.rs) — partial removal
Removed variants:
- `Publish` — publish subcommand args
- `Metadata` — metadata subcommand args
- `GenerateDocs` — generate-docs subcommand args

Preserved variants:
- `Apply` — consumer template application
- `Pull` — consumer template pull from OCI registry

#### `Commands::Features` variant (cli.rs)
Entire enum variant removed from the top-level `Commands` enum.

#### Feature authoring output types (`features_publish_output.rs`)
- `PublishOutput` — top-level publish result
- `PublishSummary` — publish summary stats
- `PublishFeatureResult` — per-feature publish outcome
- `PublishCollectionResult` — collection-level publish outcome

#### Feature authoring shared types (`features/shared.rs`)
- `PackagingMode` — single vs collection packaging
- `CollectionMetadata` — collection-level metadata
- `FeatureDescriptor` — discovered feature descriptor
- `SourceInformation` — source attribution

#### Templates result type (`templates.rs`) — simplification
`TemplatesResult` struct: Remove authoring-only fields (`digest`, `size`) if they are not used by consumer commands.

### Core Library (`crates/core/`)

#### `features_info` module (`features_info.rs`)
- `VerboseJson` — verbose info output structure
- `PublishedTagsJson` — tag listing output
- `ManifestJson` — manifest output structure

#### `features_test` module (`features_test/`)
- `TestRun` — test execution result model
- `TestScenario` — test scenario definition
- `TestCollection` — test collection discovery result
- Various error types in `errors.rs`
- Discovery types in `discovery.rs`
- Runner state in `runner.rs`

## Models Being Preserved

### Core Library (`crates/core/`)

| Module | Key Types | Consumer Usage |
|--------|-----------|---------------|
| `features.rs` | `Feature`, `FeatureSet`, feature parsing | `FeatureInstaller`, `deacon up` |
| `feature_installer.rs` | `FeatureInstaller`, installation workflow | `deacon up` feature installation |
| `feature_ref.rs` | `FeatureRef`, OCI reference parsing | Feature resolution during `up` |
| `oci/` | `OciClient`, `TemplateRef`, registry client | Feature installation, `templates pull` |
| `templates.rs` | `TemplateMetadata`, template parsing | `templates apply`, `templates pull` |
| `registry_parser.rs` | `parse_registry_reference` | OCI ref parsing for all commands |

### CLI Layer (`crates/deacon/`)

| Module | Key Types | Consumer Usage |
|--------|-----------|---------------|
| `commands/templates.rs` | `execute_templates_pull`, `execute_templates_apply` | `templates pull`, `templates apply` |
| `commands/up.rs` | Up command with feature installation | `deacon up` |

## State Transitions

N/A — This is a removal feature with no new state machines or transitions.

## Validation Rules

Post-removal validation:
1. `cargo build --all-features` must succeed with zero errors
2. `cargo clippy --all-targets -- -D warnings` must produce zero warnings
3. All retained tests must pass
4. No orphaned imports or unused variables
