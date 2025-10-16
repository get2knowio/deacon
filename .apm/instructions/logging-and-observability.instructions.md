---
description: "Tracing spans, structured fields, and secret redaction"
---

# Logging & Observability

- Use tracing spans for: config.resolve, feature.install, container.create, template.apply, lifecycle.run
- Add structured fields (ids, options); avoid string concatenation
- Plan redaction for secrets; consider JSON logging mode
