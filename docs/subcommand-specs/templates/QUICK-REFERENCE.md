# Templates Subcommand - Quick Reference Card

**Purpose**: At-a-glance comparison between specification and implementation.  
**For**: Developers working on templates implementation  
**Updated**: October 13, 2025

---

## 🎯 Command Signatures

### Apply

```bash
# SPEC SAYS:
deacon templates apply \
  --workspace-folder /path/to/workspace \
  --template-id ghcr.io/owner/templates/rust:latest \
  --template-args '{"projectName":"myapp"}' \
  --features '[{"id":"ghcr.io/features/git:1","options":{}}]' \
  --omit-paths '["tests/*","examples/*"]'

# WE HAVE:
deacon templates apply \
  /path/to/template \
  --output /path/to/workspace \
  --option projectName=myapp \
  --force \
  --dry-run
```

**Gaps**: Wrong flags, wrong option format, missing features, missing omit-paths, missing JSON output

---

### Publish

```bash
# SPEC SAYS:
deacon templates publish ./src \
  --namespace owner/repo \
  --registry ghcr.io

# WE HAVE:
deacon templates publish /path/to/template \
  --registry ghcr.io/owner/repo \
  --dry-run \
  --username user \
  --password-stdin
```

**Gaps**: Missing `--namespace`, wrong registry format, missing collection support, missing semantic tags

---

### Metadata

```bash
# SPEC SAYS:
deacon templates metadata ghcr.io/owner/templates/rust:latest

# WE HAVE:
deacon templates metadata /path/to/template
```

**Gaps**: Takes local path instead of OCI reference, doesn't query registry

---

### Generate Docs

```bash
# SPEC SAYS:
deacon templates generate-docs \
  --project-folder ./my-templates \
  --github-owner myorg \
  --github-repo my-templates-repo

# WE HAVE:
deacon templates generate-docs /path/to/template \
  --output /path/to/output
```

**Gaps**: Missing GitHub flags, different argument style

---

## 📦 Data Structures

### Template Option Substitution

```bash
# SPEC SAYS:
# In template files:
FROM ubuntu:${templateOption:baseImageVersion}
LABEL maintainer="${templateOption:maintainerEmail}"

# Provided via:
--template-args '{"baseImageVersion":"22.04","maintainerEmail":"dev@example.com"}'

# WE HAVE:
# Uses general variable substitution, NOT templateOption: namespace
FROM ubuntu:${localWorkspaceFolder}  # ✅ Works
FROM ubuntu:${templateOption:version}  # ❌ Doesn't work
```

**Action Required**: Add `templateOption:` prefix support to variable substitution

---

### Feature Injection

```bash
# SPEC SAYS:
--features '[
  {"id":"ghcr.io/devcontainers/features/git:1","options":{}},
  {"id":"ghcr.io/devcontainers/features/node:1","options":{"version":"18"}}
]'

# WE HAVE:
# Not supported
```

**Action Required**: Parse JSON array, inject into `devcontainer.json` after template application

---

### Output Formats

```bash
# SPEC SAYS (apply):
{"files":["Dockerfile","devcontainer.json","src/main.rs"]}

# WE HAVE:
# Logs to stderr, no structured JSON output

# SPEC SAYS (publish):
{
  "rust-template": {
    "publishedTags": ["1", "1.2", "1.2.3", "latest"],
    "digest": "sha256:abcd...",
    "version": "1.2.3"
  },
  "python-template": {
    "publishedTags": ["2", "2.0", "2.0.1", "latest"],
    "digest": "sha256:ef01...",
    "version": "2.0.1"
  }
}

# WE HAVE:
{
  "command": "publish",
  "status": "success",
  "digest": "sha256:...",
  "size": 1024
}
```

**Action Required**: Fix both output formats

---

## 🔍 Critical Functions to Modify

### In `crates/deacon/src/cli.rs`

```rust
// CURRENT:
pub enum TemplateCommands {
    Apply {
        template: String,  // ❌ Should be --template-id
        option: Vec<String>,  // ❌ Should be --template-args JSON
        output: Option<String>,  // ❌ Should be --workspace-folder
        force: bool,  // ⚠️ Not in spec
        dry_run: bool,  // ⚠️ Not in spec
    },
    // ... other commands
}

// SHOULD BE:
pub enum TemplateCommands {
    Apply {
        #[arg(long = "template-id", short = 't')]
        template_id: String,
        
        #[arg(long = "workspace-folder", short = 'w', default_value = ".")]
        workspace_folder: String,
        
        #[arg(long = "template-args", short = 'a', default_value = "{}")]
        template_args: String,  // Parse as JSON
        
        #[arg(long = "features", short = 'f', default_value = "[]")]
        features: String,  // Parse as JSON array
        
        #[arg(long = "omit-paths")]
        omit_paths: Option<String>,  // Parse as JSON array
        
        #[arg(long = "tmp-dir")]
        tmp_dir: Option<String>,
        
        // Keep extensions but mark as such:
        #[arg(long)]
        force: bool,
        
        #[arg(long)]
        dry_run: bool,
    },
    // ...
}
```

---

### In `crates/core/src/variable.rs`

```rust
// ADD THIS:
impl VariableSubstitution {
    pub fn substitute_string(
        input: &str,
        context: &SubstitutionContext,
        report: &mut SubstitutionReport,
    ) -> String {
        // ... existing code ...
        
        // ADD: Handle ${templateOption:KEY}
        if let Some(captures) = TEMPLATE_OPTION_RE.captures(input) {
            let option_key = &captures[1];
            if let Some(template_opts) = &context.template_options {
                if let Some(value) = template_opts.get(option_key) {
                    // Replace with option value
                    // ... record in report ...
                }
            }
        }
        
        // ... existing code ...
    }
}
```

---

### In `crates/deacon/src/commands/templates.rs`

```rust
// ADD THIS:
async fn inject_features_into_devcontainer(
    workspace: &Path,
    features: &[TemplateFeatureOption],
) -> Result<()> {
    // 1. Find devcontainer.json in workspace
    // 2. Parse JSONC
    // 3. Ensure "features" object exists
    // 4. For each feature: features[feature.id] = feature.options || {}
    // 5. Write back with formatting preserved
}

// MODIFY THIS:
async fn execute_templates_apply(...) -> Result<()> {
    // ... existing template application ...
    
    // ADD: If features provided, inject them
    if !features.is_empty() {
        inject_features_into_devcontainer(&output_dir, &features)?;
    }
    
    // ADD: Output structured JSON
    let output = serde_json::json!({
        "files": result.files  // Collect during apply
    });
    println!("{}", serde_json::to_string(&output)?);
    
    Ok(())
}
```

---

## 🧪 Test Cases to Add

### Apply Tests

```rust
#[test]
fn test_apply_with_template_args_json() {
    let json_args = r#"{"version":"1.2.3","name":"myapp"}"#;
    // Test that JSON is parsed and substituted in template files
}

#[test]
fn test_apply_with_features() {
    let features_json = r#"[{"id":"ghcr.io/features/git:1","options":{}}]"#;
    // Test that features are injected into devcontainer.json
}

#[test]
fn test_apply_with_omit_paths() {
    let omit_json = r#"["tests/*","examples/*"]"#;
    // Test that specified paths are excluded
}

#[test]
fn test_apply_json_output() {
    // Test that output is valid JSON with "files" array
}

#[test]
fn test_template_option_substitution() {
    // Template file contains: ${templateOption:projectName}
    // Args contain: {"projectName":"myproject"}
    // Verify substitution occurs
}
```

### Publish Tests

```rust
#[test]
fn test_publish_semantic_tags() {
    // Template with version: "1.2.3"
    // Verify tags pushed: ["1", "1.2", "1.2.3", "latest"]
}

#[test]
fn test_publish_collection() {
    // Directory with src/template1, src/template2
    // Verify both are published + collection metadata
}

#[test]
fn test_publish_output_format() {
    // Verify output is map of template IDs to results
}
```

### Metadata Tests

```rust
#[test]
fn test_metadata_from_registry() {
    // Given OCI reference
    // Verify manifest is fetched and annotation parsed
}

#[test]
fn test_metadata_missing_annotation() {
    // Verify outputs {} and exits non-zero
}
```

---

## 📋 Implementation Checklist

Copy this to your task tracker:

```
[ ] Phase 1: CLI Interface
  [ ] Update TemplateCommands::Apply flags
  [ ] Update TemplateCommands::Publish flags
  [ ] Update TemplateCommands::Metadata to accept OCI ref
  [ ] Update TemplateCommands::GenerateDocs flags
  [ ] Update argument parsing and validation
  
[ ] Phase 2: Template Options
  [ ] Add templateOption: pattern to variable substitution
  [ ] Update SubstitutionContext to handle template options
  [ ] Test ${templateOption:KEY} substitution
  
[ ] Phase 3: Features
  [ ] Parse --features JSON array
  [ ] Implement inject_features_into_devcontainer()
  [ ] Handle missing devcontainer.json gracefully
  [ ] Test feature injection
  
[ ] Phase 4: Omit Paths
  [ ] Add omit_paths to ApplyOptions
  [ ] Implement glob matching in plan_template_application
  [ ] Include reserved files in omit set
  [ ] Test omit paths
  
[ ] Phase 5: Outputs
  [ ] Collect applied file paths in apply
  [ ] Output { files: [] } JSON to stdout
  [ ] Fix publish output to be map of results
  [ ] Ensure logs go to stderr only
  
[ ] Phase 6: Semantic Versioning
  [ ] Parse template version as semver
  [ ] Fetch existing tags from registry
  [ ] Compute tags to push (major, minor, patch, latest)
  [ ] Push manifest to multiple tags
  [ ] Include publishedTags in output
  
[ ] Phase 7: Collections
  [ ] Detect collection vs single template
  [ ] Iterate through src/ subdirectories
  [ ] Package and publish each template
  [ ] Generate devcontainer-collection.json
  [ ] Publish collection metadata
  
[ ] Phase 8: Metadata from Registry
  [ ] Rewrite metadata command to fetch from OCI
  [ ] Parse manifest annotations
  [ ] Return {} on missing annotation
  [ ] Test with real registry
  
[ ] Phase 9: Testing
  [ ] Add all missing test cases
  [ ] Integration tests for full workflows
  [ ] Test error cases
  
[ ] Phase 10: Documentation
  [ ] Update CLI help text
  [ ] Add examples matching spec
  [ ] Write migration guide
  [ ] Update README
```

---

## 🚀 Quick Start for Contributors

1. **Read these docs first**:
   - [SPEC.md](./SPEC.md) - The source of truth
   - [IMPLEMENTATION-GAP-ANALYSIS.md](./IMPLEMENTATION-GAP-ANALYSIS.md) - Detailed gaps
   - This file - Quick reference

2. **Start with quick wins**:
   - Add `--github-owner` and `--github-repo` to generate-docs
   - Add structured JSON output to apply
   - Add `${templateOption:KEY}` substitution

3. **Tackle hard problems next**:
   - Refactor CLI interface
   - Implement feature injection
   - Add semantic versioning

4. **Test extensively**:
   - Use fixtures in `fixtures/templates/`
   - Test against real OCI registry
   - Verify output formats match spec exactly

---

## 📞 Questions?

- Spec unclear? Check [SPEC.md](./SPEC.md) or the reference implementation
- Implementation details? See [IMPLEMENTATION-GAP-ANALYSIS.md](./IMPLEMENTATION-GAP-ANALYSIS.md)
- Current status? Check [IMPLEMENTATION-STATUS.md](./IMPLEMENTATION-STATUS.md)
- Need help? Ask in issue #XXX (create if needed)

---

**Remember**: The spec is the source of truth. When in doubt, match the spec exactly.
