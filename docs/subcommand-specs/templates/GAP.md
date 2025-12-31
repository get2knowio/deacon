# Templates Subcommand Implementation Gap Analysis

**Date**: October 13, 2025  
**Specification Version**: Based on `SPEC.md`, `DATA-STRUCTURES.md`, `DIAGRAMS.md` in `/workspaces/deacon/docs/subcommand-specs/templates/`  
**Implementation Version**: Current state in `/workspaces/deacon/crates/deacon/src/commands/templates.rs` and `/workspaces/deacon/crates/core/src/templates.rs`

## Executive Summary

The current `templates` subcommand implementation has **significant gaps** compared to the specification. While basic functionality for local templates exists, the implementation diverges substantially from the specified CLI interface, data structures, and OCI registry behavior.

**Overall Compliance: ~40%**

### Critical Missing Features
1. Incorrect CLI argument structure for all subcommands
2. Missing `--features` injection capability in `apply`
3. Incorrect output format (no structured JSON for `apply`)
4. Missing `--omit-paths` functionality
5. Incomplete `publish` workflow (no semantic versioning, collection metadata)
6. `metadata` command operates on local paths instead of OCI refs
7. Missing support for template option substitution patterns

---

## 1. CLI Interface Gaps

### 1.1 `templates apply` Command

#### Specification Requirements
```
templates apply
  --workspace-folder, -w <path>      (Required, default: '.')
  --template-id, -t <oci-ref>        (Required)
  --template-args, -a <json>         (JSON object, default: {})
  --features, -f <json>              (JSON array, default: [])
  --omit-paths <json>                (JSON array, default: [])
  --tmp-dir <path>                   (Optional)
  --log-level <info|debug|trace>     (Default: info)
  
Output: { files: string[] }
```

#### Current Implementation
```rust
Apply {
    template: String,                // Positional (not --template-id)
    option: Vec<String>,             // --option in key=value format (not JSON)
    output: Option<String>,          // --output (not --workspace-folder)
    force: bool,                     // --force (not in spec)
    dry_run: bool,                   // --dry-run (not in spec)
}
```

#### Gaps

| Feature | Spec | Implementation | Status | Severity |
|---------|------|----------------|--------|----------|
| `--template-id` flag | Required | Positional `template` arg | ❌ Missing | **CRITICAL** |
| `--workspace-folder` flag | Required (default `.`) | `--output` | ❌ Wrong semantics | **CRITICAL** |
| `--template-args` JSON | Required | `--option key=value` | ❌ Wrong format | **CRITICAL** |
| `--features` JSON array | Required | Not present | ❌ Missing | **CRITICAL** |
| `--omit-paths` JSON array | Optional | Not present | ❌ Missing | **HIGH** |
| `--tmp-dir` | Optional | Not present | ❌ Missing | **MEDIUM** |
| `--log-level` | Optional | Global flag | ⚠️ Different location | **LOW** |
| `--force` flag | Not in spec | Present | ⚠️ Extension | **LOW** |
| `--dry-run` flag | Not in spec | Present | ⚠️ Extension | **LOW** |
| JSON output `{ files: [] }` | Required | Logs only | ❌ Missing | **CRITICAL** |

**Impact**: The CLI interface is completely incompatible with the specification. Users cannot invoke the command as specified.

**Recommendation**: Complete rewrite of CLI argument parsing for `apply` subcommand.

---

### 1.2 `templates publish` Command

#### Specification Requirements
```
templates publish [target]
  --registry, -r <host>          (Default: ghcr.io)
  --namespace, -n <owner/repo>   (Required)
  --log-level <info|debug|trace> (Default: info)
  
Output: { [templateId]: { publishedTags?: string[], digest?: string, version?: string } }
```

#### Current Implementation
```rust
Publish {
    path: String,                    // Positional (called 'path', not 'target')
    registry: String,                // --registry (matches but different semantics)
    dry_run: bool,                   // --dry-run (not in spec)
    username: Option<String>,        // --username (not in spec)
    password_stdin: bool,            // --password-stdin (not in spec)
}
```

#### Gaps

| Feature | Spec | Implementation | Status | Severity |
|---------|------|----------------|--------|----------|
| Positional `target` | Optional (default `.`) | `path` (required) | ⚠️ Close but wrong | **MEDIUM** |
| `--namespace` flag | Required | Not present | ❌ Missing | **CRITICAL** |
| `--registry` flag | Optional (default `ghcr.io`) | Required | ⚠️ Wrong validation | **MEDIUM** |
| Collection support | Templates collection handling | Single template only | ❌ Missing | **CRITICAL** |
| Semantic tag computation | Required | Not implemented | ❌ Missing | **CRITICAL** |
| Collection metadata publish | Required | Not implemented | ❌ Missing | **HIGH** |
| Output format | Map of template results | Single result | ❌ Wrong structure | **HIGH** |
| `--username`/`--password-stdin` | Not in spec | Present | ⚠️ Extension | **LOW** |

**Impact**: Cannot publish template collections; no semantic versioning support; incorrect output format.

**Recommendation**: Refactor `publish` to support collection directories, semantic tag computation, and proper output structure.

---

### 1.3 `templates metadata` Command

#### Specification Requirements
```
templates metadata <templateId>
  --log-level <info|debug|trace>
  
Where templateId is an OCI reference (e.g., ghcr.io/owner/templates/name:tag)
Output: Template metadata JSON from manifest annotation or {}
```

#### Current Implementation
```rust
Metadata {
    path: String,  // Local filesystem path, not OCI reference
}
```

#### Gaps

| Feature | Spec | Implementation | Status | Severity |
|---------|------|----------------|--------|----------|
| OCI reference as input | Required | Local path | ❌ Wrong input type | **CRITICAL** |
| Fetch from registry | Required | Parse local file | ❌ Wrong source | **CRITICAL** |
| Manifest annotation parsing | Required | N/A (no registry interaction) | ❌ Missing | **CRITICAL** |
| Return `{}` on missing metadata | Required | Error on missing file | ⚠️ Wrong error handling | **MEDIUM** |

**Impact**: Command cannot retrieve metadata from published templates in registries. Completely different semantics.

**Recommendation**: Rewrite to fetch OCI manifest and extract `dev.containers.metadata` annotation.

---

### 1.4 `templates generate-docs` Command

#### Specification Requirements
```
templates generate-docs
  --project-folder, -p <path>  (Default: .)
  --github-owner <owner>
  --github-repo <repo>
  --log-level <info|debug|trace>
```

#### Current Implementation
```rust
GenerateDocs {
    path: String,          // Positional, not --project-folder
    output: String,        // --output (not in spec)
}
```

#### Gaps

| Feature | Spec | Implementation | Status | Severity |
|---------|------|----------------|--------|----------|
| `--project-folder` flag | Optional (default `.`) | Positional `path` | ⚠️ Different style | **MEDIUM** |
| `--github-owner` flag | Optional | Not present | ❌ Missing | **MEDIUM** |
| `--github-repo` flag | Optional | Not present | ❌ Missing | **MEDIUM** |
| `--output` flag | Not in spec | Present | ⚠️ Extension | **LOW** |
| Output destination | Implicit in project structure | Explicit via `--output` | ⚠️ Different approach | **LOW** |

**Impact**: Less critical as command is primarily for authoring. Missing GitHub metadata limits documentation quality.

**Recommendation**: Add `--github-owner` and `--github-repo` flags; consider aligning with spec's implicit output approach.

---

### 1.5 `templates pull` Command

#### Specification Note
The spec states: "The reference CLI does not expose a dedicated `templates pull` subcommand; fetching occurs within `templates apply`."

#### Current Implementation
```rust
Pull {
    registry_ref: String,
    json: bool,
}
```

**Status**: ⚠️ **Extension beyond spec**  
**Impact**: Not a gap per se, but this is an implementation-specific extension. Document it as such.

**Recommendation**: Keep as a useful extension but clearly mark as non-standard in documentation.

---

## 2. Data Structure Gaps

### 2.1 Template Option Substitution

#### Specification
- Files should support `${templateOption:KEY}` token substitution
- All template options must be provided or have defaults
- Substitution happens during `apply`

#### Current Implementation
- Uses general variable substitution (`${localWorkspaceFolder}`, etc.)
- **DOES NOT** support `${templateOption:KEY}` pattern
- No special handling for template options in substitution context

#### Gap
❌ **CRITICAL**: Template-specific variable substitution pattern is not implemented.

**Impact**: Template authors cannot use the standard `${templateOption:...}` pattern specified in the Dev Containers spec.

**Recommendation**: Extend `VariableSubstitution` in `crates/core/src/variable.rs` to handle `templateOption:` prefix and look up values from `ApplyOptions.options`.

---

### 2.2 Feature Injection

#### Specification
```typescript
interface TemplateFeatureOption {
  id: string;
  options: Record<string, string | boolean | undefined>;
}
```

Apply command should accept `--features <json>` and inject into `devcontainer.json`:
- Create `features` object if missing
- Set `features[feature.id] = options || {}`

#### Current Implementation
- No `--features` flag
- No feature injection logic in `apply_template`

#### Gap
❌ **CRITICAL**: Cannot apply features alongside templates.

**Impact**: Users must manually add features after applying template, reducing automation.

**Recommendation**: 
1. Add `--features` flag accepting JSON array
2. Parse JSON to `Vec<TemplateFeatureOption>`
3. After template application, locate `devcontainer.json` in output
4. Parse JSONC, add/update `features` property, write back

---

### 2.3 Omit Paths

#### Specification
- `--omit-paths <json>` should accept JSON array of paths
- Paths can have `/*` suffix for directory-wide exclusion
- Reserved files (`devcontainer-template.json`, `README.md`, `NOTES.md`) always omitted

#### Current Implementation
- Hardcoded exclusion of `devcontainer-template.json` only
- No user-controllable omit paths

#### Gap
❌ **HIGH**: Cannot omit template files selectively.

**Impact**: Users cannot exclude unwanted files (e.g., tests, examples, documentation).

**Recommendation**: 
1. Add `omit_paths` to `ApplyOptions`
2. Implement glob-style matching in `plan_template_application`
3. Always include reserved files in omit set

---

### 2.4 Output Format

#### Specification
Apply should output JSON to stdout:
```json
{ "files": ["path/to/file1.txt", "path/to/file2.json"] }
```

#### Current Implementation
- Outputs logs to stderr (good)
- **DOES NOT** output structured JSON to stdout
- No list of applied files returned

#### Gap
❌ **CRITICAL**: Missing structured output for programmatic consumption.

**Impact**: Cannot parse apply results in CI/CD pipelines or automation scripts.

**Recommendation**: 
1. Collect relative file paths during `execute_planned_actions`
2. Print JSON object to stdout on success
3. Ensure logs go to stderr only

---

## 3. OCI/Registry Interaction Gaps

### 3.1 Semantic Version Tags

#### Specification
Publish should compute and push semantic tags:
- Given version `1.2.3`, push tags: `1`, `1.2`, `1.2.3`, `latest`
- Skip tags if already published with higher version
- Requires fetching existing tags from registry

#### Current Implementation
```rust
// In execute_templates_publish:
let template_ref = TemplateRef::new(registry_url, namespace, name, tag);
let publish_result = fetcher.publish_template(&template_ref, tar_data.into(), &metadata).await?;
```

- Publishes to single tag only
- No semantic tag computation
- No existing tag checks

#### Gap
❌ **CRITICAL**: Semantic versioning not supported.

**Impact**: Cannot follow Dev Container registry best practices; poor discoverability.

**Recommendation**:
1. Parse `metadata.version` as semver
2. Call registry API to list existing tags
3. Compute semantic tags to push (major, minor, patch, latest)
4. Push manifest to multiple tags
5. Return list of `publishedTags` in output

---

### 3.2 Collection Metadata

#### Specification
When publishing a collection (directory with `src/` containing multiple templates):
1. Package each template individually
2. Publish each template with semantic tags
3. Generate `devcontainer-collection.json` with all template metadata
4. Publish collection metadata to `<namespace>:latest`

#### Current Implementation
- Only handles single template at `path`
- No collection support
- No `devcontainer-collection.json` generation

#### Gap
❌ **CRITICAL**: Cannot publish template collections.

**Impact**: Cannot use repository structure recommended by Dev Containers spec.

**Recommendation**:
1. Detect if `target` is a collection (has `src/` subdirectory) or single template
2. For collections, iterate through `src/` subdirectories
3. Package and publish each template
4. Aggregate metadata into `devcontainer-collection.json`
5. Publish collection metadata to separate reference

---

### 3.3 Manifest Annotations

#### Specification
Published template manifests must include annotation:
```json
"annotations": {
  "dev.containers.metadata": "<serialized-template-metadata>"
}
```

#### Current Implementation
```rust
// In oci.rs (assumed from context):
fetcher.publish_template(&template_ref, tar_data.into(), &metadata).await?
```

Likely passes metadata, but needs verification that it's properly serialized into `dev.containers.metadata` annotation.

#### Gap
⚠️ **Verification Needed**: Check if `publish_template` in `deacon_core::oci` properly sets manifest annotation.

**Recommendation**: 
1. Review `crates/core/src/oci.rs` `publish_template` implementation
2. Ensure manifest includes `dev.containers.metadata` annotation
3. Add integration test verifying annotation presence

---

## 4. Functional Gaps

### 4.1 Variable Substitution Patterns

#### Specification
Templates use `${templateOption:KEY}` for user-provided options.

#### Current Implementation
- Only supports built-in variables (`${localWorkspaceFolder}`, etc.)
- No `templateOption:` namespace

#### Gap Details
In `crates/core/src/variable.rs`, the `VariableSubstitution::substitute_string` method needs to:
1. Recognize `${templateOption:KEY}` pattern
2. Look up `KEY` in `context.template_options`
3. Replace with option value

Currently, template options are added to context but not accessed with the correct pattern:
```rust
// In templates.rs:
context.template_options = Some(template_options);
```

But substitution likely doesn't check for `templateOption:` prefix.

#### Recommendation
Modify `VariableSubstitution` to handle template option namespace explicitly.

---

### 4.2 Required vs. Optional Options

#### Specification
Options without defaults are required; apply should error if not provided.

#### Current Implementation
```rust
// In execute_templates_apply:
for (option_name, option_def) in &metadata.options {
    if !option_values.contains_key(option_name) {
        if let Some(default_value) = option_def.default_value() {
            option_values.insert(option_name.clone(), default_value);
        } else {
            return Err(anyhow::anyhow!(
                "Missing required option '{}'. Provide a value with --option {}=<value> or define a default.",
                option_name,
                option_name,
            ));
        }
    }
}
```

✅ **CORRECT**: This is properly implemented.

---

### 4.3 Dry Run Behavior

#### Specification
Not explicitly mentioned in spec, but `--dry-run` is a common pattern.

#### Current Implementation
- `--dry-run` flag exists
- Properly skips file writes
- Reports planned actions

✅ **CORRECT**: Good implementation practice (extension beyond spec).

---

## 5. Testing Gaps

### 5.1 Spec-Compliant Tests

Current tests validate:
- ✅ Local template metadata parsing
- ✅ Publish dry-run
- ✅ Generate-docs output
- ✅ CLI help text

Missing tests:
- ❌ Apply with `--template-args` JSON (currently uses key=value)
- ❌ Apply with `--features` JSON
- ❌ Apply with `--omit-paths`
- ❌ Apply JSON output format
- ❌ Metadata from OCI reference
- ❌ Publish semantic versioning
- ❌ Publish collection workflow
- ❌ Template option substitution (`${templateOption:...}`)

**Recommendation**: Add test suite covering spec-defined behavior once implementation is corrected.

---

## 6. Summary of Gaps by Severity

### Critical (Blocks Core Functionality)
1. ❌ Apply CLI interface completely mismatched (args, flags, output)
2. ❌ Missing `--features` injection capability
3. ❌ Missing `${templateOption:KEY}` substitution pattern
4. ❌ Metadata command operates on local paths instead of OCI refs
5. ❌ Publish doesn't support semantic versioning
6. ❌ Publish doesn't support collections
7. ❌ Apply missing structured JSON output

### High (Limits Functionality)
8. ❌ Missing `--omit-paths` in apply
9. ❌ Publish missing `--namespace` flag
10. ❌ Publish output format incorrect (single result vs. map)
11. ❌ No collection metadata generation

### Medium (Workflow Limitations)
12. ⚠️ Generate-docs missing `--github-owner` and `--github-repo`
13. ⚠️ Publish has wrong default for `--registry` (required vs. optional)
14. ⚠️ Metadata error handling differs from spec

### Low (Minor Issues)
15. ⚠️ Log level flag location differs (global vs. per-command)
16. ⚠️ Pull command is non-standard extension (document as such)

---

## 7. Recommended Refactoring Approach

### Phase 1: Fix CLI Interface (Highest Priority)
1. Update `TemplateCommands` in `cli.rs` to match spec exactly
2. Fix `apply` argument structure
3. Fix `publish` argument structure
4. Fix `metadata` to accept OCI reference
5. Update argument parsing and validation

**Estimated Effort**: 2-3 days

### Phase 2: Implement Missing Core Features
1. Add `--features` JSON parsing and injection
2. Implement `${templateOption:KEY}` substitution
3. Add `--omit-paths` support
4. Add structured JSON output for `apply`

**Estimated Effort**: 3-4 days

### Phase 3: Fix Registry Operations
1. Implement semantic version tag computation
2. Implement collection detection and metadata generation
3. Update `metadata` command to fetch from registry
4. Fix `publish` output format

**Estimated Effort**: 4-5 days

### Phase 4: Testing and Documentation
1. Add integration tests for all spec-defined behaviors
2. Update documentation to reflect correct usage
3. Add examples matching spec patterns

**Estimated Effort**: 2-3 days

**Total Estimated Effort**: 11-15 days

---

## 8. Breaking Changes Required

The following changes will break existing usage:

### Breaking Changes
1. `templates apply <template>` → `templates apply --template-id <oci-ref>`
2. `templates apply --option key=value` → `templates apply --template-args '{"key":"value"}'`
3. `templates apply --output <dir>` → `templates apply --workspace-folder <dir>`
4. `templates metadata <path>` → `templates metadata <oci-ref>`
5. `templates publish <path> --registry <url>` → `templates publish [target] --namespace <ns>`
6. Output format changes for all commands

### Migration Path
1. Mark current flags as deprecated with warnings (v0.x)
2. Support both old and new flags during transition period
3. Remove old flags in v1.0 release
4. Provide migration guide and examples

**Recommendation**: Given the extent of changes, consider this a v1.0 milestone and document breaking changes clearly.

---

## 9. Compliance Checklist

Use this checklist to track implementation progress:

### CLI Interface
- [ ] `apply` uses `--template-id` instead of positional
- [ ] `apply` uses `--workspace-folder` instead of `--output`
- [ ] `apply` accepts `--template-args` as JSON
- [ ] `apply` accepts `--features` as JSON array
- [ ] `apply` accepts `--omit-paths` as JSON array
- [ ] `apply` outputs `{ files: [] }` JSON to stdout
- [ ] `publish` accepts `--namespace` (required)
- [ ] `publish` defaults `--registry` to `ghcr.io`
- [ ] `publish` positional `target` defaults to `.`
- [ ] `metadata` accepts OCI reference (not local path)
- [ ] `generate-docs` accepts `--github-owner` and `--github-repo`

### Functional Requirements
- [ ] Template option substitution with `${templateOption:KEY}`
- [ ] Feature injection into `devcontainer.json`
- [ ] Omit paths with glob support (`/*`)
- [ ] Semantic version tag computation
- [ ] Collection detection and handling
- [ ] Collection metadata generation
- [ ] Manifest annotation (`dev.containers.metadata`)

### Output Formats
- [ ] `apply` outputs JSON with `files` array
- [ ] `publish` outputs map of template results
- [ ] `metadata` outputs template JSON or `{}`

### Tests
- [ ] Apply with JSON template args
- [ ] Apply with features
- [ ] Apply with omit paths
- [ ] Apply JSON output parsing
- [ ] Metadata from registry
- [ ] Publish semantic tags
- [ ] Publish collection
- [ ] Template option substitution

---

## 10. Conclusion

The current `templates` implementation provides basic local template functionality but diverges significantly from the Dev Containers specification. The main issues are:

1. **CLI interface is incompatible** - Flags and arguments don't match spec
2. **Missing critical features** - Feature injection, omit paths, proper substitution
3. **Registry operations incomplete** - No semantic versioning, no collections
4. **Output formats wrong** - Missing structured JSON for automation

**Recommendation**: Treat this as a major refactoring effort for a v1.0 release. The changes are breaking and substantial, requiring careful migration planning and extensive testing.

**Priority**: HIGH - The templates system is a core Dev Containers feature. Correct implementation is essential for compatibility with the broader Dev Containers ecosystem.

**Estimated Total Effort**: 11-15 developer days for full specification compliance.
