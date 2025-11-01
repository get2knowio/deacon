# Quickstart — Features Info

This guide shows how to use the Features Info subcommand once implemented.

## Prerequisites
- Deacon CLI built from this repository
- Network access for registry queries

## Manifest and Canonical ID

Text mode:

```sh
# Prints boxed sections: Manifest and Canonical Identifier
 deacon features info manifest ghcr.io/devcontainers/features/node:1
```

JSON mode:

```sh
# Prints a single JSON document with { manifest, canonicalId }
 deacon features info manifest ghcr.io/devcontainers/features/node:1 --output-format json
```

## Published Tags

```sh
# Text mode prints a boxed "Published Tags" list; JSON mode prints { publishedTags: [...] }
 deacon features info tags ghcr.io/devcontainers/features/node
 deacon features info tags ghcr.io/devcontainers/features/node --output-format json
```

## Dependency Graph (text only)

```sh
# Emits Mermaid graph; copy into https://mermaid.live/ to render
 deacon features info dependencies ghcr.io/devcontainers/features/node:1
```

## Verbose (combined)

```sh
# Text mode: three boxed sections (manifest/canonicalId, tags, dependency graph)
 deacon features info verbose ghcr.io/devcontainers/features/node:1

# JSON mode: union of manifest/canonicalId and publishedTags. If any sub-mode fails,
# include { errors: { <mode>: "message" } } and exit with code 1.
 deacon features info verbose ghcr.io/devcontainers/features/node:1 --output-format json
```

## Exit Codes
- 0 on success
- 1 on any error

## Notes
- For local feature paths, `canonicalId` is `null` in JSON mode.
- Pagination limits: up to 10 pages or 1000 tags; per-request timeout 10s.
