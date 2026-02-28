# Implementation Plan: Named Config Folder Search

**Branch**: `014-named-config-search` | **Date**: 2026-02-22 | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification from `/specs/014-named-config-search/spec.md`

## Summary

Add support for the third devcontainer.json search location: named config folders inside `.devcontainer/`. When no config exists at the two existing priority locations (`.devcontainer/devcontainer.json`, `.devcontainer.json`), the tool enumerates direct subdirectories of `.devcontainer/` looking for `devcontainer.json`/`devcontainer.jsonc`. If exactly one is found, it is used automatically. If multiple are found, an error lists them and requires `--config` selection. This is implemented by extending `ConfigLoader::discover_config()` in core with a new `DiscoveryResult` type, then adapting the two call sites (shared `load_config()` and `down` command) to handle the multiple-configs case.

## Technical Context

**Language/Version**: Rust 1.70+ (Edition 2021)
**Primary Dependencies**: serde, tracing, thiserror, clap (existing — no new dependencies)
**Storage**: N/A (filesystem-only config discovery)
**Testing**: cargo-nextest (`make test-nextest-fast` for iteration, `make test-nextest` before PR)
**Target Platform**: Linux (primary), macOS, Windows (cross-platform path handling)
**Project Type**: CLI tool (core library + binary crate)
**Performance Goals**: Config discovery completes in <10ms for typical workspace layouts
**Constraints**: Must be backward-compatible with existing two-location search; alphabetical sort for deterministic multi-platform behavior
**Scale/Scope**: ~3 files modified in core + CLI, ~200 lines of new code, ~15 new unit tests

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

| Principle | Status | Notes |
|-----------|--------|-------|
| I. Spec-Parity | PASS | Implements the third search location per upstream containers.dev spec. Priority order and one-level-deep enumeration match spec exactly. |
| II. Consumer-Only Scope | PASS | Config discovery is consumer functionality (finding configs to use containers). |
| III. Keep the Build Green | PASS | Will run `cargo fmt`, `cargo clippy`, `make test-nextest-fast` after every change. |
| IV. No Silent Fallbacks | PASS | Multiple configs produce a clear error listing paths; no silent selection. |
| V. Idiomatic, Safe Rust | PASS | Uses `std::fs::read_dir` for enumeration, `Result` propagation, `thiserror` for new error variant. |
| VI. Observability | PASS | Tracing spans/debug logs for discovery steps. No output contract changes. |
| VII. Testing Completeness | PASS | Unit tests for all discovery scenarios; integration tests via shared loader. |
| VIII. Shared Abstractions | PASS | Extends existing `ConfigLoader::discover_config()` — all 6 commands benefit through shared `load_config()`. `down` command's custom call site also updated. |
| IX. Examples | N/A | No example changes needed — this is internal discovery logic. |

**Pre-Implementation Validation**:
1. Spec review: FR-001 through FR-009 mapped to implementation
2. Scope check: Consumer command (config discovery)
3. Data model: New `DiscoveryResult` enum replaces `ConfigLocation` return
4. Algorithm: Three-tier search with short-circuit, alphabetical enumeration
5. Input validation: `--config` bypass, filename validation unchanged
6. Config resolution: Discovery feeds into existing `load_with_extends` chain
7. Output contracts: No JSON schema changes
8. Testing: 15+ new tests covering all acceptance scenarios and edge cases
9. Infrastructure reuse: `ConfigLoader`, `ConfigError`, shared `load_config()`
10. Nextest config: New tests are unit tests — no new test group needed

## Project Structure

### Documentation (this feature)

```text
specs/014-named-config-search/
├── plan.md              # This file
├── research.md          # Phase 0: design decisions
├── data-model.md        # Phase 1: data structures
├── quickstart.md        # Phase 1: implementation guide
├── contracts/           # Phase 1: interface contracts
│   └── config-discovery.md  # Discovery function contract
└── tasks.md             # Phase 2 output (/speckit.tasks command)
```

### Source Code (repository root)

```text
crates/
├── core/src/
│   ├── config.rs         # ConfigLoader::discover_config() — MODIFY
│   │                     # Add DiscoveryResult enum, enumerate_named_configs()
│   └── errors.rs         # ConfigError — MODIFY (add MultipleConfigs variant)
└── deacon/src/commands/
    ├── shared/
    │   └── config_loader.rs  # load_config() — MODIFY (handle DiscoveryResult)
    └── down.rs               # Custom discovery call — MODIFY (handle DiscoveryResult)
```

**Structure Decision**: Existing Rust workspace structure. Changes are localized to 4 files in the existing module hierarchy. No new modules or crates needed.

## Complexity Tracking

> No constitution violations — no entries needed.
