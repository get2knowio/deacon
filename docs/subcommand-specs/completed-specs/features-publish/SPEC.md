# Features Publish Subcommand Design Specification

## 1. Subcommand Overview
- Purpose: Publish packaged Dev Container Features and collection metadata to an OCI registry with appropriate semantic tags and digests.
- User Personas:
  - Feature authors/maintainers: Release new versions and update ‘latest’, major, minor tags.
  - CI engineers: Automate releases and handle idempotent publish attempts.
- Specification References:
  - Features distribution over OCI: containers.dev implementors spec (OCI Registry)
  - Collection metadata push semantics.
- Related Commands:
  - `features package`: Creates the artifacts to publish.
  - `features info`: Verify manifests and tags after publish.

## 2. Command-Line Interface
- Full Syntax:
  - `devcontainer features publish [target] --registry <host> --namespace <owner/repo> [--log-level <lvl>]`
- Flags and Options (from TS CLI):
  - `--registry, -r <host>`: Registry hostname (default `ghcr.io`).
  - `--namespace, -n <owner/repo>`: Collection namespace (required).
  - `--log-level <info|debug|trace>`: Logging level (default `info`).
  - Positional `target` (default `.`): Same semantics as `package`.
- Flag Taxonomy:
  - Required: `--namespace`.
  - Optional: `--registry`, `--log-level`, positional target.
  - Mutually exclusive: n/a.
- Argument Validation Rules:
  - Requires packaged output; if invoked directly, implementation will package into a temp folder before publishing.
  - Registry auth must be provided (see Security).

## 3. Input Processing Pipeline

```pseudocode
FUNCTION parse_command_arguments(args) -> ParsedInput:
    input.target = args.positional('target') OR '.'
    input.registry = args['--registry'] OR 'ghcr.io'
    input.namespace = require(args['--namespace'])
    input.log_level = map_log_level(args['--log-level'])
    RETURN input
END FUNCTION
```

## 4. Configuration Resolution
- Sources: Artifacts built by `package`; if absent, run packaging first into a temp dir.
- Ref Derivation:
  - Collection ref: `<registry>/<namespace>`
  - Individual feature refs derived from collection metadata (feature IDs).

## 5. Core Execution Logic

```pseudocode
FUNCTION execute_subcommand(input) -> ExecutionResult:
    logger = create_logger(level=input.log_level)
    cli_host = detect_cli_host(CWD)
    out_dir = tmp_or_given_output_dir()

    // Ensure artifacts exist
    packaged = do_features_package(target=input.target, output_dir=out_dir)
    ASSERT packaged.features NOT EMPTY

    // Determine version and semantic tags per feature
    for feature in packaged.features:
        version = feature.version
        oci_ref = make_feature_ref(input.registry, input.namespace, feature.id)
        published_tags = fetch_published_tags(oci_ref)
        tags_to_publish = compute_semantic_tags(version, published_tags)
        if tags_to_publish:
            digest = push_feature_tarball(oci_ref, out_dir, feature, tags_to_publish)
            record_result(feature.id, digest, tags_to_publish)
        else:
            log_warn('Version already exists; skipping')

  // Publish collection metadata to <registry>/<namespace>:collection
  collection_ref = make_collection_ref(input.registry, input.namespace, tag="collection")
  push_collection_metadata(
    collection_ref,
    join(out_dir, 'devcontainer-collection.json'),
    media_type='application/vnd.devcontainer.collection+json'
  )

    RETURN Success
END FUNCTION
```

## 6. State Management
- Persistent State: None; remote registry stores blobs and tags.
- Cache Management: None at CLI level; registry may cache layers.
- Lock Files: None.
- Idempotency: Safe to re‑run; existing tags are detected and skipped.

## 7. External System Interactions

### OCI Registries
- Authentication:
  - Supports `DOCKER_CONFIG` based auth or `DEVCONTAINERS_OCI_AUTH` (`host|user|pass`) environment for tests.
  - Uses registry APIs to list tags and push OCI artifacts.
- Manifest/Tags:
  - `getPublishedTags`: Retrieve current tags for feature ref.
  - `pushOCIFeatureOrTemplate`: Upload tarball and apply computed tags.
- Semantic Tagging:
  - For version `X.Y.Z`, publish tags `[X, X.Y, X.Y.Z, latest]` if not already published.

### File System
- Reads artifacts from output folder; reads `devcontainer-collection.json` for collection publishing.
  - Collection publish target: `<registry>/<namespace>:collection`
  - Media type: `application/vnd.devcontainer.collection+json`

## 8. Data Flow Diagrams

```
┌───────────────┐
│ Packaged Art. │
└──────┬────────┘
       │
       ▼
┌──────────────────────┐
│ Fetch Published Tags │
└──────┬───────────────┘
       │
       ▼
┌──────────────────────┐
│ Compute Semantic Tags│
└──────┬───────────────┘
       │
       ▼
┌──────────────────────┐
│ Push Blobs + Tags    │
└──────────────────────┘
```

## 9. Error Handling Strategy
- User Errors:
  - Missing `--namespace`: argument error.
  - Not authenticated: authentication error with hint.
- System Errors:
  - Network/registry failures: retries (library responsibility), error propagation otherwise.
- Configuration Errors:
  - Invalid semantic version: exit with error.

## 10. Output Specifications
- Text Mode: Logs steps (“Fetching published versions…”, “Publishing tags: …”). Shows success or warns when version exists.
- JSON Mode: Emit a single root object:
  - `features`: array of per-feature objects:
    - `featureId` (string)
    - `version` (string)
    - `digest` (string)
    - `publishedTags` (string[])
    - `skippedTags` (string[])
    - `movedLatest` (boolean)
    - `registry` (string)
    - `namespace` (string)
  - `collection` (object, optional): `{ "digest": string }` when collection metadata is published
  - `summary` (object): `{ "features": number, "publishedTags": number, "skippedTags": number }`
- Exit Codes:
  - `0`: success (including all‑skipped)
  - `1`: fatal error (e.g., invalid semver, no features discovered, auth failure)
- Stdout/Stderr:
  - In JSON mode, stdout contains only the JSON document; all logs to stderr.
  - On fatal errors, stdout is empty and the error is written to stderr.

## 11. Performance Considerations
- Sequential publish per feature; can be parallelized carefully (avoid tag conflicts on same repo).

## 12. Security Considerations
- Secrets Handling: Support `DOCKER_CONFIG`/credential helpers. Redact credentials from logs.
- Privilege Escalation: None.
- Input Sanitization: Validate namespace and ids to avoid injection into registry paths.

## 13. Cross-Platform Behavior
- Path handling via CLI host; registry behavior OS‑agnostic.

## 14. Edge Cases and Corner Cases
- Retrying on transient network errors.
- Mixed existing and new tags; only publish missing ones.

## 15. Testing Strategy

```pseudocode
TEST "first publish": expect tags X, X.Y, X.Y.Z, latest
TEST "re-publish same version": expect skip warning, no error
TEST "invalid version": expect error and exit 1
TEST "auth via DEVCONTAINERS_OCI_AUTH": use local registry in tests
```

## 16. Migration Notes
- None.

### Selected Design Decisions

#### Design Decision: Semantic Version Tagging
Implementation Behavior:
- Publish `major`, `major.minor`, `major.minor.patch`, `latest` if not already published.

Specification Guidance:
- Community practice; not strictly mandated by spec but used by TS CLI.

Rationale:
- Aligns discoverability with npm‑style semver ranges.

Alternatives Considered:
- Only publish exact version; fewer tags but poorer UX.

Trade-offs:
- More tags to manage; better consumer ergonomics.

