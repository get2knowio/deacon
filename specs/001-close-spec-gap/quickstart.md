# Quickstart — Close Spec Gap (Features Plan)

This feature closes gaps in the “features plan” behavior: strict input validation, deterministic order, and precise graph semantics.

## Prerequisites
- Rust stable toolchain (see `rust-toolchain.toml`)
- This repo checked out: /workspaces/deacon

## Fast dev loop

```bash
# Format + clippy + unit/examples + doctests
make dev-fast
```

## What will be implemented
- Validate `--additional-features` is a JSON object (fail fast otherwise)
- Reject local feature paths (use registry references only)
- Build deterministic `{ order, graph }` where:
  - `order` is topological with lexicographic tie-breakers
  - `graph` lists direct deps: union(installsAfter, dependsOn), deduped and sorted
- Shallow merge of option maps with CLI precedence
- Fail fast on registry metadata fetch errors (401/403/404/network)
- JSON output contract: plan JSON to stdout; logs to stderr

## Try it (after implementation)

```bash
# Show CLI help
cargo run -p deacon -- features plan --help
```

Expected output:
```
Generate feature installation plan

Note: Variable substitution is not performed during planning; feature IDs are treated
 as opaque strings; option values pass through unchanged and are not normalized or transformed.                                                                           
Usage: deacon features plan [OPTIONS]

Options:
      --json <JSON>
          Output in JSON format
          
          [default: true]
          [possible values: true, false]

      --log-format <LOG_FORMAT>
          Log format (text or json, defaults to text, can be set via DEACON_LOG_FORMA
T env var)                                                                           
          Possible values:
          - text: Human-readable text format
          - json: JSON structured format

      --additional-features <ADDITIONAL_FEATURES>
          Additional features to install (JSON object map of id -> value/options) Acc
epts a JSON object like {"ghcr.io/devcontainers/node": "18", "git": true}            
      --log-level <LOG_LEVEL>
          Log level

          Possible values:
          - error: Error messages only
          - warn:  Warning and error messages
          - info:  Informational messages and above
          - debug: Debug messages and above
          - trace: All messages including trace
          
          [default: info]

      --workspace-folder <PATH>
          Workspace folder path

      --config <PATH>
          Configuration file path

      --override-config <PATH>
          Override configuration file path (highest precedence)

      --secrets-file <PATH>
          Secrets file path (KEY=VALUE format, can be specified multiple times)

      --no-redact
          Disable secret redaction in output (debugging only - WARNING: may expose se
crets)                                                                               
      --progress <PROGRESS>
          Progress format (json|none|auto). Auto is silent unless --progress-file is 
set                                                                                  
          Possible values:
          - none: No progress output
          - json: JSON structured progress events
          - auto: Auto mode: silent unless --progress-file is set (future: TTY spinne
r)                                                                                             
          [default: auto]

      --progress-file <PATH>
          Progress file path (for JSON output when using --progress auto or json)

      --plugin <NAME>
          Enable specific plugins

      --runtime <RUNTIME>
          Container runtime to use (docker or podman, can be set via DEACON_RUNTIME e
nv var)                                                                              
          Possible values:
          - docker: Docker runtime
          - podman: Podman runtime

      --docker-path <DOCKER_PATH>
          Path to docker executable
          
          [default: docker]

      --docker-compose-path <DOCKER_COMPOSE_PATH>
          Path to docker-compose executable
          
          [default: docker-compose]

      --terminal-columns <TERMINAL_COLUMNS>
          Terminal columns for output formatting (requires --terminal-rows)

      --terminal-rows <TERMINAL_ROWS>
          Terminal rows for output formatting (requires --terminal-columns)

  -h, --help
          Print help (see a summary with '-h')
```

```bash
# Basic usage (requires registry authentication)
cargo run -p deacon -- features plan --config examples/features/lockfile-demo/devcontainer.json \
  --additional-features '{"ghcr.io/devcontainers/features/git:1": {"version": "latest"}}'
```

Expected output (when authenticated):
- JSON output with `order` array and `graph` object showing deterministic installation order
- Features from both config and `--additional-features` merged with CLI precedence
- Graph shows direct dependencies (union of `installsAfter` and `dependsOn`) sorted lexicographically

Expected output (without authentication):
- Command fails with authentication error messages for registry fetches
- No partial plan is emitted (fail-fast behavior)

Notes
- Tests should cover invalid JSON, local paths, cycle detection, tie-break ordering, and merge behavior.
- Keep build green: fmt, clippy, tests.
