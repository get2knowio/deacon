---
description: "Guidance for introducing async and isolating IO for testability"
---

# Asynchronicity & IO

- Introduce async only for IO-bound work; keep pure logic sync
- Isolate filesystem/network calls for testability
