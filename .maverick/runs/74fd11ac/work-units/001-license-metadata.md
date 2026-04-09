---
work-unit: license-metadata
flight-plan: consumer-core-completion
sequence: 1
depends-on: []
parallel-group: alpha
---

## Task

Update Cargo.toml license fields to MIT across all workspace members, verify LICENSE file and README consistency

## Acceptance Criteria

- All Cargo.toml files specify license = MIT (directly or via workspace inheritance) [SC-017]
- LICENSE file at workspace root contains MIT text [SC-018]
- README.md license badge is consistent with MIT [SC-018]
- cargo build succeeds with no metadata warnings [SC-019]

## File Scope

### Create


### Modify

- Cargo.toml
- crates/core/Cargo.toml
- crates/deacon/Cargo.toml
- LICENSE

### Protect


## Procedure

### Step 1: Read current license state
- MUST Read Cargo.toml (root workspace), crates/core/Cargo.toml, and crates/deacon/Cargo.toml
- MUST Read LICENSE file at workspace root
- MUST Grep README.md for any license references

### Step 2: Verify license fields
- MUST verify root Cargo.toml line 10 contains license = MIT under [workspace.package]
- MUST verify crates/core/Cargo.toml line 5 contains license.workspace = true
- MUST verify crates/deacon/Cargo.toml line 5 contains license.workspace = true
- IF any Cargo.toml has Apache-2.0 or any license other than MIT, MUST update to MIT
- IF any crate Cargo.toml does NOT inherit from workspace, MUST change to license.workspace = true

### Step 3: Verify LICENSE file
- MUST verify LICENSE file contains MIT License text
- IF LICENSE contains Apache-2.0 text, MUST replace with MIT License text for copyright holder get2know.io

### Step 4: Verify README consistency
- MUST verify README.md license badge references MIT
- IF badge references Apache-2.0, MUST update to MIT

### Step 5: Verify build
- MUST run cargo build --quiet to confirm no metadata errors

## Verification

- cargo build --quiet 2>&1 | tail -1
- head -1 LICENSE | grep -q MIT && echo PASS || echo FAIL
