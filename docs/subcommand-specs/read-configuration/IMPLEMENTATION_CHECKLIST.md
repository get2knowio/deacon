# Read-Configuration Implementation Checklist

Use this checklist to track implementation progress against the specification.

**Legend:**
- ✅ Implemented and tested
- 🟡 Partially implemented
- ❌ Not implemented
- ⏭️ Skipped (intentional)

---

## CLI Flags

### Path and Discovery
- [x] `--workspace-folder <PATH>` (via global flag)
- [x] `--config <PATH>` (via global flag)
- [x] `--override-config <PATH>` (via global flag)
- [x] `--mount-workspace-git-root` (optional boolean, default true)

### Container Selection
- [x] `--container-id <ID>` (optional)
- [x] `--id-label <name=value>` (optional, repeatable)

### Docker Tooling
- [x] `--docker-path <PATH>` (optional, default `docker`) - via global flag
- [x] `--docker-compose-path <PATH>` (optional, default `docker-compose`) - via global flag

### Logging/Terminal
- [x] `--log-level {info|debug|trace}` (via global flag)
- [x] `--log-format {text|json}` (via global flag)
- [ ] `--terminal-columns <N>` (optional, requires `--terminal-rows`)
- [ ] `--terminal-rows <N>` (optional, requires `--terminal-columns`)

### Features and Output Shaping
- [x] `--include-merged-configuration` (optional boolean)
- [ ] `--include-features-configuration` (optional boolean)
- [ ] `--additional-features <JSON>` (optional)
- [ ] `--skip-feature-auto-mapping` (optional hidden boolean)

### Other
- [ ] `--user-data-folder <PATH>` (accepted but unused)

---

## Argument Validation

- [x] Validate `--id-label` format: `/.+=.+/` (non-empty key and value)
- [ ] Require at least one of: `--container-id`, `--id-label`, or `--workspace-folder` (Note: workspace-folder defaults to current directory, implicitly satisfying this requirement)
- [ ] Validate terminal dimensions are paired (both or neither)
- [ ] Validate `--additional-features` parses as JSON object
- [x] Handle invalid/missing config paths with clear errors

---

## Configuration Resolution

### Discovery and Reading
- [x] Build CLI host with platform/env
- [x] Compute workspace from `--workspace-folder`
- [x] Determine config path (explicit or discovered)
- [x] Read config via JSONC parser with comment support
- [x] Normalize old properties (e.g., `containerEnv` → `remoteEnv`)
- [x] Handle missing config error with path in message
- [ ] Support empty base config when only container flags provided

### Substitution Rules
- [x] Pre-container substitution: `${env:VAR}`, `${localEnv:VAR}`, `${localWorkspaceFolder}`, etc.
- [ ] Before-container substitution: `${devcontainerId}` using id-labels
- [ ] Container substitution: `${containerEnv:VAR}`, `${containerWorkspaceFolder}`
- [ ] Default values: `${localEnv:NAME:default}`, `${containerEnv:NAME:default}`

### Feature Resolution
- [ ] Compute `featuresConfiguration` when `--include-features-configuration` is set
- [ ] Compute `featuresConfiguration` when `--include-merged-configuration` without container
- [ ] Merge `--additional-features` into feature plan
- [ ] Support `--skip-feature-auto-mapping` toggle
- [ ] Output `featureSets` array with source information

### Merge Algorithm
- [ ] When container found: read metadata from container (`getImageMetadataFromContainer`)
- [ ] When container found: apply `containerSubstitute` to metadata
- [ ] When no container: compute `imageBuildInfo` from config + features
- [ ] When no container: derive metadata via `getDevcontainerMetadata`
- [ ] Merge base config + image metadata using `mergeConfiguration`
- [ ] Handle remoteEnv merging (last-wins per key)
- [ ] Handle mounts deduplication (by target)
- [ ] Handle lifecycle hooks merging
- [ ] Handle host requirements merging

---

## External System Interactions

### Docker/Container Runtime
- [ ] Find container by `--container-id`
- [ ] Find container by `--id-label` (with label matching)
- [ ] Infer container from `--workspace-folder` (id-label generation)
- [ ] Execute `docker inspect <container>` to read metadata
- [ ] Extract container environment variables
- [ ] Extract container labels
- [ ] Handle Docker unavailable error gracefully

### File System
- [x] Read `devcontainer.json` or `.devcontainer/devcontainer.json`
- [x] Support JSON with comments (JSONC)
- [x] Support override config file
- [x] Handle cross-platform paths (POSIX, Win32, WSL2)
- [x] Resolve symlinks where applicable
- [x] Handle read permission errors

---

## Output Structure

### Stdout JSON Payload
- [ ] Always include `configuration` field (substituted DevContainerConfig)
- [ ] Include `workspace` field (WorkspaceConfig with workspaceFolder, workspaceMount, etc.)
- [ ] Include `featuresConfiguration` field when requested or needed
- [ ] Include `mergedConfiguration` field when `--include-merged-configuration` is set
- [ ] Omit optional fields when not requested
- [ ] Single-line JSON output

### Field Structure Compliance
- [ ] `configuration`: matches `DevContainerConfig` spec structure
- [ ] `workspace`: matches `WorkspaceConfig` spec structure  
- [ ] `featuresConfiguration`: matches `FeaturesConfig` spec structure (with `featureSets` array)
- [ ] `mergedConfiguration`: matches `MergedDevContainerConfig` spec structure

### Stderr Logging
- [x] Format logs per `--log-format` (text or JSON)
- [x] Filter logs per `--log-level` (info, debug, trace)
- [x] Include progress and diagnostic details
- [x] No JSON payload on stderr in error case

---

## Error Handling

### User Errors
- [ ] Missing selector: "Missing required argument: One of --container-id, --id-label or --workspace-folder is required." (Note: workspace-folder defaults to current directory, so this error is not typically encountered)
- [x] Invalid `--id-label` format: "Unmatched argument format: id-label must match <name>=<value>."
- [x] Config not found: Message includes resolved path
- [x] Malformed JSON in config: Parse/validation failure details
- [ ] Malformed JSON in `--additional-features`: Parse error

### System Errors
- [ ] Docker unavailable: Exit 1 with clear message
- [ ] Docker inspect failure: Exit 1 with diagnostic info
- [x] Filesystem read errors: Exit 1 with error details

### Configuration Errors
- [x] Non-object config root: Exit 1 with validation message
- [ ] Compose config without workspace: Error from underlying helpers

### Exit Codes
- [x] Exit 0 on success with JSON to stdout
- [x] Exit 1 on error with message to stderr, no stdout

---

## Edge Cases

- [ ] Only container flags provided (no config/workspace): return `{ configuration: {}, ... }`
- [ ] `--id-label` order differences: do not affect `${devcontainerId}` value
- [ ] Adding/removing labels: changes `${devcontainerId}` value
- [x] `--override-config` without workspace: allowed and works
- [x] Invalid/missing `devcontainer.json` when `--config` given: error
- [x] Read-only filesystems: command works (no writes)
- [x] Permission denied reading config: error with details

---

## Testing

### Unit Tests
- [x] Basic config reading
- [x] Config with variable substitution
- [x] Override config merging
- [x] Secrets integration
- [x] Config not found error
- [ ] Container selection (by ID)
- [ ] Container selection (by labels)
- [ ] Feature resolution
- [ ] Merged config (container metadata)
- [ ] Merged config (feature metadata)
- [ ] Additional features merging
- [ ] ID label validation
- [ ] Selector requirement validation

### Integration Tests
- [ ] End-to-end with running container
- [ ] End-to-end with features
- [ ] End-to-end with Docker Compose
- [ ] Cross-platform path handling (Linux, macOS, Windows, WSL2)
- [ ] Container environment substitution
- [ ] Feature-derived metadata merging

---

## Performance & Security

### Performance
- [x] Minimal memory footprint
- [x] No unnecessary network I/O
- [x] Fast config parsing (JSONC)
- [ ] No redundant Docker inspect calls

### Security
- [x] Redaction of secrets in logs
- [x] No execution of untrusted code
- [ ] Input sanitization for `--id-label`
- [ ] Input sanitization for `--additional-features` JSON
- [ ] No command injection surfaces
- [x] No container execution (inspect only)

---

## Documentation

- [x] Specification documents (SPEC.md, DATA-STRUCTURES.md, DIAGRAMS.md)
- [ ] Implementation gap analysis (this document)
- [ ] Command-line help text (`--help`)
- [ ] Examples in repository
- [ ] Migration guide for breaking changes

---

## Progress Summary

**Total Items:** ~80  
**Completed:** ~20 (25%)  
**In Progress:** 0 (0%)  
**Not Started:** ~60 (75%)

---

## Next Steps

1. [ ] Implement container selection flags and logic
2. [ ] Add Docker integration for metadata reading
3. [ ] Implement feature resolution
4. [ ] Fix output structure to match spec
5. [ ] Implement proper merge algorithm
6. [ ] Add input validation
7. [ ] Expand test coverage
8. [ ] Update documentation and examples

---

**Last Updated:** October 13, 2025  
**Tracking Version:** 1.0
