# Templates Subcommand — Detailed Design Specification

This document specifies the DevContainer CLI `templates` command group, covering `apply`, `publish`, `metadata`, and `generate-docs`. It reverse‑engineers the TypeScript reference implementation and aligns behavior with the Development Containers specification, using language‑agnostic pseudocode and explicit algorithms.

## 1. Subcommand Overview
- Purpose: Manage Dev Container Templates end‑to‑end: fetch/apply a template into a workspace, publish templates to an OCI registry, retrieve published template metadata, and generate authoring documentation.
- User Personas:
  - Template consumers: quickly scaffold a project via a published template.
  - Template authors: package, publish, and document templates.
  - CI maintainers: automate publish/promotion and docs generation.
- Specification References:
  - Dev Containers Templates: https://containers.dev/implementors/templates
  - OCI Distribution Spec (pull manifests/layers): https://github.com/opencontainers/distribution-spec
  - OCI Image Spec (manifest, image index): https://github.com/opencontainers/image-spec
- Related Commands:
  - `features publish|test|package|plan|info` for Features authoring and distribution
  - `read-configuration`, `up`, `build` to consume resultant configuration after applying templates

## 2. Command-Line Interface

Full command group signature (reference CLI semantics):

- templates apply
  - Flags:
    - `--workspace-folder, -w <path>`: Target workspace folder to apply Template. Required (default `.`).
    - `--template-id, -t <oci-ref>`: Template reference, e.g. `ghcr.io/owner/templates/name:tag` or `@sha256:<digest>`. Required.
    - `--template-args, -a <json>`: JSON object mapping option names to string values. Default `{}`.
    - `--features, -f <json>`: JSON array of `{ id: string, options: Record<string, string|boolean|undefined> }`. Default `[]`.
    - `--omit-paths <json>`: JSON array of paths within the template to omit (globlike suffix `/*` supported). Default `[]`.
    - `--tmp-dir <path>`: Temp directory for downloads/extraction.
    - `--log-level <info|debug|trace>`: Logging verbosity. Default `info`.
  - Output: JSON to stdout with `{ files: string[] }` (relative paths written to the workspace).

- templates publish
  - Flags:
    - `--registry, -r <host>`: OCI registry host (default `ghcr.io`).
    - `--namespace, -n <owner/repo>`: Collection namespace (required).
    - `--log-level <info|debug|trace>`.
  - Positionals:
    - `target` (default `.`): Either (1) a collection `src/` folder containing multiple templates or (2) a single template folder containing `devcontainer-template.json`.
  - Output: JSON mapping of template ids to `{ publishedTags?: string[], digest?: string, version?: string }`.

- templates metadata
  - Positionals:
    - `templateId`: OCI ref for a published template (tag or digest form).
  - Flags:
    - `--log-level <info|debug|trace>`.
  - Output: JSON object with the published template metadata (from manifest annotation) or `{}` on failure/missing.

- templates generate-docs
  - Flags:
    - `--project-folder, -p <path>`: Project root containing `src/` (and optionally `test/`). Default `.`.
    - `--github-owner <owner>` and `--github-repo <repo>`: Used for link scaffolding.
    - `--log-level <info|debug|trace>`.
  - Output: writes docs to disk; no structured stdout except logs.

Flag Taxonomy:
- Required: `apply --workspace-folder`, `apply --template-id`, `publish --namespace` (and `target` implied), `metadata <templateId>`.
- Optional: all others.
- Mutually exclusive groups: none.
- Deprecated: none in scope.

Argument Validation Rules:
- `--template-args` must be valid JSON object of `string -> string` pairs; reject arrays, numbers, booleans at top level or non‑string values.
- `--features` must be valid JSON array; each entry requires `id: string`; `options` is optional object.
- `--omit-paths` must be a valid JSON array of strings.
- `templateId`/`--template-id` must parse to an OCI ref: `registry/namespace/name[:tag|@sha256:digest]`; `name` matches `[a-z0-9]+([._-][a-z0-9]+)*` path segments.

## 3. Input Processing Pipeline

Pseudocode — common CLI argument parsing and validation:

```
FUNCTION parse_command_arguments(argv) -> ParsedInput:
  SWITCH subcommand OF argv:
    CASE 'apply':
      SET workspace = argv['--workspace-folder'] OR '.'
      REQUIRE argv['--template-id']
      PARSE json_object FROM argv['--template-args'] WITH jsonc; COLLECT parse_errors
      IF parse_errors OR NOT is_object(json_object) OR NOT all_values_strings(json_object) THEN error_exit('Invalid template arguments provided')

      PARSE features FROM argv['--features'] WITH jsonc; COLLECT parse_errors
      IF parse_errors OR NOT is_array(features) OR NOT all_have_id(features) THEN error_exit('Invalid template arguments provided')

      PARSE omit_paths FROM argv['--omit-paths'] WITH jsonc; IF present AND NOT is_array(omit_paths) THEN error_exit('Invalid --omit-paths argument provided')

      RETURN { kind: Apply, workspace, template_id, options: json_object, features, omit_paths, tmp_dir, log_level }

    CASE 'publish':
      REQUIRE argv['--namespace']
      RETURN { kind: Publish, target, registry, namespace, log_level }

    CASE 'metadata':
      REQUIRE positional templateId
      RETURN { kind: Metadata, template_id, log_level }

    CASE 'generate-docs':
      RETURN { kind: GenerateDocs, project_folder, github_owner, github_repo, log_level }

    DEFAULT: error_exit('Unknown subcommand')
END FUNCTION
```

Error handling: on validation failure, emit an error to stderr at `Error` level and exit non‑zero.

## 4. Configuration Resolution

Configuration sources (by precedence):
- CLI flags (see Section 2).
- Environment variables impacting registry auth & behavior:
  - `DEVCONTAINERS_OCI_AUTH`: comma‑separated `registry|user|token` triples.
  - `DOCKER_CONFIG`: alternate location of Docker `config.json` for credential helpers.
  - `GITHUB_TOKEN` (and optional `GITHUB_HOST`): GHCR auth.
- Files within templates (consumed/modified by apply):
  - `devcontainer-template.json`: template metadata and options schema.
  - `devcontainer.json`: target configuration to which Features may be added.
- Defaults: for logging, workspace folder, etc.

Merge Algorithm:
- When applying Features into an existing `devcontainer.json`:
  - Parse JSONC content.
  - If `features` property missing, create as empty object.
  - For each requested Feature `{ id, options }`, set `features[id] = options || {}` (overwriting any existing value for that id).

Variable Substitution:
- Template files may contain `${templateOption:<key>}` tokens.
- For each file in the applied set, replace all occurrences with the selected option value (string) or empty string if unset.
- Before substitution, auto‑fill missing user options from defaults in `devcontainer-template.json` (`string` defaults copied as‑is; `boolean` defaults converted to `"true"`/`"false"`).

## 5. Core Execution Logic

Pseudocode — apply:

```
FUNCTION execute_apply(input) -> Result:
  INIT logging with input.log_level
  // Resolve & fetch
  REF := parse_oci_ref(input.template_id) OR error_exit('Failed to parse template ref')
  MANIFEST := fetch_manifest(REF) OR error_exit('Failed to fetch template manifest')
  LAYER_DIGEST := first_layer_digest(MANIFEST) OR error_exit('Manifest missing layer')
  FILES, METADATA := get_blob_and_extract(
    ref=REF, digest=LAYER_DIGEST, dest=input.workspace, tmp_dir=input.tmp_dir,
    omit = input.omit_paths ∪ { 'devcontainer-template.json', 'README.md', 'NOTES.md' },
    metadata_filename='devcontainer-template.json')
    OR error_exit('Failed to download package')

  // Options defaulting and substitution
  OPTIONS := merge_defaults(METADATA.options, input.options)
  FOR EACH file IN FILES:
    REPLACE '${templateOption:KEY}' with OPTIONS[KEY] in file content

  // Optional features
  IF input.features NOT EMPTY:
    CONFIG := find_file(FILES endswith 'devcontainer.json')
    IF CONFIG EXISTS: add_features(CONFIG, input.features)
    ELSE log_error('Could not find devcontainer.json to apply Features onto')

  EMIT stdout JSON { files: FILES }
  RETURN success
END FUNCTION
```

Pseudocode — publish:

```
FUNCTION execute_publish(input) -> Result:
  INIT logging
  META := package_templates(target=input.target)  // writes tgz per template and collection metadata JSON
  IF META undefined THEN exit(1)
  RESULT := {}
  FOR EACH template IN META.templates:
    IF template.version missing: warn('no version'); CONTINUE
    REF := parse_collection_item_ref(registry=input.registry, namespace=input.namespace, id=template.id)
    SEMANTIC_TAGS := compute_semver_tags(template.version, get_published_tags(REF)) OR []
    DIGEST := push_layer_and_manifest(REF, archive=get_archive_path(template), tags=SEMANTIC_TAGS,
                                      annotations={ 'dev.containers.metadata': json(template) })
    IF DIGEST: RESULT[template.id] = { publishedTags: SEMANTIC_TAGS, digest: DIGEST, version: template.version }
  // Publish collection metadata (latest)
  COLLECTION_REF := parse_collection_ref(input.registry, input.namespace)
  push_collection_metadata(COLLECTION_REF, path_to_collection_json)
  PRINT stdout json(RESULT)
  RETURN success
END FUNCTION
```

Pseudocode — metadata:

```
FUNCTION execute_metadata(input) -> Result:
  INIT logging
  REF := parse_oci_ref(input.template_id) OR exit(1)
  MANIFEST := fetch_manifest_if_exists(REF)
  IF not MANIFEST: PRINT {}; exit(1)
  META_STR := MANIFEST.annotations['dev.containers.metadata']
  IF not META_STR: warn('no metadata'); PRINT {}; exit(1)
  PRINT stdout parse_json(META_STR)
  RETURN success
END FUNCTION
```

Pseudocode — generate-docs:

```
FUNCTION execute_generate_docs(input) -> Result:
  INIT logging
  generate_templates_documentation(input.project_folder, input.github_owner, input.github_repo)
  RETURN success
END FUNCTION
```

## 6. State Management
- Persistent State: None required. Operations read/write regular files in the workspace; publish uses temporary output folder for packaging artifacts before upload.
- Cache Management: Network layer may cache authorization headers per registry during a process run. No persistent disk cache mandated by design.
- Lock Files: None for templates. (Lockfiles are used by other subcommands like config lock, out of scope here.)
- Idempotency:
  - apply: re‑running will re‑materialize the same files and substitutions; feature entries are overwritten for the same id.
  - publish: safe to re‑run; existing tags are skipped; non‑published semantic tags only are pushed.
  - metadata: read‑only.

## 7. External System Interactions

Docker/Container Runtime: Not used by `templates` subcommands.

OCI Registries:
- Authentication flow:
  - Attempt request; on 401/403 use `WWW-Authenticate` to choose Basic or Bearer.
  - Credentials sources: `DEVCONTAINERS_OCI_AUTH`, Docker credential helpers via `~/.docker/config.json` and `DOCKER_CONFIG`, `GITHUB_TOKEN` for `ghcr.io`.
  - Cache `Authorization` header per registry for reuse in the process.
- Manifest fetching algorithm:
  - Build URL `https://<registry>/v2/<namespace>/<name>/manifests/<tag|digest>`.
  - Accept header: manifest or index media type; if index, select platform entry; then fetch actual manifest.
  - Compute canonical content digest from header `docker-content-digest` or recalculate SHA‑256 over body.
- Layer download & extraction:
  - Build blob URL `https://<registry>/v2/<namespace>/<name>/blobs/<digest>`.
  - Download to temp dir; extract into destination, omitting configured paths and reserved files (`devcontainer-template.json`, `README.md`, `NOTES.md`).
  - Return list of relative file paths and parsed `devcontainer-template.json`.
- Platform selection: When fetching via index, choose entry matching current OS/arch (mapping Node to GOARCH conventions) — not commonly used by templates but supported by the shared code.

File System:
- Reads: template metadata files (`devcontainer-template.json`, `devcontainer.json`).
- Writes: extracted template files into workspace; modified `devcontainer.json` when adding Features; generated docs.
- Permissions: best‑effort preservation from tar; normal user write permissions required in target folder.
- Paths: normalized joins; omit rules allow directory‑wide exclusions via `/*` suffix.
- Symlinks: extraction follows tar library semantics; path traversal protections should be enforced by extraction implementation (no `..` escapes allowed).

## 8. Data Flow Diagrams
See `DIAGRAMS.md` for Mermaid sequence diagrams and ASCII flows.

## 9. Error Handling Strategy

User Errors:
- Invalid JSON in `--template-args`, `--features`, or `--omit-paths` -> exit code 1; message: `Invalid template arguments provided` or specific flag message.
- Malformed `templateId`/`--template-id` -> exit 1; message: failed to parse ref.
- Missing `devcontainer.json` when adding features -> error log; apply continues.
- Destination not writable -> exit with message from filesystem error.

System Errors:
- Docker/registry unavailable, DNS/network issues -> error log; exit non‑zero.
- Authentication failures -> error log; exit non‑zero after auth attempts.
- 404 for manifest/blob -> treated as not found; for metadata prints `{}` and exits non‑zero; for apply/publish exits non‑zero.

Configuration Errors:
- Invalid/missing `devcontainer-template.json` in published blob -> apply exits non‑zero.
- Invalid semantic version in `version` during publish -> exit non‑zero.

Exit Codes:
- 0 success; 1 for validation/operation failure. No additional codes reserved by design.

## 10. Output Specifications

Standard Output (stdout):
- apply: `{ files: string[] }` JSON only.
- publish: JSON map `{ [templateId: string]: { publishedTags?: string[], digest?: string, version?: string } }`.
- metadata: JSON object with template metadata or `{}` on missing/unavailable.
- generate-docs: no structured output.

Standard Error (stderr):
- Logs respect `--log-level` and include trace/debug/info messages and diagnostics.

Quiet Mode:
- Not defined; control verbosity via `--log-level`.

## 11. Performance Considerations
- Caching Strategy: Authorization header cache within process; network responses not persisted.
- Parallelization: Publish iterates templates sequentially; could be parallelized per template push, but constrained by rate limits and meaningful log ordering.
- Resource Limits: Memory bounded by tar extraction and JSON parsing; disk usage bounded by template size in temp and target directories.
- Optimization Opportunities: Parallel uploads for independent templates; conditional download if digest already present in local cache (future extension); stream extraction.

## 12. Security Considerations
- Secrets Handling: Credentials pulled from env or Docker config; never log secrets; redact tokens in error messages.
- Privilege Escalation: None (filesystem writes only). Registry access limited by tokens provided.
- Input Sanitization: Validate OCI ref and JSON inputs; reject unknown types; rely on tar extraction with path traversal safeguards (no writing outside target).
- Container Isolation: Not applicable for this command group.

## 13. Cross-Platform Behavior

| Aspect           | Linux | macOS | Windows | WSL2 |
|------------------|-------|-------|---------|------|
| Path handling    | POSIX | POSIX | use Node path joins | POSIX within WSL |
| Temp directory   | `/tmp`| `/tmp`| `%TEMP%`| `/tmp` |
| Line endings     | LF    | LF    | preserve source | preserve source |
| Credential store | Docker config & helpers | same | same | same |

## 14. Edge Cases and Corner Cases
- Templates with no `version` during publish are skipped with a warning.
- Template manifests without `dev.containers.metadata` annotation return `{}` in `metadata` command.
- Template without `devcontainer.json` will not receive Feature injection; apply still succeeds otherwise.
- Omit paths exclude directories when suffixed with `/*`; exact file names omit single files.
- Large templates and slow networks: extraction and downloads may be long‑running; logs should reflect progress.

## 15. Testing Strategy

```
TEST SUITE for templates:

  TEST "apply happy path":
    GIVEN valid template id and args
    WHEN apply executes
    THEN files are written and stdout lists them

  TEST "apply invalid args":
    GIVEN malformed JSON in --template-args
    WHEN apply executes
    THEN exit 1 with validation error

  TEST "apply features injection":
    GIVEN features array targeting a template containing devcontainer.json
    WHEN apply executes
    THEN devcontainer.json contains features entries

  TEST "metadata with annotation":
    GIVEN published template with dev.containers.metadata
    WHEN metadata executes
    THEN stdout is parsed template JSON

  TEST "metadata missing annotation":
    GIVEN template manifest without metadata
    WHEN metadata executes
    THEN stdout is {} and exit non-zero

  TEST "publish semantic tags":
    GIVEN unpublished version
    WHEN publish executes
    THEN digest and tags include major, minor, version, latest

  TEST "publish existing version":
    GIVEN version already published
    WHEN publish executes
    THEN operation skips and returns empty entry for that template
END TEST SUITE
```

## 16. Migration Notes
- Deprecated Behavior: none.
- Breaking Changes: none relative to the reference.
- Compatibility Shims: N/A.

## Design Decisions and Rationale (Critical Analysis)

#### Design Decision: Use manifest annotation for metadata (`dev.containers.metadata`)
Implementation Behavior: Publish attaches fully serialized template metadata as an OCI manifest annotation. `metadata` command reads it without downloading layers.
Specification Guidance: OCI manifest supports annotations; Dev Containers tooling uses annotations for metadata discovery.
Rationale: Enables lightweight metadata fetch (no blob download), consistent with registry distribution best practices.
Alternatives Considered: Store metadata in a separate tag or a dedicated artifact; would complicate discovery and version coupling.
Trade-offs: Annotations length limits in some registries; metadata must be kept small.

#### Design Decision: Compute semantic tags (major, minor, full, latest)
Implementation Behavior: For a new semantic version, publish tags `[major, major.minor, major.minor.patch, latest]` iff they are not superseded by existing published tags.
Specification Guidance: Not mandated by OCI; common distribution practice for discoverability.
Rationale: Improves consumer ergonomics; mirrors npm‑style tagging.
Alternatives Considered: Only publish the explicit version; fewer tags reduce tag churn but harm UX.
Trade-offs: Tag proliferation; requires fetching tag list and extra pushes.

#### Design Decision: Option substitution via `${templateOption:KEY}`
Implementation Behavior: Replace tokens in all extracted files using provided or defaulted string values.
Specification Guidance: Templates spec defines parameterization via options; token redaction pattern is an implementation contract.
Rationale: Simple, explicit in‑file substitution with minimal tooling.
Alternatives Considered: Templating engines; more powerful but heavier and less portable.
Trade-offs: Limited expressiveness; no conditionals/loops.

#### Design Decision: Feature injection into devcontainer.json
Implementation Behavior: If requested, add `features[feature.id] = options` into devcontainer.json, creating `features` object if missing.
Specification Guidance: Features are declared under `features` in devcontainer.json; JSONC editing preserves comments and structure.
Rationale: Integrates templates with features ecosystem seamlessly.
Alternatives Considered: Out‑of‑band instructions to add features; worse UX and error‑prone.
Trade-offs: Overwrites existing feature with same id; explicit.

#### Design Decision: First layer holds template tarball
Implementation Behavior: Apply expects the first manifest layer digest to reference the tarball payload.
Specification Guidance: OCI manifest supports multiple layers; this convention mirrors features/templates packaging approach in the reference.
Rationale: Simplicity; shared packaging/publish code assumes first layer is the content.
Alternatives Considered: Distinct media types per layer; would require extra negotiation.
Trade-offs: Requires consistent packaging.

#### Deviations and Notes
- The reference CLI does not expose a dedicated `templates pull` subcommand; fetching occurs within `templates apply`. If an implementation adds `pull`, document it as an extension.

