# Nextest Quickstart

1. **Install cargo-nextest**
   - Use `cargo install cargo-nextest --locked` or follow <https://nexte.st/book/getting-started.html>.
   - Verify with `cargo nextest --version`.

2. **Run the fast feedback loop**
   - Execute `make test-nextest-fast`.
   - Expected outcome: high-signal unit/integration tests run in parallel; smoke/parity suites are skipped.
   - Artifacts: `artifacts/nextest/dev-fast-timing.json` (created on demand).

3. **Run the full suite locally**
   - Execute `make test-nextest`.
   - This uses the `full` profile, including smoke/parity groups where required while respecting their serial constraints.

4. **Execute the CI-aligned conservative profile**
   - Optional but recommended before pushing: `make test-nextest-ci`.
   - Mirrors the GitHub Actions job, emitting timing data and JUnit reports under `artifacts/nextest/`.

5. **Classify or audit tests**
   - View test group configuration: `cargo nextest show-config test-groups`.
   - List all tests with details: `cargo nextest list --verbose`.
   - Or use the convenience target: `make test-nextest-audit`.
   - To move a test, update `.config/nextest.toml` group selectors and document the rationale in `docs/testing/nextest.md`.

6. **Handle missing dependencies**
   - If `make` exits with a message about missing cargo-nextest, install it and rerun.
   - Docker-dependent groups still honor the existing `DOCKER=0` overrides; ensure the daemon is running for smoke/parity tests.

7. **Compare runtimes against the baseline**
   - Serial baseline: `make test` (unchanged).
   - Parallel comparison: inspect the JSON artifacts recorded by the nextest profiles to confirm ≥40% speedup locally and 50–70% in CI.
