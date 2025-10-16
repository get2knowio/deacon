---
description: "OCI registry HTTP client usage: HEAD checks, Location handling, status codes"
---

# OCI HTTP Client Guidelines

- Use HEAD to check blob existence; avoid GET for presence checks
- Respect Location header for upload flows; do not hardcode URLs
- If trait signatures change, update all impls and mocks
- Distinguish 404 (not found), 401/403 (auth), 5xx (server)
