# Quickstart: Remove Feature Authoring Commands

**Feature Branch**: `010-remove-feature-authoring`
**Date**: 2026-02-19

## What This Feature Does

Removes all DevContainer Feature authoring commands from Deacon, narrowing the CLI to consumer-only commands. After this change:
- The `features` subcommand group no longer exists
- The `templates` group retains only `pull` and `apply`
- Feature installation during `deacon up` works exactly as before
- Dead code is fully cleaned up
- License metadata is aligned to MIT

## Implementation Strategy

This is a **surgical removal** — delete authoring code, preserve consumer code, clean up references.

### Phase 1: Core Removal (Compilation-Breaking)
1. Remove `FeatureCommands` enum and `Commands::Features` variant from `cli.rs`
2. Remove authoring variants from `TemplateCommands` enum in `cli.rs`
3. Remove dispatch logic for removed commands in `cli.rs`
4. Remove module declarations from `commands/mod.rs`
5. Delete `commands/features/` directory, `features_monolith.rs`, `features_publish_output.rs`
6. Simplify `commands/templates.rs` to only pull/apply
7. Remove `pub mod features_info` and `pub mod features_test` from core `lib.rs`
8. Delete `features_info.rs` and `features_test/` from core

### Phase 2: Test Cleanup
1. Delete all test files for removed commands (16 files across deacon/core)
2. Modify `test_templates_cli.rs` to keep only pull/apply tests
3. Clean up nextest configuration for deleted test binaries

### Phase 3: Documentation & Examples
1. Delete 5 spec directories under `docs/subcommand-specs/completed-specs/`
2. Delete 8 example directories (6 features + 1 template authoring + 1 registry publish)
3. Update `README.md`, `CLI_PARITY.md`, `examples/README.md`

### Phase 4: License & Verification
1. Fix Cargo.toml license field
2. Run `cargo fmt --all && cargo clippy --all-targets -- -D warnings`
3. Run `make test-nextest-fast` to verify consumer functionality
4. Search for any remaining orphaned references

## Verification Checklist

```bash
# 1. Build succeeds
cargo build --all-features

# 2. Lint clean
cargo clippy --all-targets -- -D warnings

# 3. Format clean
cargo fmt --all -- --check

# 4. Tests pass
make test-nextest-fast

# 5. Features group gone from help
cargo run -- --help | grep -i features  # should find nothing

# 6. Templates only shows pull/apply
cargo run -- templates --help  # should only list pull and apply

# 7. License aligned
grep 'license' Cargo.toml  # should show MIT

# 8. No orphaned references
grep -r "features_info\|features_test\|features_monolith\|features_publish_output" crates/  # should find nothing
```
