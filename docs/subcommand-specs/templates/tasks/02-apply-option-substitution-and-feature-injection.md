---
subcommand: templates
type: enhancement
priority: high
scope: medium
labels: ["subcommand: templates", "type: enhancement", "priority: high", "scope: medium"]
---

# [templates] Implement `${templateOption:KEY}` Substitution and Feature Injection in Apply

## Issue Type
- [x] Core Logic Implementation
- [x] Error Handling
- [x] Testing & Validation

## Description
Implement template option substitution using the `${templateOption:KEY}` pattern across all text files during `templates apply` and support `--features` injection into `devcontainer.json`. This brings the apply behavior in line with the reference CLI and enables parameterized templates and integrated features.

## Specification Reference

**From SPEC.md Section:** §4 Configuration Resolution (Variable Substitution), §5 Core Execution Logic (apply)

**From GAP.md Section:** 2.1 Template Option Substitution; 2.2 Feature Injection

### Expected Behavior
- Before substitution, merge provided `--template-args` with defaults from `devcontainer-template.json` (booleans become "true"/"false" strings).
- Replace occurrences of `${templateOption:KEY}` with the final string value for KEY across all substituted files.
- If `--features` present, locate a `devcontainer.json` in the applied set and set `features[id] = options || {}`; create `features` object if missing. If no `devcontainer.json` present, log an error but do not fail apply.

### Current Behavior
- Variable engine already supports `templateOption:` namespace in `VariableSubstitution::resolve_variable`, but core apply path does not wire defaults from `devcontainer-template.json` nor feature injection. No `--features` flag at CLI yet (addressed in Task 01).

## Implementation Requirements

### 1. Code Changes Required

#### Files to Modify
- `crates/core/src/templates.rs`
  - Merge default option values from parsed `devcontainer-template.json` with CLI-provided options into `context.template_options`.
  - After `execute_planned_actions`, if `features` exist (from CLI layer), open the written `devcontainer.json` (if any), parse as JSONC/JSON, add features, and write back.
- `crates/deacon/src/commands/templates.rs`
  - Pass parsed `TemplateFeatureOption[]` into core (either extend `ApplyOptions` or pass as separate param).

#### Specific Tasks
- [ ] Parse template metadata from source and collect default values for options.
- [ ] Merge defaults with provided `--template-args` into `HashMap<String, String>` and set `context.template_options` accordingly.
- [ ] Implement helper to find `devcontainer.json` under workspace and inject features.
- [ ] Preserve JSONC comments if feasible; if not available yet, parse as JSON and re-emit (document limitation for now).

### 2. Data Structures

From DATA-STRUCTURES.md:
```rust
pub type TemplateOptions = std::collections::HashMap<String, String>;

pub struct TemplateFeatureOption {
    pub id: String,
    pub options: std::collections::HashMap<String, serde_json::Value>,
}
```

### 3. Validation Rules
- [ ] All required options (no default) must be present; if missing, emit error: "Missing required option '<name>'. Provide a value with --template-args or define a default."
- [ ] For features, `id` must be non-empty string; `options` if present must be an object.

### 4. Cross-Cutting Concerns

- [ ] Theme 1 - JSON Output Contract remains unchanged from Task 01.
- [ ] Theme 6 - Error messages must follow spec phrasing.

## Testing Requirements

### Unit Tests (core)
- [ ] Verify merging defaults and provided options; boolean defaults become strings.
- [ ] Substitution replaces `${templateOption:...}` tokens across multiple file types.

### Integration Tests (CLI)
- [ ] Apply with `--template-args` JSON and verify substitutions.
- [ ] Apply with `--features` JSON and verify `devcontainer.json` augmentation.

### Smoke Tests
- [ ] Extend smoke to assert at least one substitution occurred and features injected when `devcontainer.json` exists.

### Examples
- [ ] Update or add `examples/template-management/template-with-options/` showcasing `${templateOption:KEY}` and `--features`.

## Acceptance Criteria
- [ ] Options defaulting and `${templateOption:KEY}` substitution implemented.
- [ ] Feature injection into `devcontainer.json` implemented.
- [ ] Tests pass and CI green.

## Implementation Notes
- If `devcontainer.json` is not found, log error per spec but do not fail apply.
- Consider adding a minimal JSONC parser later; initial implementation can use strict JSON parsing with a note.

## Definition of Done
- [ ] Substitution and features injection behave per spec.
- [ ] Tests and examples updated.
