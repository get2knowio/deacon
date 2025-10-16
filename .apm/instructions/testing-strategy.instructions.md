---
description: "Unit/integration testing approach with deterministic, hermetic tests"
---

# Testing Strategy

- Unit tests for pure logic
- Integration tests for cross-process/runtime boundaries
- Smoke tests in crates/deacon/tests/smoke_basic.rs for CLI
- Deterministic tests; Docker-unavailable paths acceptable with defined errors
- Update fixtures/ and examples/ when behavior changes
