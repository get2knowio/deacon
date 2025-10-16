---
description: "Module layout and responsibilities across crates"
---

# Module & Project Structure

- crates/deacon: CLI entrypoint and orchestration
- crates/core: shared logic and models (planned/expanding)
- Group commands under a `commands` module; tests in crates/deacon/tests/
