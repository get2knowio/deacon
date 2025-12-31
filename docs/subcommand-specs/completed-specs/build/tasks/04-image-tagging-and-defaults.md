---
subcommand: build
type: enhancement
priority: high
scope: medium
---

# [build] Implement image tagging via --image-name and deterministic defaults

## Issue Type
- [ ] Missing CLI Flags
- [x] Core Logic Implementation
- [ ] Infrastructure/Cross-Cutting Concern
- [ ] Error Handling
- [x] Testing & Validation

## Parent Issue
Tracks: #0 (tracking issue)

## Description
Tag the built image with all user-provided `--image-name` values and preserve a deterministic default tag when no names are provided. Return the final tag(s) in the output per the spec.

## Specification Reference

**From SPEC.md Section:** §5 Core Execution Logic; §7 External System Interactions

**From GAP.md Section:** 3.1 Image Tagging

### Expected Behavior
- If `--image-name` values supplied, apply a `-t` for each to the docker build call or tag post-build (depending on BuildKit usage and flow).
- If no `--image-name`, retain the deterministic `deacon-build:<config-hash-12>` tag.
- Output uses these final names in `imageName` field (string or array).

### Current Behavior
- Only a single deterministic tag is applied; no support for `--image-name`.

## Implementation Requirements

### 1. Code Changes Required

#### Files to Modify
- `crates/deacon/src/commands/build.rs` – update `execute_docker_build` to add multiple `-t` flags for each `image_names` entry; continue to add deterministic tag when none provided.
- `crates/deacon/src/commands/build.rs` – store and return all applied tags for the output task to use.

#### Specific Tasks
- [ ] Append multiple `-t` args for each `image_names` value.
- [ ] Keep deterministic tag if `image_names` is empty.
- [ ] Ensure `BuildResult.tags` reflects all tags in order provided by the user (deterministic tag can be last or first; document choice and keep consistent).

### 2. Data Structures
N/A beyond existing `BuildArgs.image_names: Vec<String>` (from issue 01).

### 3. Validation Rules
- N/A; handled by separate validation issue.

### 4. Cross-Cutting Concerns

**Applies from PARITY_APPROACH.md:**
- [x] Theme 1 - JSON Output Contract: ensure `imageName` reflects the final tagging.

## Testing Requirements

### Unit Tests
- [ ] Verify that multiple `-t` flags are added when `image_names` present.
- [ ] Verify that deterministic tag is used when `image_names` empty.

### Integration Tests
- [ ] End-to-end build that fakes docker call or uses a harmless context to assert assembled args include expected tags.

### Smoke Tests
- [ ] Update smoke as needed to assert presence of provided tag(s) in JSON output.

### Examples
- [ ] Add `examples/build/platform-and-cache/` or existing example README to show `--image-name` usage.

## Acceptance Criteria
- [ ] Built image is tagged with all user-provided names in order.
- [ ] Deterministic tag remains when no names provided.
- [ ] Output includes correct `imageName` value(s).
- [ ] CI checks pass.

## Implementation Notes
- For BuildKit with `--push`/`--output`, tagging behavior may differ; for this issue, focus on normal load scenario and ensure tagging is correct when image is locally available.

## Definition of Done
- [ ] Tagging implemented and tests adjusted.

## References
- Specification: `docs/subcommand-specs/build/SPEC.md` (§5, §7)
- Gap Analysis: `docs/subcommand-specs/build/GAP.md` (§3.1)
