# Quickstart: Up Build Parity and Metadata

1. Read `spec.md`, `research.md`, `data-model.md`, and `contracts/up.md` to align on build-option propagation, skip-feature-auto-mapping, lockfile/frozen enforcement, and metadata merging.
2. In `crates/core`, locate shared config/feature/build helpers and extend them to carry BuildKit/cache-from/cache-to/buildx options to both Dockerfile and feature build paths; ensure fail-fast when builder/BuildKit unavailable.
3. In `crates/deacon`, ensure `up` argument parsing threads skip-feature-auto-mapping, lockfile, and frozen flags; validate lockfile before builds and halt on mismatch/missing when frozen.
4. Extend merged configuration construction to include feature metadata for every resolved feature (empty object when none) and record applied build options/enforcement markers.
5. Maintain stdout/stderr contracts: JSON mode emits only the mergedConfiguration on stdout; logs go to stderr; preserve ordering from user config/lockfile.
6. Update or add tests with nextest grouping: unit parsing/merging → `make test-nextest-unit`; Docker/build flows → `make test-nextest-docker`; final quick pass → `make test-nextest-fast`. Run fmt and clippy before tests.
7. Update fixtures/examples only if user-visible outputs or flags change; keep exec.sh scripts in sync if touched.

Commands checklist:
- `cargo fmt --all && cargo fmt --all -- --check`
- `cargo clippy --all-targets -- -D warnings`
- `make test-nextest-unit`
- `make test-nextest-docker` (for build/runtime changes)
- `make test-nextest-fast`
