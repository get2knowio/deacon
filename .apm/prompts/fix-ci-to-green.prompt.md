---
description: Recover CI to green without changing behavior
mode: agent
tools: []
---

## Failure overview
- Gate: {{failing_gate}}
- Output excerpt:
```
{{failure_output}}
```

## Likely causes / suspected files
{{suspected_files}}

## Minimal fix plan
1. 
2. 

## Verification
- {{acceptance_criteria}}
- Re-run gates: build → tests → doctests → fmt → fmt-check → clippy

## Composition
- Role mode: ci-green-keeper
- Contexts: quality-gates, doctest-hygiene, codebase-map
- Instructions: prime-directives, quality-gates, imports-formatting-and-style
