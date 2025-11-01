---

description: "Tasks to implement Spec 003 — Features Publish Spec Compliance"
---

# Tasks: Features Publish Spec Compliance

Input: Design documents from `specs/003-features-publish-compliance/`
Prerequisites: plan.md (required), spec.md (required), research.md, data-model.md, contracts/

Organization: Tasks are grouped by user story so each story can be implemented and validated independently.

## Format: `[ID] [P?] [Story] Description`

- [P]: Can run in parallel (different files, no dependencies)
- [Story]: Which user story this task belongs to (e.g., US1, US2, US3)
- Include exact file paths in descriptions

---

## Phase 1: Setup (Shared Infrastructure)

Purpose: Prepare structures and utilities used across stories (no user-facing behavior yet).

- [ ] T001 [P] Create publish output models matching schema in `crates/deacon/src/commands/features_publish_output.rs`
- [ ] T002 [P] Update semantic tag utility to exclude `latest` for pre-releases in `crates/core/src/semver_utils.rs`
- [ ] T003 [P] Add helper to compute desired vs existing tags using list endpoint in `crates/deacon/src/commands/features.rs`
- [ ] T004 Ensure contract file is referenced in code comments for JSON output (`specs/003-features-publish-compliance/contracts/publish-output.schema.json`)

---

## Phase 2: Foundational (Blocking Prerequisites)

Purpose: Core wiring and APIs that user stories depend on (must be done before any story).

- [ ] T005 Add `--namespace` flag and default `--registry ghcr.io` in CLI parse (required) in `crates/deacon/src/cli.rs`
- [ ] T006 [P] Thread new flags through dispatcher to features command in `crates/deacon/src/cli.rs`
- [ ] T007 [P] Refactor `execute_features_publish` signature to accept `namespace` and default registry in `crates/deacon/src/commands/features.rs`
- [ ] T008 [P] Implement tag listing via `FeatureFetcher::list_tags` and diff computation in `crates/deacon/src/commands/features.rs`
- [ ] T009 Implement JSON-only-to-stdout and logs-to-stderr discipline for publish path in `crates/deacon/src/commands/features.rs`
- [ ] T010 [P] Introduce collection publish helper in core (`publish_collection_metadata`) in `crates/core/src/oci.rs` (use OCI artifact media type)
- [ ] T011 Ensure auth sources (env + Docker config) are honored; document redaction notes in code in `crates/core/src/oci.rs`

Checkpoint: Foundation ready — user story implementation can now begin.

---

## Phase 3: User Story 1 — First publish of a stable version (Priority: P1) 🎯 MVP

Goal: Publish a packaged feature to `<registry>/<namespace>/<name>` with semantic tags `X`, `X.Y`, `X.Y.Z`, and `latest` (stable only) and emit spec-compliant JSON.

Independent Test: Running `deacon features publish ./path --namespace owner/repo --registry ghcr.io --output json` yields JSON matching `contracts/publish-output.schema.json` and shows published tags when none exist.

### Implementation

- [ ] T012 [P] [US1] Validate feature version is SemVer; compute desired tags in `crates/deacon/src/commands/features.rs`
- [ ] T013 [P] [US1] Package feature if artifacts are missing in `crates/deacon/src/commands/features.rs` (reuse `create_feature_package`)
- [ ] T014 [US1] Retrieve existing tags via `list_tags`, compute `to_publish` (stable adds `latest`) in `crates/deacon/src/commands/features.rs`
- [ ] T015 [US1] Publish missing tags using `publish_feature_multi_tag` in `crates/core/src/oci.rs` (call site in `features.rs`)
- [ ] T016 [US1] Build JSON output object (features[], collection?, summary) using models in `crates/deacon/src/commands/features_publish_output.rs`
- [ ] T017 [US1] Print single JSON document to stdout; log human text to stderr in `crates/deacon/src/commands/features.rs`

Checkpoint: US1 fully functional and independently verifiable with JSON output.

---

## Phase 4: User Story 2 — Idempotent re-publish (Priority: P1)

Goal: Re-running publish when tags already exist should perform no uploads and complete quickly, reporting skipped tags in JSON.

Independent Test: When all desired tags exist, command exits 0, performs no uploads, and JSON shows skipped counts.

### Implementation

- [ ] T018 [P] [US2] Use `list_tags` result to detect existing; short-circuit when `to_publish` is empty in `crates/deacon/src/commands/features.rs`
- [ ] T019 [US2] Populate `skippedTags` and `summary.skippedTags` correctly in `crates/deacon/src/commands/features.rs`
- [ ] T020 [US2] Ensure end-to-end path completes under ~10s locally by avoiding unnecessary network calls in `crates/deacon/src/commands/features.rs`

Checkpoint: US2 independently verifiable — no-op publish returns success with accurate JSON.

---

## Phase 5: User Story 3 — Invalid version input (Priority: P1)

Goal: Reject non-SemVer versions with a clear validation error and exit code 1.

Independent Test: Feature with invalid version triggers validation error before network operations.

### Implementation

- [ ] T021 [P] [US3] Add version validation and error path before packaging in `crates/deacon/src/commands/features.rs`
- [ ] T022 [US3] Ensure non-zero exit and helpful message (stderr) with no JSON body in `crates/deacon/src/commands/features.rs`

Checkpoint: US3 independently verifiable — invalid input fails fast.

---

## Phase 6: User Story 4 — Authentication via env/config (Priority: P2)

Goal: Successfully publish to private registry/namespace using Docker config helpers or explicit env credentials; never log secrets.

Independent Test: With credentials present (env or Docker config), publish succeeds; with missing creds, fails with actionable error.

### Implementation

- [ ] T023 [P] [US4] Ensure `ReqwestClient` auth precedence (env > docker config) is used by publish path in `crates/core/src/oci.rs`
- [ ] T024 [US4] Add minimal CLI docs for env vars in help text (no new flags) in `crates/deacon/src/cli.rs`
- [ ] T025 [US4] Verify secret redaction registry is used for any captured values in `crates/core/src/logging.rs` and `crates/core/src/oci.rs`

Checkpoint: US4 independently verifiable — private publish works without secret leakage.

---

## Phase 7: User Story 5 — Collection metadata publish (Priority: P2)

Goal: Publish `devcontainer-collection.json` to `<registry>/<namespace>:collection` using media type `application/vnd.devcontainer.collection+json` and include digest in JSON.

Independent Test: If collection metadata exists, command publishes it and JSON includes `collection.digest`.

### Implementation

- [ ] T026 [P] [US5] Detect `devcontainer-collection.json` in packaged output in `crates/deacon/src/commands/features.rs`
- [ ] T027 [P] [US5] Implement `publish_collection_metadata` using blob+manifest flow in `crates/core/src/oci.rs`
- [ ] T028 [US5] Wire call from publish command and set `collection.digest` in output in `crates/deacon/src/commands/features.rs`

Checkpoint: US5 independently verifiable — collection metadata is published and discoverable.

---

## Phase N: Polish & Cross-Cutting Concerns

Purpose: Align docs, examples, and small hardening improvements.

- [ ] T029 [P] Update CLI help and Quickstart with new flags and examples in `specs/003-features-publish-compliance/quickstart.md`
- [ ] T030 [P] Add example JSON snippet aligned to schema in `examples/registry/dry-run-publish/README.md`
- [ ] T031 Harden error messages and add tracing spans (`features.publish`) in `crates/deacon/src/commands/features.rs`
- [ ] T032 Ensure output schema stays stable and reference it in rustdoc in `crates/deacon/src/commands/features_publish_output.rs`

---

## Dependencies & Execution Order

Phase dependencies

- Setup (Phase 1): No dependencies
- Foundational (Phase 2): Depends on Setup — blocks all user stories
- User Stories (Phase 3+): Depend on Foundational; can proceed in priority order (US1 → US2 → US3 → US4 → US5) or in parallel where specified
- Polish (Final): After desired user stories complete

User story dependencies

- US1 (P1): After Foundational; no dependency on other stories
- US2 (P1): After US1 (shares code paths); can be implemented with US1 diff logic
- US3 (P1): After Foundational; independent of US1/US2 (validation only)
- US4 (P2): After Foundational; independent of US1/US2/US3 (auth wiring is core)
- US5 (P2): After US1 (publishing flow) and Foundational (OCI helper)

---

## Parallel execution examples

US1 parallelizable tasks

- T012 and T013 can proceed in parallel (validation/utilities vs packaging helper)
- After T014 computes `to_publish`, T015 (publish) and T016 (output wiring) are sequential

US5 parallelizable tasks

- T026 (detection) and T027 (core helper) can proceed in parallel; T028 depends on both

---

## Implementation strategy

MVP first (deliver US1):

1) Phase 1 + Phase 2
2) Implement US1 tasks (T012–T017)
3) Validate JSON contract and behavior end-to-end

Then incrementally add US2 → US3 → US4 → US5, keeping each story independently testable and shippable.

---

## Appendix — Story-to-Task Mapping

- US1: T012–T017 (6 tasks)
- US2: T018–T020 (3 tasks)
- US3: T021–T022 (2 tasks)
- US4: T023–T025 (3 tasks)
- US5: T026–T028 (3 tasks)
- Setup/Foundational/Polish: T001–T011, T029–T032 (15 tasks)

---

## Format validation

All tasks follow the required checklist format: `- [ ] T### [P?] [US#?] Description with file path` and include concrete file paths.
