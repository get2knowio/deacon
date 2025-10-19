---
subcommand: upgrade
type: enhancement
priority: medium
scope: medium
---

# [upgrade] OCI and Feature Resolution Integration for Lockfile

## Issue Type
- [ ] Missing CLI Flags
- [x] Core Logic Implementation
- [ ] Infrastructure/Cross-Cutting Concern
- [ ] Error Handling
- [ ] Testing & Validation
- [ ] Other: ___________

## Description
Ensure the feature resolution used by `upgrade` fetches manifests/digests via the existing OCI client and produces the integrity fields required for lockfile entries. This task focuses on stitching together the resolver and OCI layers; not re-implementing the client.

## Specification Reference

**From SPEC.md Section:** §7 External System Interactions — OCI Registries

**From GAP.md Section:** 4.1 OCI Registry Integration, 2.4 Feature Resolution

### Expected Behavior
- For OCI-backed features: obtain digest and resolved reference (`path@sha256:...`).
- For tarball/direct sources: set `resolved` to tarball URI and compute integrity digest if applicable.
- Fail fast with clear error when network/auth issues occur.

### Current Behavior
- Core has OCI and features modules, but `upgrade` path not wired.

## Implementation Requirements

### 1. Code Changes Required

#### Files to Modify
- `crates/deacon/src/commands/upgrade.rs`
  - Call feature resolver methods that traverse OCI as needed
  - Map resolver response into lockfile entries
  - Propagate errors with context

#### Specific Tasks
- [ ] Use HEAD for blob existence checks per repo guidelines (core may already do this)
- [ ] Ensure Location header handling (applies if uploads occur — may not in upgrade)
- [ ] Distinguish 404, 401/403, and 5xx into actionable error chains

### 2. Data Structures
- Reuse `FeaturesConfig` structures from DATA-STRUCTURES.md

### 3. Validation Rules
- [ ] None beyond pass/fail

### 4. Cross-Cutting Concerns
- [ ] Theme 4 - Semantic Versioning Operations
- [ ] Theme 6 - Error Messages

## Testing Requirements

### Unit Tests
- [ ] Map resolver outputs to lockfile entries (mock)

### Integration Tests
- [ ] Use fake/fixture registry or mock client where available to assert digest handling

### Smoke Tests
- [ ] None

### Examples
- [ ] None

## Acceptance Criteria
- [ ] Lockfile entries include correct `resolved` and `integrity`
- [ ] Errors from OCI surface clearly; exit code 1 on failure
- [ ] CI passes

## References
- SPEC: `docs/subcommand-specs/upgrade/SPEC.md` (§7)
- GAP: `docs/subcommand-specs/upgrade/GAP.md` (§4.1)
