---
issue: 286
title: "[read-configuration] Implement Full Parity for `read-configuration`"
---

## Issue Body

## Dependencies Overview

Recommended execution order (Blocked By relationships reflected in child issues):
1. #287 — CLI flags: c
ontainer selection and selector validation
2. #292 — CLI flags: Docker tooling paths and mount/workspace options (parallel)
3. #293 — CLI flags: terminal dimensions (parallel)
4. #289 — Features resolution and flags (depends on flags)
5. #288 — Container discovery and substitutions (depends on container flags)
6. #295 — Workspace output and mount semantics (depends on mount flag)
7. #290 — Merged configuration algorithm (depends on container discovery + features)
8. #291 — Output payload structure (depends on features/merged/workspace)
9. #294 — Validation & exact error messages (ties together; depends on flags)
10. #296 — Tests and examples (final polish; depends on all above)
11. #297 — Governance/docs (runs alongside output change planning)
