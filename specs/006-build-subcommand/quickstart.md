# Quickstart Guide: Build Subcommand Parity Closure

1. **Sync with Spec & Research**  
   Review `specs/006-build-subcommand/spec.md` and `research.md` to ensure alignment with the build subcommand requirements and recorded decisions.

2. **Implement CLI Flag Parity**  
   - Extend `crates/deacon/src/cli.rs` to include `--image-name`, `--push`, `--output`, and `--label` with repeatable semantics.  
   - Update `BuildArgs` in `crates/deacon/src/commands/build.rs` to capture new fields and propagate them into execution logic.

3. **Add Validation & Output Contracts**  
   - Enforce mutually exclusive flag rules, BuildKit gating, and config filename checks.  
   - Replace the existing JSON result with the spec-compliant `{ "outcome": ... }` payloads defined in `contracts/build-cli-contract.yaml`.

4. **Implement Build Behavior Across Modes**  
   - Dockerfile mode: inject metadata labels, apply features, and tag images per `--image-name`.  
   - Image-reference mode: extend the base image with features and metadata rather than rejecting configurations.  
   - Compose mode: target only the referenced service, generate overrides, and block unsupported flags.

5. **Support Feature Installation & Metadata**  
   - Generate feature build contexts/scripts, honor skip flags, and update metadata labels with customizations and lockfile information.  
   - Wire BuildKit build contexts and security options derived from features.

6. **Push & Export Workflows**  
   - When `--push` is set and BuildKit available, pass `--push` to buildx and report pushed tags.  
   - When `--output` is specified, honor the export spec and populate `exportPath` in the success payload.

7. **Testing & Validation**  
   - Update or add unit tests for CLI parsing and validation rules.  
   - Expand integration/smoke tests under `crates/deacon/tests/` to cover Dockerfile, image, and Compose scenarios, including error cases.  
   - Run fast loop (`make dev-fast`) after each iteration and the full gate before submitting a PR.

8. **Documentation & Examples**  
   - Update affected examples under `examples/build/` to demonstrate new flags and behaviors.  
   - Ensure docs in `docs/subcommand-specs/build/` reflect any clarified workflows if discrepancies are resolved.
