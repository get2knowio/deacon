# Deacon MVP Roadmap: `up` and `exec` Commands

**Generated**: 2025-12-26
**Version**: 0.1.4
**Status**: MVP functional, iterations defined

---

## Executive Summary

**Good news: The core `up`/`exec` workflow is already functional.** Testing confirms:
- `deacon up` creates containers from `devcontainer.json`
- Lifecycle hooks execute (postCreateCommand confirmed)
- `deacon exec` runs commands in the container
- `remoteEnv` variables propagate correctly
- Working directory and user resolution work
- Container discovery by workspace folder works
- Exit code propagation works
- 2054 tests pass in the fast suite

**Remaining work**: Polish, edge cases, and advanced features (Compose, Features installation during up, dotfiles container-side installation).

---

## Current State Assessment

### What Works Today (Tested End-to-End)

| Feature | Status | Evidence |
|---------|--------|----------|
| `up` with image-based config | ✅ Working | Manual test + smoke tests |
| Lifecycle hooks (onCreate, postCreate, postStart, postAttach) | ✅ Working | `smoke_up_then_exec_traditional` |
| Container state management | ✅ Working | `.devcontainer-state/` written |
| `exec` with workspace discovery | ✅ Working | Manual test |
| `remoteEnv` injection | ✅ Working | Manual test (TEST_VAR accessible) |
| Working directory resolution | ✅ Working | `/workspace` confirmed |
| Exit code propagation | ✅ Working | `test_exec_exit_code_propagation` |
| TTY detection | ✅ Working | `test_exec_tty_detection` |
| `--env` flag for exec | ✅ Working | `test_exec_env_merges` |
| Container reuse (idempotency) | ✅ Working | `smoke_up_idempotent` |
| `down` command | ✅ Working | Manual test |
| JSON output mode | ✅ Working | UpResult JSON confirmed |
| Configuration resolution with extends | ✅ Working | `ConfigLoader::load_with_extends()` |
| GPU detection and mode | ✅ Working | 6 GPU test files |
| Prebuild mode | ✅ Working | 7 prebuild tests |
| Lifecycle recovery | ✅ Working | `up_lifecycle_recovery.rs` |
| Lockfile validation | ✅ Working | `up_lockfile_frozen.rs` |
| Merged configuration | ✅ Working | `up_merged_configuration.rs` |

### What Has Tests But May Need Verification

| Feature | Test File | Tests Passing |
|---------|-----------|---------------|
| Dotfiles installation | `up_dotfiles.rs` | 13 tests ignored (not in MVP) |
| Build options (cache, BuildKit) | `integration_up_build_options.rs` | Tests passing |
| Features during up | `integration_up_with_features.rs` | Tests passing |
| Host requirements | `integration_host_requirements.rs` | Tests passing |
| Port forwarding flags | `integration_port_forwarding.rs` | Tests passing |

### What Has Disabled Tests (Known Gaps)

| Feature | Test File | Disabled Tests | Blocking Task |
|---------|-----------|----------------|---------------|
| Compose profiles | `up_compose_profiles.rs` | 7 | T020 |
| Reconnect/expect-existing | `up_reconnect.rs` | 8 | T021, T023, T029 |
| Config resolution advanced | `up_config_resolution.rs` | 2 | T029 |

**Total disabled tests**: 17 (out of 2000+)

---

## MVP Definition (Publishable Now)

For a user whose workflow is `deacon up` + `deacon exec -- zsh`, the MVP must support:

### P0 - Must Have for MVP Release

| Capability | Status | Notes |
|------------|--------|-------|
| `up` from image-based devcontainer.json | ✅ Done | Working |
| Lifecycle hooks execution | ✅ Done | All phases work |
| `exec` with command execution | ✅ Done | Working |
| `exec` with interactive shell (zsh/bash) | ✅ Done | TTY allocation works |
| `remoteEnv` variable injection | ✅ Done | Verified |
| Working directory from config | ✅ Done | Verified |
| `remoteUser` support | ✅ Done | Verified (root fallback) |
| Container reuse on re-run | ✅ Done | Idempotent |
| `down` for cleanup | ✅ Done | Working |
| Exit code propagation | ✅ Done | Verified |
| `--workspace-folder` flag | ✅ Done | Primary discovery method |
| Error messages on failure | ✅ Done | Comprehensive error handling |

### P1 - Important but Can Ship Without

| Capability | Status | Notes |
|------------|--------|-------|
| Features installation during up | ⚠️ Partial | Config merges, installation needs verification |
| Dockerfile-based builds | ✅ Done | `build` command + up integration |
| `--id-label` container selection | ✅ Done | Tests passing |
| `--container-id` direct selection | ✅ Done | Tests passing |
| Dotfiles installation | ⚠️ Partial | Host-side works, container-side TODO |
| Secrets file handling | ⚠️ Partial | Parsing exists, redaction incomplete |
| `--skip-post-create` flag | ✅ Done | Tests passing |
| `--skip-non-blocking-commands` | ✅ Done | Tests passing |
| `userEnvProbe` modes | ✅ Done | All modes implemented |

### P2 - Nice to Have

| Capability | Status | Notes |
|------------|--------|-------|
| Docker Compose support | ⚠️ Partial | Basic works, profiles incomplete |
| `--expect-existing-container` | ❌ Not done | Validation logic missing |
| UID remapping | ⚠️ Partial | Flag exists, execution incomplete |
| Port forwarding | ⚠️ Partial | Flags exist, functionality deferred |
| `--shutdown` flag | ❌ Not done | Flag exists, not wired |

---

## MVP Validation Checklist

Before release, verify these scenarios work:

```bash
# Scenario 1: Basic up/exec workflow
mkdir -p /tmp/mvp-test/.devcontainer
cat > /tmp/mvp-test/.devcontainer/devcontainer.json << 'EOF'
{
  "name": "MVP Test",
  "image": "mcr.microsoft.com/devcontainers/base:ubuntu",
  "workspaceFolder": "/workspace",
  "remoteEnv": { "GREETING": "Hello MVP" },
  "postCreateCommand": "echo 'Container ready!'"
}
EOF

# Test up
deacon up --workspace-folder /tmp/mvp-test

# Test exec with command
deacon exec --workspace-folder /tmp/mvp-test -- echo "It works!"

# Test exec with shell (interactive)
deacon exec --workspace-folder /tmp/mvp-test -- bash

# Test remoteEnv
deacon exec --workspace-folder /tmp/mvp-test -- printenv GREETING

# Test cleanup
deacon down --workspace-folder /tmp/mvp-test
```

```bash
# Scenario 2: Lifecycle hooks
cat > /tmp/mvp-test/.devcontainer/devcontainer.json << 'EOF'
{
  "name": "Lifecycle Test",
  "image": "alpine:3.19",
  "workspaceFolder": "/workspace",
  "onCreateCommand": "echo 'onCreate'",
  "postCreateCommand": "echo 'postCreate'",
  "postStartCommand": "echo 'postStart'"
}
EOF

deacon up --workspace-folder /tmp/mvp-test
# Verify all lifecycle hooks executed
```

---

## Iteration Plan: MVP to Full Support

### Iteration 0: MVP Release (Current State)

**Goal**: Ship what works today for the basic `up`/`exec` workflow.

**Scope**:
- Image-based containers ✅
- All lifecycle hooks ✅
- exec with commands and shells ✅
- remoteEnv ✅
- Container reuse ✅

**Test Gate**: `make test-nextest-fast` (2054 tests pass)

**Deliverable**: v0.2.0 release with working up/exec

---

### Iteration 1: Feature Installation & Dotfiles

**Goal**: Complete the "enhanced image" workflow.

**Tasks**:

| Task | File | Status | Work Required |
|------|------|--------|---------------|
| T016: Feature installation in up flow | `commands/up/container.rs` | Partial | Wire `check_for_disallowed_features()` + BuildKit build |
| T015: Container-side dotfiles | `commands/up/dotfiles.rs` | Partial | Implement container exec for dotfiles install |
| Verify feature tests | `integration_up_with_features.rs` | Tests exist | Manual verification |

**Acceptance Criteria**:
- Features declared in config are installed during up
- Dotfiles repository clones and runs install command
- `up_dotfiles.rs` tests stay green

**Effort**: Medium (2-3 days)

---

### Iteration 2: Compose Support

**Goal**: Support Docker Compose-based devcontainer configurations.

**Tasks**:

| Task | File | Status | Work Required |
|------|------|--------|---------------|
| T020: Mount conversion | `commands/up/compose.rs` | Missing | Implement bind-to-volume conversion |
| T020: Profile selection | `commands/up/compose.rs` | Missing | Parse and apply profiles |
| T020: .env COMPOSE_PROJECT_NAME | `commands/up/compose.rs` | Missing | Read .env file |
| Enable compose tests | `up_compose_profiles.rs` | 7 disabled | Implement and enable |

**Acceptance Criteria**:
- `deacon up` works with `docker-compose.yml` references
- Profiles are respected
- All 7 disabled compose tests pass

**Effort**: Medium-High (3-5 days)

---

### Iteration 3: Advanced Container Discovery

**Goal**: Complete `--id-label` and `--expect-existing-container` workflows.

**Tasks**:

| Task | File | Status | Work Required |
|------|------|--------|---------------|
| T029: ID label discovery | `commands/up/mod.rs:951` | Stub | Wire discovery into main flow |
| T029: Disallowed features list | `commands/up/mod.rs:914` | Empty list | Implement actual list |
| T023: Expect-existing fast-fail | `commands/up/args.rs` | Flag only | Add validation before docker ops |
| Enable reconnect tests | `up_reconnect.rs` | 8 disabled | Implement and enable |

**Acceptance Criteria**:
- `--id-label name=value` finds existing containers
- `--expect-existing-container` fails fast if not found
- All 8 disabled reconnect tests pass

**Effort**: Medium (2-3 days)

---

### Iteration 4: Secrets & Security

**Goal**: Complete secrets handling and security options.

**Tasks**:

| Task | File | Status | Work Required |
|------|------|--------|---------------|
| T021: Secrets file loading | `commands/shared/` | Partial | Parse KEY=value files |
| T021: Redaction in logs | Multiple | Partial | Integrate with SecretRegistry |
| T017: UID update | `commands/up/container.rs` | Partial | usermod/groupmod in container |
| T017: Security options | `commands/up/container.rs` | Partial | Wire capAdd, securityOpt, init |

**Acceptance Criteria**:
- `--secrets-file` values never appear in logs or JSON output
- Security options applied to container runtime
- Secrets tests pass

**Effort**: Medium (2-3 days)

---

### Iteration 5: Polish & Parity

**Goal**: Full spec compliance and reference CLI parity.

**Tasks**:

| Task | Status | Work Required |
|------|--------|---------------|
| Wire `--include-configuration` flag | Flag exists | Add to JSON output |
| Wire `--include-merged-configuration` | Flag exists | Add to JSON output |
| Implement `--shutdown` | Flag exists | Wire to container stop |
| Implement `--container-name` | Flag exists | Wire to docker create |
| Fix parity test failure | 1 failing | `remote_env_validation_message_matches` |
| Enable all disabled tests | 17 disabled | Complete implementations |

**Acceptance Criteria**:
- All CLI flags functional
- All 2071+ tests pass (including currently disabled)
- Parity tests pass against reference CLI

**Effort**: Medium (2-3 days)

---

## Test Coverage Summary

| Category | Total | Passing | Disabled/Failing |
|----------|-------|---------|------------------|
| Fast tests | 2054 | 2054 | 0 |
| Docker tests | ~597 | 256 | 1 failing, 339 not run (due to fail-fast) |
| Smoke tests | ~20 | 20 | 0 |
| Up-specific | ~200 | ~183 | 17 disabled |
| Exec-specific | ~40 | ~40 | 0 |

---

## File Reference: Key Implementation Locations

### UP Command
- Entry point: `crates/deacon/src/commands/up/mod.rs`
- Args: `crates/deacon/src/commands/up/args.rs`
- Container workflow: `crates/deacon/src/commands/up/container.rs`
- Compose workflow: `crates/deacon/src/commands/up/compose.rs`
- Lifecycle: `crates/deacon/src/commands/up/lifecycle.rs`
- Dotfiles: `crates/deacon/src/commands/up/dotfiles.rs`
- Features: `crates/deacon/src/commands/up/features_build.rs`

### EXEC Command
- Implementation: `crates/deacon/src/commands/exec.rs` (single file, 1164 lines)

### Shared Infrastructure
- Config loading: `crates/deacon/src/commands/shared/config_loader.rs`
- Env/user resolution: `crates/deacon/src/commands/shared/env_user.rs`
- Terminal handling: `crates/deacon/src/commands/shared/terminal.rs`

### Core Library
- Container env probe: `crates/core/src/container_env_probe.rs`
- Lifecycle execution: `crates/core/src/container_lifecycle.rs`
- Config resolution: `crates/core/src/config.rs`

### Specs
- UP spec: `docs/subcommand-specs/up/SPEC.md`
- EXEC spec: `docs/subcommand-specs/completed-specs/exec/SPEC.md`

---

## Recommended Release Strategy

### Phase 1: Soft Launch (v0.2.0-beta)
- Release current state with documented limitations
- Target early adopters for feedback
- Focus: "It works for simple image-based configs"

### Phase 2: Feature Complete (v0.2.0)
- Complete Iterations 1-2 (Features + Compose)
- Enable all up-related tests
- Target: General availability

### Phase 3: Production Ready (v0.3.0)
- Complete Iterations 3-5 (Discovery + Security + Polish)
- Full spec compliance
- All tests passing
- Target: Enterprise/production use

---

## Sources

- [Dev Container Specification](https://containers.dev/implementors/spec/)
- [Dev Container JSON Reference](https://containers.dev/implementors/json_reference/)
- [Dev Container Features Reference](https://containers.dev/implementors/features/)
- [Dev Container CLI Reference Implementation](https://github.com/devcontainers/cli)
- Internal: `specs-completed/001-up-gap-spec/TODO-ANALYSIS.md`
