---
description: "Error handling taxonomy using thiserror; anyhow only at binary boundaries"
---

# Error Taxonomy

- Use thiserror for domain errors; anyhow only at binary boundary
- Categories: Configuration, Docker/Runtime, Feature, Template, Network, Validation, Authentication
- Carry minimal actionable context; convert lower-level errors with #[from] as appropriate
