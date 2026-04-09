---
name: consumer-core-completion
version: '1'
created: '2026-04-09'
objective: 'Complete Deacon''s consumer-facing DevContainer CLI surface by implementing
  5 independent beads: updateRemoteUserUID, Compose profile selection, runArgs passthrough,
  feature installation timing fix, and license housekeeping.'
tags:
- deacon
- devcontainer
- spec-compliance
- rust
- consumer-core
- bug-fix
- feature-implementation
- housekeeping
scope:
  in_scope:
  - "Bead 1: Implement updateRemoteUserUID \u2014 host UID/GID detection, ephemeral\
    \ Dockerfile layer with usermod/groupmod, skip conditions (non-Linux, root, already\
    \ matching, explicitly false), graceful fallback on failure"
  - "Bead 2: Docker Compose profile selection \u2014 parse and forward --profile flags\
    \ to docker compose up/down/etc., support multiple profiles"
  - "Bead 3: Wire runArgs passthrough \u2014 forward runArgs array to docker create/run\
    \ after Deacon flags and before image name, ignore in Compose mode"
  - "Bead 4: Fix feature installation timing (GitHub issue #1) \u2014 move feature\
    \ installation from running container to image build phase via generated Dockerfile,\
    \ deterministic layers for cache friendliness"
  - "Bead 5: Fix license metadata \u2014 update Cargo.toml license fields from Apache-2.0\
    \ to MIT across all workspace members, verify LICENSE file and README consistency"
  - Unit and integration tests for all new behavior
  - Nextest test group configuration for any new integration tests
  out_of_scope:
  - "Variable substitution engine \u2014 complete and well-tested, do not modify"
  - "Lifecycle command execution \u2014 SP-003/004/005 already shipped fixes"
  - "CLI flag signature changes \u2014 no removal or renaming of existing flags"
  - "Feature authoring commands (test, info, plan, package, publish) \u2014 permanently\
    \ out of scope per consumer-only constraint"
  - "Podman runtime support \u2014 in development separately"
  - Any changes to the devcontainer spec itself
  boundaries: []
---

# consumer-core-completion

## Objective

Complete Deacon's consumer-facing DevContainer CLI surface by implementing 5 independent beads: updateRemoteUserUID, Compose profile selection, runArgs passthrough, feature installation timing fix, and license housekeeping.

## Context

# Deacon Consumer Core Completion

## Background
Deacon is a Rust implementation of the DevContainer CLI following the containers.dev specification. A February 2026 audit identified spec-compliance gaps and missing capabilities affecting real-world `devcontainer.json` configurations. This plan addresses 5 independent beads covering the remaining consumer-facing surface.

## Architecture
- **Workspace:** Two crates — `crates/deacon` (CLI binary, clap 4.5) and `crates/core` (library with domain logic)
- **Key files:** `crates/core/src/config.rs` (config resolution), `crates/core/src/container.rs` (container runtime), `crates/core/src/feature_installer.rs` (OCI features), `crates/deacon/src/commands/up/` (up command flow)
- **Spec source of truth:** `docs/subcommand-specs/*/SPEC.md` and upstream devcontainers/spec repo

## Bead Summary
| Bead | Priority | Risk | Description |
|------|----------|------|-------------|
| 1 | High | Medium | updateRemoteUserUID — sync container user UID/GID to host on Linux |
| 2 | High | Low | Docker Compose profile selection via --profile flags |
| 3 | Medium | Low | runArgs passthrough to docker create/run |
| 4 | High | Medium | Fix feature installation timing — build phase not running container |
| 5 | Low | None | License metadata housekeeping (Apache-2.0 → MIT) |

## Suggested Sequencing
By impact: Bead 4 → Bead 1 → Bead 2 → Bead 3 → Bead 5. All are independent and can be parallelized.

## Key Risk: Bead 4 (Feature Installation Timing)
This is the highest-impact change. Currently features install into running containers, losing them on rebuild and breaking layer caching. The fix requires generating a Dockerfile that layers feature install.sh scripts as RUN instructions with options as ENV vars, building an ephemeral image, then using that image for container creation. Must preserve the existing no-features path unchanged.

## Success Criteria

- [ ] Bead 1: updateRemoteUserUID=true (default) updates container user UID/GID to match host on Linux
- [ ] Bead 1: updateRemoteUserUID=false skips the update
- [ ] Bead 1: Root user (UID 0) is never modified
- [ ] Bead 1: Non-Linux platforms skip the update entirely
- [ ] Bead 1: UID already matching skips the update
- [ ] Bead 1: Failure to update UID logs warning but does not abort up
- [ ] Bead 2: Compose profiles can be specified and forwarded to docker compose commands
- [ ] Bead 2: Multiple profiles can be activated simultaneously
- [ ] Bead 2: docker compose down also receives profile flags
- [ ] Bead 2: Default behavior (no profiles) is unchanged
- [ ] Bead 3: runArgs values are forwarded to docker create command
- [ ] Bead 3: runArgs is ignored in Compose mode
- [ ] Bead 4: Features are installed during image build, not in running container
- [ ] Bead 4: Feature options are passed correctly as env vars
- [ ] Bead 4: Configs without features skip the feature build step
- [ ] Bead 4: Rebuilding without config changes uses Docker cache
- [ ] Bead 5: All Cargo.toml files specify license = MIT
- [ ] Bead 5: LICENSE file at workspace root contains MIT text
- [ ] All changes pass cargo clippy and cargo test
- [ ] No regressions in existing test suite

## Scope

### In

- Bead 1: Implement updateRemoteUserUID — host UID/GID detection, ephemeral Dockerfile layer with usermod/groupmod, skip conditions (non-Linux, root, already matching, explicitly false), graceful fallback on failure
- Bead 2: Docker Compose profile selection — parse and forward --profile flags to docker compose up/down/etc., support multiple profiles
- Bead 3: Wire runArgs passthrough — forward runArgs array to docker create/run after Deacon flags and before image name, ignore in Compose mode
- Bead 4: Fix feature installation timing (GitHub issue #1) — move feature installation from running container to image build phase via generated Dockerfile, deterministic layers for cache friendliness
- Bead 5: Fix license metadata — update Cargo.toml license fields from Apache-2.0 to MIT across all workspace members, verify LICENSE file and README consistency
- Unit and integration tests for all new behavior
- Nextest test group configuration for any new integration tests

### Out

- Variable substitution engine — complete and well-tested, do not modify
- Lifecycle command execution — SP-003/004/005 already shipped fixes
- CLI flag signature changes — no removal or renaming of existing flags
- Feature authoring commands (test, info, plan, package, publish) — permanently out of scope per consumer-only constraint
- Podman runtime support — in development separately
- Any changes to the devcontainer spec itself

## Constraints

- Maintain unsafe_code = forbid workspace policy
- All changes must pass cargo clippy --all-targets -- -D warnings (zero tolerance)
- All changes must pass cargo fmt --all -- --check
- Rust edition 2021, async runtime: tokio
- Do not modify the variable substitution engine
- Do not modify lifecycle command execution (SP-003/004/005)
- Do not remove existing commands or change CLI flag signatures
- Spec-parity: all behavior must align with upstream devcontainers/spec (commit 113500f4) and docs/subcommand-specs/*/SPEC.md
- No silent fallbacks — fail fast with clear errors except where spec mandates graceful degradation (e.g., Bead 1 UID update failure)
- Panic-free runtime code — no unwrap()/unchecked expect() in non-test paths
- Consumer-only scope — no feature authoring functionality
- All 5 beads are independent with no inter-dependencies
