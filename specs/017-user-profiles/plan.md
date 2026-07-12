# Implementation Plan: User-Scoped Profiles for Host Settings

**Branch**: `017-user-profiles` | **Date**: 2026-07-12 | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification from `/specs/017-user-profiles/spec.md`

## Summary

Extend the read-only host settings file (`{user_data_folder}/settings.json`) with a
`profiles` map, a `defaultProfile` selector, and an optional root-level `mergeConfig`,
plus a global `--profile` / `DEACON_PROFILE` selector. A selected profile layers one or
more devcontainer.json fragments onto the resolved configuration (reusing the existing
`--override-config` merge machinery) and may override the root scalar settings
(`hostCa`, `browser`). Resolution is a shared helper consumed by every subcommand that
honors `--override-config` today, so behavior does not vary by subcommand. Unknown/dangling
profile references fail fast; a missing/profiles-free settings file behaves exactly as
today; unknown keys round-trip for forward compatibility.

**Technical approach**: add profile types + a pure resolver to `crates/core/src/settings.rs`;
generalize the single override input in the shared config loader (and
`ConfigLoader::load_with_overrides_and_substitution`) to an **ordered list** of override
paths so the precedence ladder (project ⊕ root override ⊕ profile override ⊕ CLI override)
falls out of the existing merge; add one shared CLI glue helper that loads settings,
resolves the selection, logs the applied profile to stderr, and returns the ordered
override paths + effective scalars; make the two existing `Settings::load` scalar readers
profile-aware. The host-hook trust refinement (FR-020a) is implemented (research.md Decision 6):
the origin of the effective `initializeCommand` is recomputed from the ordered override chain and
an owner-authored (user-data-folder) fragment bypasses the workspace-trust gate, while workspace-
sourced or outside-user-data fragments stay gated.

## Technical Context

**Language/Version**: Rust, Edition 2024, MSRV 1.95 (`unsafe_code = "deny"` workspace-wide)  
**Primary Dependencies**: `serde`/`serde_json` (settings + fragment parsing), `indexmap` 2.x with `serde` (declaration-ordered `profiles` map — already a core dep), `clap` (global `--profile` flag + `DEACON_PROFILE` env), `tracing` (applied-profile diagnostic), `thiserror` (core domain errors), `anyhow` (binary boundary)  
**Storage**: `{user_data_folder}/settings.json` — read-only in this feature (no write path); default `~/.deacon/settings.json`, honoring global `--user-data-folder`  
**Testing**: `cargo nextest` (unit tests in `settings.rs` + `config.rs`; integration tests under `crates/deacon/tests/` for `up`/`read-configuration` with `--profile`); doctests for new public helpers  
**Target Platform**: Linux, macOS, Windows (settings resolution is platform-agnostic; path resolution must be separator-agnostic in assertions)  
**Project Type**: CLI (workspace: `crates/deacon` binary + `crates/core` library)  
**Performance Goals**: Negligible — one additional settings-file read + path resolution per invocation, on the existing config-load path (no new container/network calls)  
**Constraints**: No change to non-profile behavior (zero regression for non-adopters); unknown-key tolerance preserved (`#[serde(default)]`, no `deny_unknown_fields`); stdout/JSON output contract unchanged (diagnostics to stderr only); fail-fast on unknown/dangling profile references and missing fragments  
**Scale/Scope**: Small, additive. ~1 new core module surface (profile types + resolver in `settings.rs`), 1 core signature generalization (`load_with_overrides_and_substitution`), 1 shared CLI helper, 1 new global flag threaded to 4 command handlers, 2 scalar-reader updates.

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

| Principle | Assessment | Status |
|-----------|-----------|--------|
| **I. Spec-Parity** | Profiles are a deacon-specific convenience (like `settings.json` itself); not in containers.dev, so no upstream conflict. Layering reuses the **existing** override-merge algorithm (no new merge semantics). Non-profile runs are byte-for-byte unchanged (FR-024). | ✅ Pass |
| **II. Consumer-Only** | Serves consumers (developers running containers), not feature/template authors. No new authoring surface. | ✅ Pass |
| **III. Build Green** | Plan includes unit + integration + doctests; new integration binary gets nextest grouping (fs-heavy, non-docker). | ✅ Pass |
| **IV. Fail Fast / Strict-on-Mistakes** | Unknown `--profile`/`defaultProfile` → hard error listing available names (FR-015/016); missing fragment → located error (FR-017). `profiles` is a typed `IndexMap<String, Profile>`, so serde already rejects a non-object (consistent with `features`/`customizations` strictness). Unknown keys preserved via `#[serde(flatten)]` on `Profile`. | ✅ Pass |
| **V. Idiomatic Rust** | New pure resolver (no IO in the merge logic), `thiserror` domain errors, no `unsafe`, no blocking-in-async (settings read stays on the existing sync-at-setup path, matching today's `Settings::load`). Modular: profile types + resolver in `settings.rs`, glue in a shared command helper. | ✅ Pass |
| **VI. Output Contracts** | Applied-profile diagnostic goes to stderr/`tracing` only; stdout and `--output json` documents are unchanged (FR-009b, SC-007). | ✅ Pass |
| **VIII. Shared Abstractions** | Profile resolution is a **single shared helper**; the override-path generalization lives in the shared `config_loader` + core loader, consumed everywhere. No per-subcommand bespoke logic. | ✅ Pass |
| **VII. Testing Completeness** | Selection precedence, three-state default model, scalar inheritance, override ordering, fail-fast errors, unknown-key tolerance, and per-subcommand parity are all enumerated as required tests. | ✅ Pass |

**Result**: All gates pass. No entries in Complexity Tracking. FR-020a (host-hook trust
follows author) — originally a tracked phased deferral — is implemented in this delivery, so
no deferrals remain.

## Project Structure

### Documentation (this feature)

```text
specs/017-user-profiles/
├── plan.md              # This file
├── spec.md              # Feature spec (+ Clarifications)
├── research.md          # Phase 0 output
├── data-model.md        # Phase 1 output
├── quickstart.md        # Phase 1 output
├── contracts/
│   └── cli-profile.md   # CLI flag + settings.json schema contract
├── checklists/
│   └── requirements.md  # Spec quality checklist (from /speckit.specify)
└── tasks.md             # Phase 2 output (/speckit.tasks — NOT created here)
```

### Source Code (repository root)

```text
crates/core/src/
├── settings.rs                     # EXTEND: Profile struct, MergeConfigPaths (single-or-list),
│                                   #   Settings.{profiles, default_profile, override_config},
│                                   #   ResolvedSettings + Settings::resolve(selected, settings_dir)
│                                   #   + ProfileError (thiserror) for fail-fast selection/validation
└── config.rs                       # EXTEND: generalize ConfigLoader::load_with_overrides_and_substitution
                                    #   from `override_config_path: Option<&Path>` to an ordered
                                    #   `override_config_paths: &[&Path]` (low→high). merge_configs unchanged.

crates/deacon/src/
├── cli.rs                          # ADD: global `--profile <NAME>` (env DEACON_PROFILE) on Cli + CliContext;
│                                   #   thread to Up/ReadConfiguration/Build/Outdated handlers;
│                                   #   make resolve_host_ca_activation_cli profile-aware (line ~1048)
├── commands/shared/
│   ├── config_loader.rs            # EXTEND: ConfigLoadArgs gains ordered settings_override_paths;
│   │                               #   load_config builds final ordered override list [settings…, cli]
│   └── profile.rs                  # NEW: shared glue — load Settings, resolve selection
│                                   #   (--profile > defaultProfile > none), validate, log applied
│                                   #   profile to stderr, return ordered override paths + effective scalars
└── commands/up/
    └── forward.rs                  # EXTEND: browser reader (line ~125) uses effective (profile-aware) value;
                                    #   thread selected profile name to the port-forward daemon

crates/deacon/tests/
└── integration_profiles.rs         # NEW: up/read-configuration with --profile, defaultProfile,
                                    #   unknown-profile error, scalar inheritance (nextest: fs-heavy, non-docker)
```

**Structure Decision**: Single Rust workspace (binary `crates/deacon` + library `crates/core`),
matching the existing layout. Profile domain logic (types, resolution, validation, errors)
lives in `crates/core/src/settings.rs` alongside the existing `Settings`; CLI wiring and the
one shared resolution helper live in `crates/deacon/src/commands/shared/`. No new crates.

## Complexity Tracking

> No constitution violations — this section is intentionally empty.
