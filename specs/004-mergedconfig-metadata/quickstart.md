# Quickstart: Enriched mergedConfiguration metadata for up

1. Read the spec docs  
   - specs/001-mergedconfig-metadata/spec.md  
   - docs/repomix-output-devcontainers-cli.xml (mergedConfiguration, metadata, labels)  
   - docs/subcommand-specs/up/SPEC.md and DATA-STRUCTURES.md (includeMergedConfig behavior)  
   - crates/deacon/src/commands/read_configuration.rs (current merge logic to reuse)

2. Design alignment  
   - Reuse read_configuration merge path for both single and compose `up` flows.  
   - Ensure feature metadata entries include provenance, ordering, and null/empty handling.  
   - Merge image/container labels with provenance; keep fields present when missing.

3. Implement  
   - Wire mergedConfiguration generation in `up` to shared merge helpers.  
   - Preserve ordering using existing ordered collections; avoid sorting.  
   - Surface null/empty fields per schema instead of omission.  
   - Keep stdout JSON clean; logs on stderr.

4. Tests  
   - Unit/logic: cover metadata/label merging and ordering/null semantics (`make test-nextest-unit`).  
   - Fast suite: `cargo fmt --all && cargo fmt --all -- --check`; `cargo clippy --all-targets -- -D warnings`; `make test-nextest-fast`.  
   - Add/adjust fixtures for single and compose to assert mergedConfiguration differs from base and includes feature metadata keys and labels when available.

5. Validation  
   - Compare base vs merged outputs to confirm enrichment only.  
   - Run schema/JSON contract checks if present; ensure no output ordering drift.  
   - Update examples/fixtures if outputs change.
