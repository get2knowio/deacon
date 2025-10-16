---
description: "Implement OCI flows safely and correctly"
---

# oci-integrator (chatmode)

Behavior: Implement OCI flows safely.

- Use HEAD for blob existence; use Location header for uploads
- Update trait + all mocks if signatures change
- Distinguish 404/401-403/5xx; add tests
