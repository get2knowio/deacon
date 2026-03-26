# PRD: Deacon Consumer Core Completion

## Overview

Complete the consumer-facing surface of Deacon's DevContainer CLI implementation. These are spec-compliance fixes and missing capabilities identified in the February 2026 audit that affect real-world `devcontainer.json` configurations. Each item has clear acceptance criteria defined by the DevContainer specification.

**Target repo:** `get2knowio/deacon` (Rust, Cargo workspace)
**Architecture:** Two crates — `crates/deacon` (CLI binary, clap 4.5) and `crates/core` (library)

---

## Bead 1: Implement updateRemoteUserUID

**Priority:** High
**Risk:** Medium — touches the container creation flow in `up`, needs Linux-specific logic

### Context

The `updateRemoteUserUID` property (defaults to `true`) syncs the remote user's UID/GID inside the container to match the host user's UID/GID on Linux. This prevents bind mount permission problems that are the #1 pain point for Linux devcontainer users.

The property is already parsed in the config struct (`DevContainerConfig`) but the UID/GID update logic is not implemented. Currently the field is read but never acted upon.

### Requirements

1. **Detect host UID/GID** — On Linux only, read the current user's UID and GID from the OS.

2. **Determine target user** — Resolve the effective remote user: `remoteUser` if set, else `containerUser` if set, else the image's default USER. The UID update targets this user.

3. **Skip conditions** — Do NOT perform UID update if:
   - Not running on Linux (macOS and Windows handle this via their VM layer)
   - `updateRemoteUserUID` is explicitly `false`
   - The target user is root (UID 0)
   - The target user's UID already matches the host UID

4. **Update mechanism** — Per the spec, the update should happen "prior to creating the container" via an image modification. The reference implementation:
   - Creates a temporary Dockerfile layer: `FROM <image>` then runs `usermod -u <host-uid> <username>` and `groupmod -g <host-gid> <groupname>`
   - Builds this as an ephemeral image
   - Uses the modified image for container creation
   - This approach avoids modifying a running container's filesystem

5. **Home directory ownership** — After UID/GID change, update ownership of the user's home directory to match the new UID/GID.

6. **Error handling** — If the UID update fails (e.g., target UID already in use by another user), log a warning and proceed with the original image. Do not fail the `up` command.

### Acceptance Criteria

- [ ] `updateRemoteUserUID: true` (default) updates container user UID/GID to match host on Linux
- [ ] `updateRemoteUserUID: false` skips the update
- [ ] Root user (UID 0) is never modified
- [ ] Non-Linux platforms skip the update entirely
- [ ] UID already matching skips the update (no unnecessary image rebuild)
- [ ] Failure to update UID logs a warning but does not abort `up`
- [ ] Existing tests continue to pass
- [ ] New tests cover: Linux UID mismatch path, skip-on-root, skip-on-match, skip-on-false

### Location

- Config reading: `crates/core/src/config.rs` — `update_remote_user_uid` field
- Implementation: `crates/deacon/src/commands/up/` — integrate into the image preparation step before container creation

---

## Bead 2: Docker Compose Profile Selection

**Priority:** High
**Risk:** Low — additive feature, clear Docker Compose semantics

### Context

Docker Compose supports profiles for selectively enabling services. In `docker-compose.yml`, services can have a `profiles` key. Services without a profile are always started. Services with a profile only start when that profile is explicitly activated.

Deacon's `runServices` field controls which services to start, but it doesn't support Compose's native `--profile` flag. This means devcontainer configs that rely on profiles (common in monorepo setups) don't work correctly with Deacon.

### Requirements

1. **Parse Compose profiles from devcontainer.json** — The DevContainer spec doesn't have a dedicated `profiles` property; instead, profiles are activated by passing `--profile` flags to `docker compose up`. Deacon should support this through its existing `runArgs`-like mechanism or via a dedicated configuration path.

2. **Pass `--profile` flags to Compose commands** — When Deacon invokes `docker compose up`, `docker compose down`, etc., it should forward profile arguments.

3. **Support multiple profiles** — Multiple `--profile` flags can be passed to activate several profiles simultaneously.

4. **Default behavior unchanged** — If no profiles are specified, behavior is identical to today (all non-profiled services start).

### Acceptance Criteria

- [ ] Compose profiles can be specified and are forwarded to `docker compose` commands
- [ ] Multiple profiles can be activated simultaneously
- [ ] `docker compose down` also receives the profile flags (so profiled services are stopped)
- [ ] Default behavior (no profiles) is unchanged
- [ ] Existing Compose-based tests pass
- [ ] New tests cover: single profile, multiple profiles, no profiles (default)

### Location

- Compose invocation: `crates/core/src/container.rs` (or wherever `docker compose` subprocess is built)
- Config: May need a new field or may leverage existing `runArgs` passthrough

---

## Bead 3: Wire Remaining `up` Flags (runArgs Passthrough)

**Priority:** Medium
**Risk:** Low — plumbing work, straightforward Docker CLI flag forwarding

### Context

The `runArgs` property in `devcontainer.json` allows passing arbitrary flags to `docker run` (or `docker create`). The field is parsed and stored in `DevContainerConfig` but not all values are forwarded to the Docker command construction.

This is tracked as T009 in the roadmap. The audit found `runArgs` status as "⚠️ — wire remaining `up` flags."

### Requirements

1. **Forward runArgs to docker create/run** — All string values in the `runArgs` array should be appended to the Docker command that creates the container.

2. **Ordering** — `runArgs` should be inserted after Deacon's own flags but before the image name, matching Docker CLI conventions.

3. **No validation of individual flags** — Deacon should pass them through as-is. Docker will validate them. This matches the reference implementation behavior.

4. **Compose scenario** — `runArgs` is not applicable to Docker Compose scenarios (the spec only defines it for image/Dockerfile scenarios). Deacon should ignore `runArgs` when in Compose mode, optionally logging a debug message.

### Acceptance Criteria

- [ ] `runArgs` values are forwarded to `docker create` command
- [ ] Flags appear in the correct position (after Deacon flags, before image name)
- [ ] `runArgs` is ignored in Compose mode
- [ ] Existing `up` tests pass
- [ ] New tests: runArgs forwarded, empty runArgs (no change), Compose mode ignores runArgs

### Location

- Docker command construction in `crates/core/src/container.rs`
- Config field: `run_args` in `DevContainerConfig`

---

## Bead 4: Fix Feature Installation Timing (Bug #1)

**Priority:** High
**Risk:** Medium — changes the container build flow

### Context

GitHub issue #1 reports that DevContainer Features are installed into a running container instead of during the image build phase. The spec says Features should be installed during image construction so they become part of the image layer. Installing into a running container means:
- Features are lost if the container is rebuilt from the image
- Layer caching doesn't work (every `up` reinstalls features)
- Feature install scripts that need `RUN`-time capabilities fail

### Requirements

1. **Move Feature installation to image build** — Features should be installed by generating a Dockerfile that layers feature installation on top of the base image, then building that image before container creation.

2. **Dockerfile generation** — For each feature:
   - Download the feature archive from the OCI registry (this part already works)
   - Generate `RUN` instructions that execute the feature's `install.sh`
   - Pass feature options as environment variables per the Features spec

3. **Cache-friendly** — The generated Dockerfile should produce deterministic layers so Docker's build cache works. Same features + same options = cache hit.

4. **Preserve existing behavior for non-feature configs** — Configs without `features` should not get an extra build step.

### Acceptance Criteria

- [ ] Features are installed during image build, not in running container
- [ ] Feature options are passed correctly as env vars
- [ ] Rebuilding without config changes uses Docker cache (no reinstall)
- [ ] Configs without features skip the feature build step
- [ ] `cargo test` passes
- [ ] New tests: feature installation produces image layer, options forwarded, cache hit on rebuild

### Location

- Feature installer: `crates/core/src/feature_installer.rs`
- `up` command flow: `crates/deacon/src/commands/up/`

---

## Bead 5: Fix License and Cargo.toml Housekeeping

**Priority:** Low
**Risk:** None — metadata-only change

### Context

The Deacon roadmap and audit identified a mismatch: the project uses the MIT license (LICENSE file, README badge) but `Cargo.toml` lists `Apache-2.0`. This should be corrected for consistency and because crates.io publishing would surface this mismatch.

### Requirements

1. **Update `Cargo.toml` license field** — Change from `Apache-2.0` to `MIT` in all workspace members.

2. **Verify LICENSE file** — Ensure the MIT LICENSE file exists at workspace root and is correct.

3. **Verify README** — Confirm the MIT badge/text matches.

### Acceptance Criteria

- [ ] All `Cargo.toml` files specify `license = "MIT"`
- [ ] LICENSE file at workspace root contains MIT text
- [ ] `cargo package --list` (dry run) shows no license warnings
- [ ] `cargo clippy` clean, `cargo test` passes

### Location

- `Cargo.toml` (workspace root and any member `Cargo.toml` files)
- `LICENSE` file at project root

---

## Dependencies

```
Bead 1 (UID update)     — independent
Bead 2 (Compose profiles) — independent
Bead 3 (runArgs)         — independent
Bead 4 (Feature timing)  — independent
Bead 5 (License fix)     — independent
```

All beads are independent and can be implemented in any order. Suggested sequencing by impact: Bead 4 → Bead 1 → Bead 2 → Bead 3 → Bead 5.

## Constraints

- **Do not modify** the variable substitution engine — it's complete and well-tested
- **Do not modify** lifecycle command execution — SP-003/004/005 already shipped these fixes
- **Do not remove** any existing commands or change CLI flag signatures
- **Maintain** `unsafe_code = "forbid"` workspace policy
- **All changes** must pass `cargo clippy` and `cargo test` before commit
- **Rust edition:** 2021, async runtime: tokio
