# Feature Specification: Fix Config Merge Rules

**Feature Branch**: `013-fix-merge-rules`
**Created**: 2026-02-21
**Status**: Draft
**Input**: User description: "Fix the property merge rules in ConfigMerger to match the DevContainer specification. Two categories of merge behavior are currently wrong: boolean properties use last-wins instead of OR semantics, and mounts/forwardPorts arrays replace instead of union."

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Feature requiring privileged mode merges correctly (Priority: P1)

A DevContainer Feature (e.g., Docker-in-Docker) declares `privileged: true` in its metadata. The user's `devcontainer.json` either omits `privileged` or explicitly sets it to `false`. After configuration merging, the final merged config must have `privileged: true` because the Feature requires elevated permissions to function.

**Why this priority**: This is a correctness bug. A Feature that needs privileged access silently loses that requirement during merge, causing container startup failures or broken functionality with no clear error message.

**Independent Test**: Can be fully tested by merging two configs where the base has `privileged: true` and the overlay has `privileged: false`, then verifying the merged result is `true`.

**Acceptance Scenarios**:

1. **Given** a base config with `privileged: true` and an overlay with `privileged: false`, **When** the configs are merged, **Then** the merged result has `privileged: true`
2. **Given** a base config with `privileged: true` and an overlay that omits `privileged`, **When** the configs are merged, **Then** the merged result has `privileged: true`
3. **Given** a base config that omits `privileged` and an overlay with `privileged: true`, **When** the configs are merged, **Then** the merged result has `privileged: true`
4. **Given** a base config with `privileged: false` and an overlay with `privileged: false`, **When** the configs are merged, **Then** the merged result has `privileged: false`
5. **Given** both configs omit `privileged`, **When** the configs are merged, **Then** the merged result has no `privileged` value (uses spec default)

---

### User Story 2 - Feature adding mounts preserves existing mounts (Priority: P1)

A DevContainer Feature declares additional mounts (e.g., a volume for caching). The user's `devcontainer.json` also declares mounts (e.g., a bind mount for SSH keys). After merging, both sets of mounts must be present in the final config. Duplicate mount entries must be removed.

**Why this priority**: This is a correctness bug of equal severity to the boolean case. Features that add mounts (e.g., for persistent caches) silently destroy the user's own mounts, causing data loss or broken workflows.

**Independent Test**: Can be fully tested by merging two configs each with distinct mounts, then verifying the merged result contains all unique mounts from both.

**Acceptance Scenarios**:

1. **Given** a base config with mounts `[A, B]` and an overlay with mounts `[C, D]`, **When** merged, **Then** the result contains `[A, B, C, D]`
2. **Given** a base config with mounts `[A, B]` and an overlay with mounts `[B, C]`, **When** merged, **Then** the result contains `[A, B, C]` (B is not duplicated)
3. **Given** a base config with mounts `[A]` and an overlay with no mounts, **When** merged, **Then** the result contains `[A]`
4. **Given** a base config with no mounts and an overlay with mounts `[A]`, **When** merged, **Then** the result contains `[A]`

---

### User Story 3 - Feature adding forwarded ports preserves existing ports (Priority: P1)

A DevContainer Feature declares forwarded ports (e.g., port 5432 for a database). The user's `devcontainer.json` also declares forwarded ports (e.g., port 3000 for their application). After merging, all unique ports from both configs must be present.

**Why this priority**: Same class of bug as mounts — a Feature that forwards ports silently removes the user's port forwards, breaking connectivity.

**Independent Test**: Can be fully tested by merging two configs each with distinct forwardPorts, then verifying the merged result contains all unique ports from both.

**Acceptance Scenarios**:

1. **Given** a base config with `forwardPorts: [3000, 8080]` and an overlay with `forwardPorts: [5432, 6379]`, **When** merged, **Then** the result contains `[3000, 8080, 5432, 6379]`
2. **Given** a base config with `forwardPorts: [3000, 8080]` and an overlay with `forwardPorts: [8080, 5432]`, **When** merged, **Then** the result contains `[3000, 8080, 5432]` (8080 is not duplicated)
3. **Given** a base config with `forwardPorts: [3000]` and an overlay with no forwardPorts, **When** merged, **Then** the result contains `[3000]`

---

### User Story 4 - Init boolean merges with OR semantics (Priority: P2)

The `init` property follows the same boolean OR merge rules as `privileged`. If any source sets `init: true`, the merged result must be `true`.

**Why this priority**: Same bug as privileged, lower priority because init failures are less severe (no security implications, just PID 1 behavior).

**Independent Test**: Can be fully tested by merging two configs where one has `init: true` and the other has `init: false`, then verifying the merged result is `true`.

**Acceptance Scenarios**:

1. **Given** a base config with `init: true` and an overlay with `init: false`, **When** merged, **Then** the result has `init: true`
2. **Given** both configs omit `init`, **When** merged, **Then** the result has no `init` value

---

### Edge Cases

- What happens when mounts contain both string-form (`"source=/src,target=/dst,type=bind"`) and object-form (`{"source": "/src", "target": "/dst", "type": "bind"}`) entries? Deduplication compares the serialized JSON representation — string and object forms of the same mount are treated as distinct entries (consistent with how they serialize differently).
- What happens when forwardPorts contain both numeric (`3000`) and string (`"3000:3000"`) representations of the same port? These are different PortSpec variants and are treated as distinct entries (consistent with the upstream spec's treatment of port numbers vs port mappings).
- What happens when three or more configs are merged in a chain (e.g., image metadata + Feature + user config)? The boolean OR and array union rules are applied at each pairwise merge step, producing the correct cumulative result.
- What happens when `privileged` and `init` are both `None` in all sources? The merged result is `None`, not `Some(false)` — the spec default applies when no source sets the value.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: Boolean properties (`privileged`, `init`) MUST merge using OR semantics: the merged value is `true` if either the base or overlay value is `true`, regardless of which source set it
- **FR-002**: The `mounts` array MUST be merged by union (concatenation with deduplication), preserving base entries first followed by new overlay entries
- **FR-003**: The `forwardPorts` array MUST be merged by union (concatenation with deduplication), preserving base entries first followed by new overlay entries
- **FR-004**: Deduplication of `mounts` entries MUST compare the full serialized JSON representation of each entry (string or object)
- **FR-005**: Deduplication of `forwardPorts` entries MUST compare the PortSpec values for equality
- **FR-006**: When both base and overlay omit a boolean property, the merged result MUST be `None` (not `Some(false)`)
- **FR-007**: Existing merge behavior for all other property categories MUST remain unchanged: scalars (last-wins), maps (key merge), objects (deep merge), and concatenated arrays (`runArgs`, `capAdd`, `securityOpt`)
- **FR-008**: The boolean OR merge MUST work correctly across multi-step merge chains (e.g., image metadata merged with Feature merged with user config)

### Key Entities

- **DevContainerConfig**: The configuration structure being merged, containing boolean, array, map, scalar, and object properties with different merge semantics per the DevContainer specification
- **ConfigMerger**: The component responsible for applying correct merge rules when combining two configs
- **PortSpec**: An enum representing a forwarded port as either a number or a string mapping, used for forwardPorts deduplication

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: All boolean merge test cases pass, including the critical `base=true, overlay=false` case that currently fails
- **SC-002**: Mount arrays from multiple sources are fully preserved after merge — no entries silently dropped
- **SC-003**: ForwardPorts arrays from multiple sources are fully preserved after merge — no entries silently dropped
- **SC-004**: All existing merge tests continue to pass without modification (no regressions)
- **SC-005**: Zero clippy warnings and all formatting checks pass
- **SC-006**: Multi-step merge chains produce correct cumulative results for boolean OR and array union properties
