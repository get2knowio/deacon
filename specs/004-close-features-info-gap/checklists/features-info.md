# Acceptance Checklist: Features Info Subcommand

**Purpose**: Validate implementation against user story acceptance scenarios
**Created**: 2025-11-02
**Feature**: ../spec.md

## User Story 1 - Inspect manifest and canonical ID

- [ ] Given a public feature ref (e.g., `ghcr.io/devcontainers/features/node:1`), when running in text mode, then the CLI prints a boxed "Manifest" section with formatted JSON and a boxed "Canonical Identifier" section including registry, path, and digest (e.g., `@sha256:...`).
- [ ] Given the same ref, when running with `--output-format json`, then output is a single JSON object containing keys `manifest` and `canonicalId` and exit code is 0.
- [ ] Given a non-existent ref, when running with `--output-format json`, then the CLI prints `{}` and exits with code 1.
- [ ] Given a local feature path without an OCI digest, when running with `--output-format json`, then output contains `manifest` and `"canonicalId": null`, and exit code is 0.

## User Story 2 - Discover published tags

- [ ] Given a public feature ref pointing to a repository, when running in text mode, then the CLI prints a boxed "Published Tags" section listing tags sorted lexicographically or registry order.
- [ ] Given the same ref, when running with `--output-format json`, then output contains `{ "publishedTags": [ ... ] }` and exit code 0.
- [ ] Given a repo with no tags or inaccessible registry, when running in JSON mode, then the CLI prints `{}` and exits with code 1.

## User Story 3 - Visualize dependency graph

- [ ] Given a feature with `dependsOn` and/or `installsAfter`, when running in text mode, then the CLI prints a boxed section titled "Dependency Tree (Render with https://mermaid.live/)" followed by valid Mermaid `graph TD` syntax.
- [ ] Given any ref, when running with `--output-format json` and `mode=dependencies`, then the CLI exits 1 with an explanatory error in text mode or `{}` in JSON mode.

## User Story 4 - Combined verbose view

- [X] Given a valid ref, when running in text mode, then the CLI outputs three boxed sections in order: Manifest/Canonical Identifier, Published Tags, Dependency Tree.
- [X] Given the same ref, when running with `--output-format json`, then output is a single JSON object union of manifest/canonicalId and publishedTags; no dependency graph is included.
- [X] Given a valid ref where dependency graph generation fails, when running with `--output-format json`, then output includes successfully retrieved fields (manifest/canonicalId and/or publishedTags as applicable), plus an `errors` object containing a `dependencies` key with a brief message, and the process exits with code 1.
- [X] Given a valid ref where at least one sub-mode fails (e.g., tags listing times out), when running with `--output-format json`, then output includes the successfully retrieved fields and an `errors` object keyed by sub-mode (e.g., `{"errors": {"tags": "<message>"}}`) and the process exits with code 1.

## Edge Cases

- [ ] Private registries require authentication: on auth failure, return `{}` in JSON mode (exit 1) or clear text error in text mode.
- [ ] Very large tag lists: handle pagination via `Link` headers; enforce defaults of 10 pages max and 1000 tags total; ensure no infinite loops.
- [ ] Invalid feature reference format: exit 1; `{}` for JSON mode.
- [ ] Local feature paths without OCI manifest: manifest mode should read local metadata when applicable; in JSON include `"canonicalId": null`.
- [ ] Network failures/timeouts: exit 1; JSON mode outputs `{}`.
- [ ] Verbose JSON partial failure: include partial fields plus an `errors` object; exit 1.
