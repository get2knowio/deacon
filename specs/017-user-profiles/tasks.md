---
description: "Task list for User-Scoped Profiles for Host Settings"
---

# Tasks: User-Scoped Profiles for Host Settings

**Input**: Design documents from `/specs/017-user-profiles/`
**Prerequisites**: plan.md, spec.md, research.md, data-model.md, contracts/cli-profile.md, quickstart.md

**Tests**: INCLUDED — Constitution Principle VII mandates spec tests, and the spec defines
acceptance scenarios + 10 contract scenarios (C1–C10 in `contracts/cli-profile.md`).

**Organization**: Grouped by user story (US1 P1 → US2 P2 → US3 P3), each an independently
testable increment. Precedence ladder and merge reuse per plan.md / research.md.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependency on an incomplete task)
- **[Story]**: US1 / US2 / US3 (setup, foundational, and polish tasks carry no story label)

## Path Conventions

Rust workspace: library `crates/core/`, binary `crates/deacon/`, integration tests
`crates/deacon/tests/`, nextest config `.config/nextest.toml`. All paths below are repo-root
relative.

---

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: Prepare test wiring. No new dependencies — `indexmap`, `serde`, `clap`,
`tracing`, `thiserror` are all already present (see plan.md Technical Context).

- [X] T001 Register the new integration test binary `integration_profiles` in `.config/nextest.toml` as a non-docker `fs-heavy` suite: add override rules to ALL profiles (`[profile.default]`, `[profile.dev-fast]`, `[profile.full]`/`ci`) so it runs in the fast loop (it is not docker-gated). Follow the fs-heavy grouping pattern already used for filesystem-only integration binaries.

**Checkpoint**: nextest knows about the profiles test binary (added in Phase 3).

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Data structures, error type, and the ordered-override plumbing that every user
story builds on. **All tasks here are behavior-preserving** — with no profiles configured and
an empty override list, runtime behavior is byte-for-byte unchanged (FR-024).

**⚠️ CRITICAL**: No user story work can begin until this phase is complete.

- [X] T002 Add the `MergeConfigPaths` untagged single-or-list enum (`Single(PathBuf)` / `Multiple(Vec<PathBuf>)`) plus a normalize-to-ordered-`Vec` helper in `crates/core/src/settings.rs`, mirroring `AppPort` (`crates/core/src/config.rs:215`). Add unit tests: a bare string and a string array both deserialize; order is preserved (FR-012).
- [X] T003 Extend `Settings` in `crates/core/src/settings.rs` with `#[serde(rename = "mergeConfig")] override_config: Option<MergeConfigPaths>`, `#[serde(default)] profiles: IndexMap<String, Profile>`, `#[serde(rename = "defaultProfile")] default_profile: Option<String>`, and `#[serde(flatten)] extra: serde_json::Map<String, Value>`; add the `Profile` struct (`mergeConfig`, `hostCa`, `browser`, `#[serde(flatten)] extra`). Keep `#[serde(default)]` / no `deny_unknown_fields`. Add unit tests: unknown top-level & unknown per-profile keys tolerated and round-trip; empty `profiles` equals today's `Default`; declaration order preserved.
- [X] T004 Add `ProfileError` (`thiserror`) in `crates/core/src/settings.rs` with variants `UnknownProfile { name: String, available: Vec<String> }` and `MissingFragment { profile: Option<String>, path: PathBuf }`, and a `From<ProfileError>` mapping into `DeaconError` (mirror the existing `settings.rs` error wrapping). Unit test: `UnknownProfile` Display lists available names in order; `MissingFragment` names the owning profile (or "root").
- [X] T005 [P] Generalize `ConfigLoader::load_with_overrides_and_substitution` in `crates/core/src/config.rs` from `override_config_path: Option<&Path>` to an ordered `override_config_paths: &[&Path]` (lowest→highest), pushing each onto the extends-resolved chain in order (research.md Decision 3). Update the two in-repo call sites `crates/core/src/config.rs:4016` and `crates/core/tests/integration_override_secrets.rs:62`. Verify existing override behavior is unchanged for the 0/1-path case.
- [X] T006 Update the shared loader in `crates/deacon/src/commands/shared/config_loader.rs`: add `settings_override_paths: &'a [PathBuf]` to `ConfigLoadArgs`; in `load_config` build the final ordered override list as `[settings_override_paths…, override_config_path?]` and pass it to the generalized core function (T005); adapt the "missing base config" fallback (`config_loader.rs:84-88`) to promote the **lowest-precedence** override to base and stack the rest. Update ALL existing `ConfigLoadArgs { … }` call sites to pass `settings_override_paths: &[]` (regression-safe: empty list = today's behavior).

**Checkpoint**: Types, errors, and n-layer override plumbing exist; build is green; runtime
behavior unchanged (no `--profile`, no settings profiles configured).

---

## Phase 3: User Story 1 - Select a named profile per run (Priority: P1) 🎯 MVP

**Goal**: A developer selects a named profile with `--profile <NAME>` (or `DEACON_PROFILE`)
and its `mergeConfig` fragment(s) layer onto the project config; an unselected profile's
overrides never apply; unknown names fail fast.

**Independent Test**: With two profiles referencing distinct fragments, run
`read-configuration --profile dev` vs `--profile agent` and confirm the resolved config
reflects only the selected profile; `--profile nope` errors listing available names.

### Tests for User Story 1

- [X] T007 [P] [US1] Add integration tests in `crates/deacon/tests/integration_profiles.rs` covering contract scenarios C1 (agent applied, dev excluded), C5 (unknown `--profile` errors listing `dev, agent`, non-zero exit), C8 (a profile `mergeConfig` list applies in order and sits below `--override-config`), and C9 (empty profile `{}` is valid, applies nothing). Use `--user-data-folder <tmp>` + `--workspace-folder <fixture>`; assert on `read-configuration` JSON. Register nothing new (binary from T001).
- [X] T008 [P] [US1] Add unit tests for `Settings::resolve` (explicit selection) in `crates/core/src/settings.rs`: selecting a defined profile returns its ordered override paths (root override first, then profile); unknown selected name → `UnknownProfile`; a nonexistent fragment path → `MissingFragment` naming the profile; relative paths resolve against the settings dir, absolute accepted as-is (FR-021).
- [X] T030 [P] [US1] Add a security/defense-in-depth integration test in `crates/deacon/tests/integration_profiles.rs` (scenario C11): a workspace `devcontainer.json` containing a `profiles`/`defaultProfile` key does NOT get treated as a profile source — profiles are read only from `--user-data-folder`; `--profile <name-only-in-workspace>` still errors as unknown (FR-019).

### Implementation for User Story 1

- [X] T009 [US1] Implement `Settings::resolve(selected: Option<&str>, settings_dir: &Path) -> Result<ResolvedSettings, ProfileError>` in `crates/core/src/settings.rs`, and the `ResolvedSettings { active_profile, override_paths, host_ca, browser }` struct. This task implements: explicit-selection resolution (defaultProfile fallback is US2; scalar computation is US3 — for now `host_ca`/`browser` may echo root), ordered override-path assembly (root `mergeConfig` then active profile `mergeConfig`), path resolution vs `settings_dir` + existence check (`MissingFragment`), and `UnknownProfile` on unknown `selected`. (Depends on T002–T004.)
- [X] T010 [P] [US1] Add the global `--profile <NAME>` flag (`#[arg(long, global = true, value_name = "NAME", env = "DEACON_PROFILE")]`) to the `Cli` struct and copy it into `CliContext` in `crates/deacon/src/cli.rs` (same tier as `override_config`). Wire it into `dispatch()` so it is available to command handlers.
- [X] T011 [US1] Create the shared glue helper `crates/deacon/src/commands/shared/profile.rs` (module-registered in `commands/shared/mod.rs`): `resolve_active_profile(user_data_folder, selected) -> Result<ResolvedSettings>` that loads `Settings`, calls `Settings::resolve` with the settings-file directory, emits a `tracing` diagnostic on stderr naming the applied profile + source when `active_profile.is_some()` (FR-009b), and maps `ProfileError` to `DeaconError`/`anyhow`. (Depends on T009.)
- [X] T012 [US1] Wire `up` to the profile helper: in the `up` handler (`crates/deacon/src/commands/up/…` / its `cli.rs` dispatch at ~1347) call `resolve_active_profile`, and pass `settings_override_paths: &resolved.override_paths` into `ConfigLoadArgs`. (Depends on T006, T011.)
- [X] T013 [P] [US1] Wire `read-configuration` to the profile helper the same way (`crates/deacon/src/commands/read_configuration.rs` + dispatch ~1620), passing the resolved override paths into `ConfigLoadArgs`. (Depends on T006, T011.)
- [X] T014 [P] [US1] Wire `build` to the profile helper (`crates/deacon/src/commands/build/mod.rs` + dispatch ~1511), passing resolved override paths into its `ConfigLoadArgs`. (Depends on T006, T011.)
- [X] T015 [US1] Wire `outdated` to the profile helper (`crates/deacon/src/commands/outdated.rs`, `resolve_config_path` ~388): adapt its bespoke single-`ConfigLocation` override handling to prepend the resolved settings override paths beneath any `--override-config`, honoring the same precedence ladder (FR-011/FR-014). (Depends on T011; may reuse the generalized loader.)

**Checkpoint**: `--profile <name>` selects a profile across `up`/`read-configuration`/`build`/
`outdated`; overrides layer correctly; unknown names fail fast. MVP is demoable.

---

## Phase 4: User Story 2 - Apply a default profile automatically (Priority: P2)

**Goal**: `defaultProfile` in `settings.json` is applied on a bare invocation; `--profile`
overrides it; with profiles but no default, a bare invocation applies nothing (three-state
model); a dangling `defaultProfile` fails fast.

**Independent Test**: Set `defaultProfile: dev`; a bare `read-configuration` applies `dev`;
`--profile agent` overrides; removing `defaultProfile` makes a bare run apply nothing; a
`defaultProfile` naming an undefined profile errors at load.

### Tests for User Story 2

- [X] T016 [P] [US2] Add integration tests in `crates/deacon/tests/integration_profiles.rs` for C2 (bare run applies `defaultProfile`), C3 (profiles present, no default → nothing applied, identical to no-profiles), C4 (no `profiles` key → unchanged behavior), and C6 (dangling `defaultProfile` errors at load listing available names).
- [X] T017 [P] [US2] Add unit tests for the three-state selection model in `crates/core/src/settings.rs`: `selected=None` + `default_profile=Some(defined)` → applied; `selected=Some` overrides the default; `default_profile=None` → no active profile; `default_profile=Some(undefined)` → `UnknownProfile`.

### Implementation for User Story 2

- [X] T018 [US2] Extend `Settings::resolve` selection logic in `crates/core/src/settings.rs` to fall back to `self.default_profile` when `selected` is `None` (FR-007/FR-008), and raise `UnknownProfile` for a dangling default (FR-016). No change needed at call sites — the glue already passes `selected = --profile` (T011) and `resolve` now honors the default. (Depends on T009.)

**Checkpoint**: US1 + US2 both work; the full selection precedence (`--profile` > env >
`defaultProfile` > none) and three-state behavior hold.

---

## Phase 5: User Story 3 - Override personal machine settings per profile (Priority: P3)

**Goal**: A selected profile may override the root scalar settings (`hostCa`, `browser`);
an unset scalar inherits the root value; CLI flag / env still wins over the profile value.

**Independent Test**: Root `browser: firefox`; profile `agent` sets `browser: none`;
selecting `agent` yields effective browser `none`, selecting `dev` (unset) yields `firefox`;
`DEACON_BROWSER` set beats the profile value.

### Tests for User Story 3

- [X] T019 [P] [US3] Add unit tests for effective-scalar resolution in `crates/core/src/settings.rs` (C7): profile value overrides root when set; unset profile inherits root; no active profile → root value (== today).
- [X] T020 [P] [US3] Add an integration test (C10) asserting env precedence over the profile value for the browser setting (flag/env > profile > root), in `crates/deacon/tests/integration_profiles.rs` (or a focused unit test at the reader if end-to-end env is impractical).

### Implementation for User Story 3

- [X] T021 [US3] Finalize `ResolvedSettings.host_ca`/`browser` computation in `Settings::resolve` (`crates/core/src/settings.rs`): effective = active-profile value `.or(root value)` (FR-013). (Depends on T009.)
- [X] T022 [US3] Make the host-CA reader profile-aware: in `resolve_host_ca_activation_cli` (`crates/deacon/src/cli.rs` ~1048) consume `ResolvedSettings.host_ca` (via the glue) instead of raw `Settings::load(...).host_ca`, preserving the existing flag/env-wins precedence above the resolved value. (Depends on T011, T021.)
- [X] T023 [US3] Make the browser reader profile-aware and thread the selected profile to the daemon (research.md Decision 5): in `crates/deacon/src/commands/up/forward.rs` (~125) use the resolved `browser`; pass the active profile name explicitly to the port-forward daemon process so it re-resolves settings with that profile rather than relying on ambient `DEACON_PROFILE` inheritance. (Depends on T011, T021.)
- [X] T029 [US3] Implement the `browser = "none"` disable sentinel (FR-013a, research.md Decision 8) in `crate::browser::resolve_browser` (`crates/core/src/browser.rs:29`): a resolved `browser` value equal to `"none"` (case-insensitive), from either root or profile, suppresses the launch instead of being treated as a program name; existing `DEACON_BROWSER` > `browser` > OS-default precedence is preserved. Add unit tests: `"none"`/`"None"` disables; a real program name still launches; unset still falls back. (Depends on T021; independent of T023 but exercised by it.)

**Checkpoint**: All three user stories independently functional; scalar precedence holds
everywhere including the out-of-process forwarder.

---

## Phase 6: Polish & Cross-Cutting Concerns

**Purpose**: Docs, examples, and the full quality gate.

- [X] T024 [P] Update `docs/FILESYSTEM_ARTIFACTS.md` — the `settings.json` row now mentions profiles/defaultProfile/mergeConfig (still read-only), and note the fragment paths resolve relative to the settings folder.
- [X] T025 [P] Add/refresh user docs for the feature (settings/profiles section in the relevant `docs/` or README settings reference), reusing `quickstart.md` content; keep terminology aligned with `contracts/cli-profile.md`.
- [X] T026 Run `quickstart.md` validation end-to-end (the throwaway `--user-data-folder` recipe) and confirm C1–C10 behaviors manually.
- [X] T027 Full quality gate: `cargo fmt --all -- --check`, `cargo clippy --all-targets --all-features -- -D warnings`, and `make test-nextest` (verify `integration_profiles` runs in the fast loop and no nextest race). Fix any drift.

---

## FR-020a Host-Hook Trust Provenance (implemented — no longer deferred)

Originally scoped as a phased deferral (research.md Decision 6); implemented in full in this
delivery. The spec is complete — nothing remains deferred.

- [X] T028 Implement FR-020a "trust follows author" for host-side hooks introduced by a profile fragment.
  - **Mechanism**: `ConfigMerger::merge_configs` collapses per-field provenance, so the origin of the *effective* `initializeCommand` is recomputed from the ordered override chain in `initialize_command_author_trusted` (`crates/deacon/src/commands/up/lifecycle.rs`). Merge is last-writer-wins, so the winner is the highest-precedence layer that sets the field: the CLI `--override-config` (highest) and the workspace base config are never owner-authored; a settings-sourced fragment is owner-authored iff `override_authored_in_user_data` (`crates/deacon/src/commands/shared/profile.rs`) finds it resolved inside the user-data folder. `execute_initialize_command` takes an `author_trusted` flag; when set it bypasses `enforce_host_trust`, otherwise the workspace-trust gate stays on. Wired at both host-hook call sites (`up/container.rs`, `up/compose.rs`). No new on-disk state — provenance is threaded in-memory.
  - **Acceptance (met)**: an `initializeCommand` whose effective value originates from a user-data-folder profile fragment runs on an untrusted workspace WITHOUT prompting; the same command originating from the workspace config, or from a profile fragment referenced by an absolute path OUTSIDE the user-data folder, remains gated. Both branches are covered by hermetic unit tests in `lifecycle.rs` (`trust_provenance_tests`: `author_trusted_command_bypasses_gate_on_untrusted_workspace`, `non_author_trusted_command_is_gated_on_untrusted_workspace`, plus the six provenance cases) and an end-to-end gated `up` test (`integration_profiles::t028_outside_user_data_fragment_initialize_command_is_gated`).

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (Phase 1)**: no dependencies.
- **Foundational (Phase 2)**: depends on Setup; BLOCKS all user stories. T005 is `[P]` with
  T002–T004 (different file); T006 depends on T005.
- **User Stories (Phase 3–5)**: all depend on Foundational.
  - US1 (P1) is the MVP. US2 and US3 are thin extensions of `Settings::resolve` (T018, T021)
    plus their own tests/wiring — each depends only on US1's T009, not on each other, so US2
    and US3 can proceed in parallel after US1.
- **Polish (Phase 6)**: after the desired stories are complete.
- **T028 (FR-020a trust provenance)**: implemented (no longer deferred); depends on the
  resolved override paths from T009/T011 and the host-hook gate in `up/lifecycle.rs`.

### User Story Dependencies

- **US1 (P1)**: needs Foundational only. Delivers the feature's core value on its own.
- **US2 (P2)**: needs T009 (US1 resolver). Independently testable via `defaultProfile`.
- **US3 (P3)**: needs T009 (US1 resolver). Independently testable via scalar overrides.

### Within Each User Story

- Tests are written to fail first, then implementation.
- `Settings::resolve` (T009) precedes the glue (T011) precedes command wiring (T012–T015).
- Core (`settings.rs`, `config.rs`) before CLI wiring.

### Parallel Opportunities

- Foundational: T005 `[P]` runs alongside T002–T004.
- US1 tests: T007, T008, T030 `[P]` together (integration + unit + security). US1 wiring:
  T013, T014 `[P]` (different command files) after the glue; T012 and T015 touch broader
  dispatch/bespoke paths (sequence with care).
- US2 and US3 (after US1): T016/T017 `[P]`, T019/T020 `[P]`; T029 (browser.rs) `[P]` with the
  US3 settings.rs work (different file); the two stories can run in parallel.
- Polish: T024, T025 `[P]`.

---

## Parallel Example: User Story 1

```bash
# Tests for US1 together:
Task: "Integration tests C1/C5/C8/C9 in crates/deacon/tests/integration_profiles.rs"   # T007
Task: "Unit tests for Settings::resolve explicit selection in crates/core/src/settings.rs" # T008

# After the glue (T011), wire the shared-loader commands in parallel:
Task: "Wire read-configuration in crates/deacon/src/commands/read_configuration.rs"  # T013
Task: "Wire build in crates/deacon/src/commands/build/mod.rs"                         # T014
```

---

## Implementation Strategy

### MVP First (User Story 1 only)

1. Phase 1 Setup → Phase 2 Foundational (behavior-preserving plumbing).
2. Phase 3 US1 → **STOP and VALIDATE**: `--profile` selects/excludes/errors across the four
   commands. This is a shippable MVP.

### Incremental Delivery

1. Foundation ready (Phases 1–2).
2. US1 → test independently → demo (MVP).
3. US2 (`defaultProfile`) → test → demo.
4. US3 (per-profile scalars) → test → demo.
5. Polish; keep the FR-020a deferral (T028) tracked until scheduled.

## Notes

- `[P]` = different files, no dependency on an incomplete task.
- Every Foundational task is regression-safe: empty profiles + empty override list = today.
- Windows lane: keep path assertions separator-agnostic; the daemon-threading (T023) is
  Unix-relevant (the forward daemon is Unix-only) — gate its test `#[cfg(unix)]` if needed.
