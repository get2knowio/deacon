# Feature Specification: Named Config Folder Search

**Feature Branch**: `014-named-config-search`
**Created**: 2026-02-21
**Status**: Draft
**Input**: User description: "Add support for the third devcontainer.json search location: named config folders inside `.devcontainer/`"

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Single Named Config Discovery (Priority: P1)

A developer working in a monorepo has a single named configuration at `.devcontainer/python/devcontainer.json` (with no config at `.devcontainer/devcontainer.json` or `.devcontainer.json`). When they run `deacon up` without specifying `--config`, the tool automatically discovers and uses the named config.

**Why this priority**: This is the core capability — without automatic discovery of named configs, the feature has no value. Most monorepo users start with a single named config before adding more.

**Independent Test**: Can be fully tested by creating a workspace with only `.devcontainer/python/devcontainer.json` and running config discovery. Delivers value by enabling named config projects to work without explicit `--config` flags.

**Acceptance Scenarios**:

1. **Given** a workspace with only `.devcontainer/myconfig/devcontainer.json`, **When** the user runs `deacon up` (or any config-consuming command) without `--config`, **Then** the tool discovers and uses that named config automatically.
2. **Given** a workspace with `.devcontainer/myconfig/devcontainer.json`, **When** the user runs `deacon read-configuration`, **Then** the output reflects the config from the named subfolder and includes the correct `configFilePath`.
3. **Given** a workspace with no config files at any of the three search locations, **When** the user runs `deacon up`, **Then** the tool reports a clear "no configuration found" error.

---

### User Story 2 - Existing Search Locations Still Work (Priority: P1)

A developer with an existing project that uses `.devcontainer/devcontainer.json` or `.devcontainer.json` continues to have their configuration discovered automatically, with no change in behavior.

**Why this priority**: Regression protection is equally critical to new functionality. Breaking existing users would be unacceptable.

**Independent Test**: Can be tested by running config discovery against workspaces that use the two existing search locations and verifying identical behavior to the current implementation.

**Acceptance Scenarios**:

1. **Given** a workspace with `.devcontainer/devcontainer.json`, **When** the user runs config discovery, **Then** that file is found (same as today).
2. **Given** a workspace with only `.devcontainer.json` at the root, **When** the user runs config discovery, **Then** that file is found (same as today).
3. **Given** a workspace with both `.devcontainer/devcontainer.json` and `.devcontainer/python/devcontainer.json`, **When** the user runs config discovery without `--config`, **Then** `.devcontainer/devcontainer.json` wins (higher priority per spec).

---

### User Story 3 - Multiple Named Configs Require Explicit Selection (Priority: P2)

A monorepo developer has multiple named configurations (e.g., `python/`, `node/`, `rust/` under `.devcontainer/`). When they run a command without specifying which config to use, the tool reports an error listing the available configs and asks them to select one with `--config`.

**Why this priority**: This is the multi-config disambiguation path. It's important for the full monorepo experience but less common than the single-config case.

**Independent Test**: Can be tested by creating a workspace with multiple named config folders and verifying the error message lists all available configs.

**Acceptance Scenarios**:

1. **Given** a workspace with `.devcontainer/python/devcontainer.json` and `.devcontainer/node/devcontainer.json` (and no higher-priority configs), **When** the user runs `deacon up` without `--config`, **Then** the tool returns an error listing both available configs and instructing the user to specify `--config`.
2. **Given** the same multi-config workspace, **When** the user runs `deacon up --config .devcontainer/python/devcontainer.json`, **Then** the tool uses the specified config without error.

---

### User Story 4 - Explicit Config Path Override (Priority: P2)

A developer explicitly specifies a config path with `--config` to select a particular named configuration, bypassing automatic discovery entirely.

**Why this priority**: Explicit selection is the escape hatch for ambiguous cases and automation/CI pipelines that need deterministic config selection.

**Independent Test**: Can be tested by providing `--config` pointing to a specific named config and verifying it is used regardless of what other configs exist.

**Acceptance Scenarios**:

1. **Given** a workspace with multiple named configs, **When** the user runs `deacon up --config .devcontainer/rust/devcontainer.json`, **Then** the tool uses exactly that config file.
2. **Given** a workspace with `.devcontainer/devcontainer.json` (higher priority) and `.devcontainer/python/devcontainer.json`, **When** the user runs `deacon up --config .devcontainer/python/devcontainer.json`, **Then** the explicitly specified named config is used, not the higher-priority default.
3. **Given** `--config` points to a non-existent file, **When** the user runs any command, **Then** the tool returns a clear error indicating the specified config file was not found.

---

### Edge Cases

- What happens when `.devcontainer/` contains subdirectories that do NOT have a `devcontainer.json`? They are silently ignored during enumeration.
- What happens when `.devcontainer/` contains files (not directories) alongside subdirectories? Only directories are enumerated; files are ignored.
- What happens when a subdirectory name contains special characters (spaces, unicode)? The tool handles any valid directory name the OS supports.
- What happens when `.devcontainer/` itself does not exist? The tool falls back to checking `.devcontainer.json` at root (existing behavior), then reports no config found.
- What happens when a subdirectory contains a `devcontainer.json` that is invalid JSON? Discovery finds the file successfully; the parse error surfaces later during config loading, not during search.
- What happens with nested subdirectories (e.g., `.devcontainer/a/b/devcontainer.json`)? Only one level deep is searched per the spec. Deeper nesting is not discovered.
- What happens with symlinked subdirectories under `.devcontainer/`? They are followed if the OS resolves them (standard filesystem behavior).

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: The config search MUST check three locations in this priority order: (1) `.devcontainer/devcontainer.json`, (2) `.devcontainer.json`, (3) `.devcontainer/<subfolder>/devcontainer.json` for each direct subdirectory of `.devcontainer/`.
- **FR-002**: When exactly one config file is found across all three search locations, the tool MUST use it automatically without requiring `--config`.
- **FR-003**: When multiple named config files are found at priority level 3 (and no higher-priority config exists), the tool MUST return an error listing all discovered configs and instructing the user to specify `--config`.
- **FR-004**: When `--config` is specified, the tool MUST use that exact path, bypassing all automatic discovery logic.
- **FR-005**: Subdirectory enumeration MUST be limited to one level deep under `.devcontainer/` — only direct child directories are checked. Results MUST be sorted alphabetically by directory name for deterministic behavior across platforms.
- **FR-006**: Subdirectories that do not contain a `devcontainer.json` or `devcontainer.jsonc` file MUST be silently skipped during enumeration. Both filename variants are valid config files, consistent with the existing search locations.
- **FR-007**: The expanded search MUST apply to all commands that consume config: `up`, `down`, `exec`, `build`, `read-configuration`, and `run-user-commands`.
- **FR-008**: The error message for multiple named configs MUST list all discovered config file paths so the user can choose.
- **FR-009**: Priority levels 1 and 2 (`.devcontainer/devcontainer.json` and `.devcontainer.json`) MUST short-circuit the search — if found, no subdirectory enumeration occurs.

## Clarifications

### Session 2026-02-22

- Q: Should `down` be included in FR-007 scope alongside the other config-consuming commands? → A: Yes, add `down` to FR-007 (6 commands total)
- Q: Should named config subfolder enumeration be sorted alphabetically for determinism? → A: Yes, sort alphabetically
- Q: Should named subfolder search check for `devcontainer.jsonc` in addition to `devcontainer.json`? → A: Yes, check both for consistency with existing discovery

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: Developers with a single named config folder can run all supported commands without specifying `--config`, with zero additional steps compared to the standard `.devcontainer/devcontainer.json` layout.
- **SC-002**: Existing projects using `.devcontainer/devcontainer.json` or `.devcontainer.json` experience no change in behavior (full backward compatibility).
- **SC-003**: Developers with multiple named configs receive a clear, actionable error message that lists all available configs within one command invocation.
- **SC-004**: All six config-consuming commands (`up`, `down`, `exec`, `build`, `read-configuration`, `run-user-commands`) correctly resolve named configs through the shared discovery logic.
- **SC-005**: The tool passes all existing tests with no regressions and includes new tests covering named config discovery, multi-config error handling, and `--config` override scenarios.
