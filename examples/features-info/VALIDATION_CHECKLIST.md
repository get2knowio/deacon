# Features Info Examples - Validation Checklist

Use this checklist to validate that all examples are complete and working correctly.

## Pre-Validation Setup

- [ ] Build the CLI: `cd /workspaces/deacon-project/worktrees-deacon/004-features-info-cmd && cargo build --release`
- [ ] Add to PATH: `export PATH="/workspaces/deacon-project/worktrees-deacon/004-features-info-cmd/target/release:$PATH"`
- [ ] Verify deacon available: `deacon --version`
- [ ] Set network tests (optional): `export DEACON_NETWORK_TESTS=1`

## Directory Structure Validation

- [ ] All 12 example directories exist
- [ ] Each directory has README.md
- [ ] Local feature examples have devcontainer-feature.json
- [ ] Local feature examples have install.sh
- [ ] Supporting docs exist (QUICK_REFERENCE.md, VISUAL_GUIDE.md, IMPLEMENTATION_SUMMARY.md)
- [ ] test-all-examples.sh exists and is executable

## Documentation Validation

### Main README.md
- [ ] Index section lists all examples
- [ ] Quick start section includes features-info examples
- [ ] User story sections are present
- [ ] Edge cases section is present
- [ ] Running examples section is present
- [ ] Notes section explains network test gating

### Individual Example READMEs
For each example, verify:
- [ ] Description section
- [ ] Use Case section
- [ ] Prerequisites section
- [ ] Running section with exact commands
- [ ] Expected Output section
- [ ] Success Criteria section (with checkboxes)
- [ ] Related Examples section (if applicable)
- [ ] Files section (for local feature examples)

### Supporting Docs
- [ ] QUICK_REFERENCE.md has command syntax table
- [ ] QUICK_REFERENCE.md has common patterns
- [ ] QUICK_REFERENCE.md has error handling section
- [ ] VISUAL_GUIDE.md has relationship diagram
- [ ] VISUAL_GUIDE.md has learning path
- [ ] VISUAL_GUIDE.md has comparison matrix
- [ ] IMPLEMENTATION_SUMMARY.md has coverage mapping
- [ ] IMPLEMENTATION_SUMMARY.md has test statistics

## Content Validation

### User Story 1: Manifest & Canonical ID
- [ ] manifest-public-registry: Command works, shows boxed sections
- [ ] manifest-local-feature: Works offline, shows "(local feature)"
- [ ] manifest-local-feature: JSON mode shows `canonicalId: null`
- [ ] manifest-json-output: JSON is valid, has both keys

### User Story 2: Published Tags
- [ ] tags-public-feature: Shows boxed tag list
- [ ] tags-public-feature: Tags are sorted
- [ ] tags-json-output: JSON is valid, has publishedTags array

### User Story 3: Dependencies
- [ ] dependencies-simple: Shows boxed Mermaid graph
- [ ] dependencies-simple: Graph includes solid and dotted arrows
- [ ] dependencies-simple: JSON mode produces error + `{}`
- [ ] dependencies-complex: Shows more complex graph
- [ ] dependencies-complex: Graph is valid Mermaid syntax

### User Story 4: Verbose
- [ ] verbose-text-output: Shows 3 boxed sections in order
- [ ] verbose-text-output: Sections are Manifest, Tags, Dependencies
- [ ] verbose-json-output: JSON has manifest, canonicalId, publishedTags
- [ ] verbose-json-output: JSON does NOT have dependencies

### Edge Cases
- [ ] error-handling-invalid-ref: Invalid ref produces `{}` in JSON
- [ ] error-handling-invalid-ref: Exit code is 1
- [ ] error-handling-network-failure: Timeout behavior documented
- [ ] local-feature-only-manifest: Manifest mode works
- [ ] local-feature-only-manifest: Tags mode fails gracefully
- [ ] local-feature-only-manifest: Dependencies mode fails gracefully
- [ ] local-feature-only-manifest: Verbose mode fails gracefully

## Automated Testing

- [ ] test-all-examples.sh runs without errors
- [ ] Script reports passed/failed/skipped counts
- [ ] Script shows color-coded output
- [ ] All offline tests pass (network tests may be skipped)
- [ ] Network tests pass when DEACON_NETWORK_TESTS=1 (if network available)
- [ ] Script exits with code 0 on success
- [ ] Script exits with code 1 if any test fails

## Cross-Reference Validation

- [ ] Main examples/README.md updated with features-info section
- [ ] Main examples/README.md has quick start commands
- [ ] Main examples/README.md has notes about features-info
- [ ] All internal cross-references work (Related Examples links)
- [ ] All spec references are correct

## Manual Testing

### Offline Tests (No network required)
Run these commands and verify output:

```bash
cd examples/features-info

# US1: Local manifest
cd manifest-local-feature
deacon features info manifest ./sample-feature
# ✓ Should show boxed Manifest and Canonical Identifier "(local feature)"

deacon features info manifest ./sample-feature --output-format json | jq '.canonicalId'
# ✓ Should output: null

# US3: Simple dependencies
cd ../dependencies-simple
deacon features info dependencies ./my-feature
# ✓ Should show boxed Mermaid graph

deacon features info dependencies ./my-feature --output-format json
# ✓ Should output: {} (and exit 1)

# US3: Complex dependencies
cd ../dependencies-complex
deacon features info dependencies ./app-feature
# ✓ Should show more complex Mermaid graph

# Edge: Local feature limits
cd ../local-feature-only-manifest
deacon features info manifest ./local-feature
# ✓ Should work

deacon features info tags ./local-feature
# ✓ Should fail with clear error

deacon features info dependencies ./local-feature
# ✓ Should fail with clear error

# Edge: Invalid ref
cd ../error-handling-invalid-ref
deacon features info manifest invalid-ref --output-format json
# ✓ Should output: {} (and exit 1)
```

### Network Tests (Requires DEACON_NETWORK_TESTS=1)
```bash
export DEACON_NETWORK_TESTS=1
cd examples/features-info

# US1: Public manifest
cd manifest-public-registry
deacon features info manifest ghcr.io/devcontainers/features/node:1
# ✓ Should show boxed sections with real data

# US1: JSON manifest
cd ../manifest-json-output
deacon features info manifest ghcr.io/devcontainers/features/node:1 --output-format json | jq '.canonicalId'
# ✓ Should output canonical ID with @sha256:

# US2: Tags
cd ../tags-public-feature
deacon features info tags ghcr.io/devcontainers/features/node
# ✓ Should show boxed list of tags

# US2: Tags JSON
cd ../tags-json-output
deacon features info tags ghcr.io/devcontainers/features/node --output-format json | jq '.publishedTags | length'
# ✓ Should output number > 0

# US4: Verbose
cd ../verbose-text-output
deacon features info verbose ghcr.io/devcontainers/features/node:1
# ✓ Should show 3 boxed sections

# US4: Verbose JSON
cd ../verbose-json-output
deacon features info verbose ghcr.io/devcontainers/features/node:1 --output-format json | jq 'keys'
# ✓ Should show: ["canonicalId", "manifest", "publishedTags"]
```

## File Validation

### Verify all files exist:
```bash
cd examples/features-info

# Check structure
find . -type f -name "README.md" | wc -l
# ✓ Should be 13 (main + 12 examples)

find . -type f -name "devcontainer-feature.json" | wc -l
# ✓ Should be 4 (local feature examples)

find . -type f -name "install.sh" | wc -l
# ✓ Should be 4 (local feature examples)

# Check supporting docs
ls -1 README.md QUICK_REFERENCE.md VISUAL_GUIDE.md IMPLEMENTATION_SUMMARY.md test-all-examples.sh
# ✓ All should exist
```

### Verify permissions:
```bash
ls -l test-all-examples.sh
# ✓ Should be executable (-rwxr-xr-x)

ls -l */*/install.sh
# ✓ All should be executable
```

## Quality Checks

### Markdown Linting
```bash
# Check for broken internal links
for readme in */README.md; do
  echo "Checking $readme"
  grep -o '\[.*\](.*\.md)' "$readme" | grep -v '^#' || true
done
# ✓ Verify all referenced files exist
```

### JSON Validation
```bash
# Validate all JSON files
find . -name "*.json" -exec echo "Validating: {}" \; -exec jq empty {} \;
# ✓ All should pass without errors
```

### Shell Script Validation
```bash
# Check shell scripts syntax
bash -n test-all-examples.sh
find . -name "install.sh" -exec bash -n {} \;
# ✓ No syntax errors
```

## Integration Validation

- [ ] Examples align with spec.md user stories
- [ ] Examples cover all acceptance scenarios
- [ ] Examples align with tasks.md implementation
- [ ] Examples work with current CLI implementation
- [ ] No deprecated flags or commands used

## Final Checks

- [ ] All checklists in this file are complete
- [ ] test-all-examples.sh passes 100%
- [ ] No TODO or FIXME comments remain
- [ ] All example commands tested manually
- [ ] Documentation is clear and complete

---

## Sign-Off

Date: ___________  
Validated by: ___________  
Notes: ___________

**Status**: 
- [ ] Ready for review
- [ ] Ready for merge
- [ ] Issues found (document below)

**Issues Found:**
1. 
2. 
3. 

**Resolution:**
1. 
2. 
3.
