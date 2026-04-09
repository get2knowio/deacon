---
work-unit: feature-install-timing-research
flight-plan: consumer-core-completion
sequence: 5
depends-on: []
parallel-group: alpha
---

## Task

Research current feature installation flow: map code paths in feature_installer.rs vs features_build.rs, identify where in-container installation still occurs, document required changes to move all feature installation to image build phase

## Acceptance Criteria

- Research document maps all feature installation code paths with file:line references [SC-013]
- Document identifies whether feature_installer.rs is orphaned or still used [SC-013]
- Document describes feature options ENV var handling [SC-014]
- Document confirms or identifies gaps in the no-features skip path [SC-015]
- Document provides actionable recommendations for the implementation bead [SC-016]

## File Scope

### Create

- .maverick/context/feature-install-timing-research.md

### Modify


### Protect


## Procedure

### Step 1: Map current feature installation code paths
- MUST Read crates/deacon/src/commands/up/features_build.rs full file (BuildKit-based feature build)
- MUST Read crates/core/src/feature_installer.rs full file (older in-container installer)
- MUST Read crates/deacon/src/commands/up/container.rs lines 180-230 to confirm features built BEFORE container creation
- MUST Grep for all imports/usages of feature_installer across workspace
- MUST Grep for all imports/usages of features_build to confirm active path
- MUST Read crates/core/src/lib.rs to check if feature_installer module is publicly exported

### Step 2: Analyze feature options ENV var handling
- MUST Read features_build.rs for how feature options become ENV vars in Dockerfile
- MUST document Dockerfile structure: FROM base, then per-feature ENV + COPY + RUN

### Step 3: Analyze cache behavior
- MUST search features_build.rs for cache-related logic
- MUST document whether deterministic layers are achieved

### Step 4: Identify no-features path
- MUST confirm container.rs lines 193-222 skip feature build when config.features is empty

### Step 5: Document findings
- MUST Write to .maverick/context/feature-install-timing-research.md with sections: Current State, Orphaned Code, Feature Options, Cache Behavior, No-Features Path, Gaps Found, Recommended Changes

## Verification

- test -f .maverick/context/feature-install-timing-research.md && echo PASS || echo FAIL
