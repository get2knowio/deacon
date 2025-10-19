---
subcommand: features-package
type: enhancement
priority: medium
scope: small
labels: ["subcommand: features-package", "type: enhancement", "priority: medium", "scope: small"]
---

# [features-package] Use .tgz Artifact Format and Improve Logging

## Issue Type
- [ ] Missing CLI Flags
- [x] Core Logic Implementation
- [ ] Infrastructure/Cross-Cutting Concern
- [ ] Error Handling
- [x] Testing & Validation
- [ ] Other: 

## Description
Update packaging to produce `.tgz` files (gzip-compressed tar) to align with common distribution practice and spec examples. Enhance logs to indicate mode and per-feature results for better UX and testability.

## Specification Reference

**From SPEC.md Section:** “§7. External System Interactions” and “§10. Output Specifications”

**From GAP.md Section:** “6. OUTPUT ISSUES: Missing Collection Artifacts”

### Expected Behavior
- Artifact filenames are `<feature-id>.tgz`.
- Logs include:
  - "Packaging single feature..." or "Packaging feature collection..."
  - "Created package: <id>.tgz (digest: <sha256:...>, size: <bytes>)"

### Current Behavior
- Produces `<feature-id>.tar` and manifest; logs limited to single feature.

## Implementation Requirements

### 1. Code Changes Required

#### Files to Modify
- `crates/deacon/src/commands/features.rs`
  - Update `create_feature_package` to support gzip compression and `.tgz` extension.
  - Ensure digest computed on the compressed file bytes.
  - Update info! messages to the required log format.

#### Specific Tasks
- [ ] Replace `tar::Builder` sink with `flate2::write::GzEncoder<File>` for gzip output and name files `<id>.tgz`.
- [ ] Compute sha256 over compressed output file; keep manifest writing if still needed by other flows.
- [ ] Adjust tests expecting `.tar` to `.tgz` and log lines accordingly.

### 2. Data Structures
N/A.

### 3. Validation Rules
N/A.

### 4. Cross-Cutting Concerns

**Applies from PARITY_APPROACH.md:**
- [ ] Theme 6 - Error Messages: maintain clarity when I/O fails.

## Testing Requirements

### Unit Tests
- [ ] `test_create_feature_package` updated to assert `.tgz` exists and digest format.

### Integration Tests
- [ ] End-to-end package run verifies `.tgz` filenames and logs.

### Smoke Tests
- [ ] Ensure smoke accepts `.tgz` artifacts.

## Acceptance Criteria
- [ ] `.tgz` artifacts produced with correct digests and sizes.
- [ ] Logs reflect mode and per-feature messages.
- [ ] CI green.

## Implementation Notes
- Keep `.tar` support only if other subcommands depend on it; otherwise migrate tests fully.

## References
- `docs/subcommand-specs/features-package/SPEC.md` (§10)
- `docs/subcommand-specs/features-package/GAP.md` (Section 6)
- `docs/PARITY_APPROACH.md` (Theme 6)
