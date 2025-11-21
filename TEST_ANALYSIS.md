# Test Binary Analysis

- binary: `deacon` (profile: `dev-fast`)
  - status: pass (391 tests)
  - runtime: ~25s wall (includes ~24s rebuild; test execution ~0.7s)
  - scope: crate-level unit tests for CLI flag parsing, build/up/down argument assembly, config loader behavior, feature packaging, template packaging, etc.
  - nextest group: default (no explicit override)
  - notes: rerun directly after a prior make failure; passes in isolation under dev-fast.

- binary: `canonical_id` (path: `crates/core/tests/canonical_id.rs`, profile: `dev-fast`)
  - status: pass (2 tests)
  - runtime: ~0s wall
  - scope: SHA256-based canonical ID determinism for OCI manifest bytes.
  - nextest group: default (no explicit override)

- binary: `features_info_models` (path: `crates/core/tests/features_info_models.rs`, profile: `dev-fast`)
  - status: pass (5 tests)
  - runtime: ~1s wall
  - scope: JSON serialization/deserialization round-trips for manifest, published tags, and verbose feature info payloads with/without optional fields.
  - nextest group: default (no explicit override)

- binary: `features_test_discovery` (path: `crates/core/tests/features_test_discovery.rs`, profile: `dev-fast`)
  - status: pass (1 test)
  - runtime: ~0s wall
  - scope: Discovers feature test scenarios from workspace fixtures ensuring collection structure is parsed.
  - nextest group: default (no explicit override)

- binary: `features_test_paths` (path: `crates/core/tests/features_test_paths.rs`, profile: `dev-fast`)
  - status: pass (13 tests)
  - runtime: ~1s wall
  - scope: Validates path resolution and validation for feature test resources and temp directories.
  - nextest group: default (no explicit override)

- binary: `features_test_scenarios` (path: `crates/core/tests/features_test_scenarios.rs`, profile: `dev-fast`)
  - status: pass (15 tests)
  - runtime: ~0s wall
  - scope: Parses and validates feature test scenario definitions and ensures expected error handling for malformed inputs.
  - nextest group: default (no explicit override)

- binary: `integration_compose` (path: `crates/core/tests/integration_compose.rs`, profile: `dev-fast`)
  - status: pass (9 tests)
  - runtime: ~0s wall
  - scope: Compose project derivation, compose file merging, and sanitized project naming against fixture compose files.
  - nextest group: default

- binaries: `integration_config`, `integration_layered_merge`, `integration_lockfile`, `integration_templates` (core integration, profile: `dev-fast`)
  - status: pass (`integration_config`:3, `layered_merge`:4, `lockfile`:10, `templates`:5 tests)
  - runtime: all sub-1s
  - scope: Config resolution/merge across layered devcontainer files, lockfile read/write paths, and template resolution behaviors.
  - nextest group: fs-heavy (per nextest overrides)

- binary: `integration_non_blocking_lifecycle` (path: `crates/core/tests/integration_non_blocking_lifecycle.rs`, profile: `dev-fast`)
  - status: pass (6 tests)
  - runtime: ~10s wall
  - scope: Ensures non-blocking lifecycle steps emit progress and return promptly across start/stop transitions.
  - nextest group: default

- binary: `oci_timeout` (path: `crates/core/tests/oci_timeout.rs`, profile: `dev-fast`)
  - status: pass (4 tests)
  - runtime: ~12s wall
  - scope: Validates configurable OCI registry timeouts for pull/push and error surfacing.
  - nextest group: default

- binaries: `integration_env_probe_cache`, `integration_env_probe_env_capture`, `integration_env_probe_remote`, `integration_env_probe_user` (core env-probe suite, profile: `full`)
  - status: timed out (>90s per binary; each hit 100s timeout with 1–2 tests stuck)
  - runtime: ~100s per binary before cancel
  - scope: Probing remote/local environment metadata capture and caching semantics; likely waiting on external/dummy probe endpoints.
  - nextest group: env-probe (per overrides); requires investigation/possible fixture or timeout tuning.

- binary: `integration_build` (path: `crates/deacon/tests/integration_build.rs`, profile: `full`)
  - status: pass (19 tests)
  - runtime: ~21s wall (includes build)
  - scope: End-to-end build subcommand coverage including cache/export handling and error mapping.
  - nextest group: docker-exclusive (per full profile overrides)

- binaries: `integration_up_initialize_command`, `integration_up_traditional` (profile: `full`)
  - status: pass (6 tests, 3 tests respectively)
  - runtime: ~3s / ~1s wall
  - scope: `up` workflow initialization and legacy flow behavior around compose/env setup.
  - nextest group: docker-exclusive (per full overrides)

- binary: `up_dotfiles` (path: `crates/deacon/tests/up_dotfiles.rs`, profile: `dev-fast`)
  - status: pass (8 tests)
  - runtime: ~31s wall
  - scope: Validates dotfiles clone/apply and idempotence across up cycles.
  - nextest group: docker-exclusive (per overrides)

- binaries: `integration_features_test_json`, `integration_features_publish`, `integration_host_requirements`, `integration_port_forwarding` (profile: `dev-fast`)
  - status: pass (4/5/5/5 tests)
  - runtime: ~0–1s each
  - scope: Devcontainer features test JSON parsing, publish behavior, host requirement gating, and port-forward flag wiring.
  - nextest group: docker-shared (per overrides)

- binary: `integration_progress` (path: `crates/deacon/tests/integration_progress.rs`, profile: `full`)
  - status: pass (4 tests)
  - runtime: ~0s wall
  - scope: Progress reporter output and throttling during long-running operations.
  - nextest group: long-running (per overrides)

- binary: `parse_docker_tooling_flags` (path: `crates/deacon/tests/parse_docker_tooling_flags.rs`, profile: `dev-fast`)
  - status: pass (1 test)
  - runtime: ~25s wall (rebuild heavy; execution ~0.01s)
  - scope: Ensures docker tooling flags parse into expected structs and conflict handling.
  - nextest group: default

- binary: `perf_outdated` (path: `crates/deacon/tests/perf_outdated.rs`, profile: `dev-fast`)
  - status: pass (5 tests)
  - runtime: ~16s wall (compile-heavy)
  - scope: Benchmark-style assertions for outdated detection performance and cache hits.
  - nextest group: default

- binary: `up_prebuild` (path: `crates/deacon/tests/up_prebuild.rs`, profile: `dev-fast`)
  - status: pass (6 tests)
  - runtime: ~3s wall
  - scope: Validates prebuild hook orchestration and state tracking.
  - nextest group: default

- binaries with no runnable tests (`parity_utils`, `test_utils`, `unit_features_package`, `up_compose_profiles`, `up_reconnect`)
  - status: no tests executed (all tests skipped/ignored)
  - runtime: ~0–1s
  - scope: Helper/fixture modules; consider removing binaries or adding coverage if intended.
  - nextest group: default

- smoke suite (profile: `full`)
  - `smoke_basic`: pass 6 tests, ~4s wall (smoke group)
  - `smoke_cli`: pass 5 tests, ~1s wall (smoke-cli group)
  - `smoke_compose_edges`: pass 6 tests, ~22s wall (smoke group)
  - `smoke_doctor_text`: pass 3 tests, ~1s wall (smoke group)
  - `smoke_down`: pass 2 tests, ~31s wall (smoke group)
  - `smoke_exec`: pass 7 tests, ~156s wall (smoke group)
  - `smoke_exec_stdin`: pass 3 tests, ~93s wall (smoke group)
  - `smoke_lifecycle`: pass 5 tests, ~4s wall (smoke group)
  - `smoke_run_user_commands`: pass 5 tests, ~1s wall (smoke group)
  - `smoke_spinner`: pass 1 test, ~31s wall (smoke group)
  - `smoke_up_idempotent`: pass 3 tests, ~93s wall (smoke group)
  - scope: End-to-end CLI flows against docker/compose lifecycle, exec streaming, and spinner UX. Heavy runtimes suggest serial docker bringup/teardown dominates.

- parity suite (profile: `full`)
  - `parity_build`: pass 6 tests, ~0s wall
  - `parity_exec`: pass 4 tests, ~1s wall
  - `parity_read_configuration`: pass 2 tests, ~0s wall
  - `parity_up_exec`: pass 1 test, ~1s wall
  - `parity_utils`: no runnable tests
  - scope: Behavior parity checks with devcontainers CLI for build/exec/read-config flows.
  - nextest group: parity / parity-cli (per overrides)

- binary: `up_json_output` (path: `crates/deacon/tests/up_json_output.rs`, profile: `dev-fast`)
  - status: pass (7 tests)
  - runtime: ~2s wall
  - scope: Validates JSON output contract for `up` command including service state serialization.
  - nextest group: default

- fast-pass binaries (all `dev-fast` unless noted; each <3s wall, pass): `cli_flags_features_info`, `exec_id_label_cli`, `integration_build_args`, `integration_cli`, `integration_compose_enhancements`, `integration_container_lifecycle`, `integration_custom_container_name`, `integration_doctor`, `integration_down`, `integration_e2e`, `integration_entrypoint_compose`, `integration_exec`, `integration_exec_env`, `integration_exec_id_label`, `integration_exec_pty`, `integration_exec_selection`, `integration_extends`, `integration_fake_registry`, `integration_feature_dependencies`, `integration_feature_installation`, `integration_features`, `integration_features_info_dependencies`, `integration_features_info_local`, `integration_features_info_manifest`, `integration_features_info_tags`, `integration_features_info_verbose`, `integration_features_package`, `integration_features_publish`, `integration_host_requirements`, `integration_json_logging`, `integration_lifecycle`, `integration_logging`, `integration_mock_runtime`, `integration_mount`, `integration_override_secrets`, `integration_override_secrets_cli`, `integration_parallel_feature_installation`, `integration_per_command_events`, `integration_ports`, `integration_read_configuration`, `integration_read_configuration_output`, `integration_runtime_selection`, `integration_security`, `integration_spans`, `integration_template_apply`, `integration_templates` (core), `integration_user_mapping`, `integration_variable_substitution`, `integration_worktree`, `integration_outdated_extends`, `integration_outdated_fail_flag`, `integration_outdated_json`, `integration_outdated_local_features`, `integration_outdated_resilience`, `integration_outdated_text`, `integration_vulnerability_scan`, `json_output_purity`, `outdated_explicit_config`, `outdated_text_render`, `runtime_unavailable_integration`, `test_additional_features`, `test_features_cli`, `test_read_configuration_validation`, `test_templates_cli`, `up_config_resolution`, `up_validation`.
  - scope: mixture of CLI flag parsing, feature info/publish/output formatting, outdated reporting modes, exec/build/down/up command wiring, config readers, and packaging helpers.
  - nextest group: default unless covered by overrides noted earlier (fs-heavy for config/template/lockfile; docker-shared for feature publish/host requirements/port forwarding; docker-exclusive for `test_features_cli` when run under CI/full profiles).
