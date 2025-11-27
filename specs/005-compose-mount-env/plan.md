# Implementation Plan: Compose mount & env injection

**Branch**: `005-compose-mount-env` | **Date**: 2025-11-26 | **Spec**: specs/005-compose-mount-env/spec.md
**Input**: Feature specification from `specs/005-compose-mount-env/spec.md`

## Summary

Apply CLI-provided mounts (including mountWorkspaceGitRoot) and remote environment entries directly into the primary compose service during the `up` subcommand without relying on temporary compose overrides, while preserving external volume references and honoring compose profiles, env-files, and project naming. Technical focus: extend compose project/command handling to inject mounts/env for the targeted service, retain external volumes unchanged, and keep naming/profile/env-file semantics intact.

## Technical Context
**Language/Version**: Rust (stable, 2021 edition; rust-toolchain pins stable)  
**Primary Dependencies**: clap, serde/serde_json, anyhow/thiserror, tracing, tokio, compose/exec helpers in crates/core and crates/deacon  
**Storage**: N/A (compose config files and runtime Docker resources)  
**Testing**: make test-nextest-fast for fast loop; make test-nextest before PR; docker-focused flows use make test-nextest-docker/smoke as needed  
**Target Platform**: Linux/macOS hosts with Docker/Compose available (devcontainer-style environments)  
**Project Type**: CLI workspace (multi-crate Rust)  
**Performance Goals**: Zero additional startup delay beyond compose invocation; mount/env injection must not add extra compose runs  
**Constraints**: No temp override files; preserve compose profiles/env-files/project naming; avoid mutating external volumes; no silent fallbacks  
**Scale/Scope**: Applies to primary-service lifecycle for `up`; typical multi-service dev projects with external volumes and env-files

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

- Spec-parity: Feature spec read and used as source of truth; no divergence planned.
- Keep build green: Plan uses nextest targets and fmt/clippy cadence; no skips allowed.
- No silent fallbacks: Injection failures will surface errors instead of ignoring mounts/env.
- Observability/output: JSON/text separation unaffected; logging to stderr maintained.
- Testing completeness: Plan includes targeted tests for mounts/env, external volumes, profiles/env-files naming; nextest grouping to be respected.
- Shared helpers: Will reuse compose/project/remote-env helpers; avoid bespoke per-subcommand logic.
- Safety/Rust hygiene: No unsafe; error handling via thiserror/anyhow with context.

**Post-Design Check (after Phase 1)**: Confirmed applicability of shared helpers and spec parity across compose mount/env injection; no constitution violations introduced.

## Project Structure

### Documentation (this feature)

```text
specs/[###-feature]/
├── plan.md              # This file (/speckit.plan command output)
├── research.md          # Phase 0 output (/speckit.plan command)
├── data-model.md        # Phase 1 output (/speckit.plan command)
├── quickstart.md        # Phase 1 output (/speckit.plan command)
├── contracts/           # Phase 1 output (/speckit.plan command)
└── tasks.md             # Phase 2 output (/speckit.tasks command - NOT created by /speckit.plan)
```

### Source Code (repository root)
<!--
  ACTION REQUIRED: Replace the placeholder tree below with the concrete layout
  for this feature. Delete unused options and expand the chosen structure with
  real paths (e.g., apps/admin, packages/something). The delivered plan must
  not include Option labels.
-->

```text
specs/005-compose-mount-env/
├── plan.md
├── research.md
├── data-model.md
├── quickstart.md
└── contracts/

crates/
├── core/            # shared helpers (compose, config, env handling)
├── deacon/          # CLI binary logic (subcommands incl. up)
└── ...              # additional supporting crates

docs/
└── CLI-SPEC.md      # authoritative behavior spec

.config/nextest.toml # test grouping and overrides
```

**Structure Decision**: Use existing multi-crate Rust CLI layout (crates/core + crates/deacon) with spec artifacts under `specs/005-compose-mount-env/`.

## Complexity Tracking

> **Fill ONLY if Constitution Check has violations that must be justified**

| Violation | Why Needed | Simpler Alternative Rejected Because |
|-----------|------------|-------------------------------------|
| _None_ |  |  |

## Phase Outputs

- Phase 0 research: specs/005-compose-mount-env/research.md (decisions on injection scope, precedence, external volumes, Git root handling, profiles/env-files/project naming)
- Phase 1 design: specs/005-compose-mount-env/data-model.md, specs/005-compose-mount-env/contracts/up.yaml, specs/005-compose-mount-env/quickstart.md
- Agent context: updated via `.specify/scripts/bash/update-agent-context.sh codex`
- Post-design constitution check: no violations identified
