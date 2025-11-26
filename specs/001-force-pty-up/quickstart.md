# Quickstart - Force PTY toggle for up lifecycle

1) Read the spec (`/workspaces/deacon/specs/001-force-pty-up/spec.md`) and research (`/workspaces/deacon/specs/001-force-pty-up/research.md`); align with Constitution gates (spec parity, no silent fallbacks, stdout/stderr separation).  
2) Implement PTY preference resolution: flag `--force-tty-if-json` overrides env `DEACON_FORCE_TTY_IF_JSON` (truthy `true/1/yes`; falsey `false/0/no`; unset disabled); default is no PTY. Apply only when JSON log mode is active.  
3) Apply resolved PTY to all lifecycle exec steps inside `deacon up`; preserve existing behavior for non-JSON mode and other exec entry points.  
4) Maintain strict stdout/stderr separation: JSON outputs remain on stdout; logs/diagnostics on stderr even under PTY. Surface clear errors if PTY allocation fails.  
5) Tests: add integration coverage for PTY-on (flag/env + JSON), PTY-off (unset), and exec regression. Configure any new integration binaries in `.config/nextest.toml` test groups. Use `make test-nextest-fast` for fast loops; expand to `make test-nextest` before PR if required.  
6) Tooling cadence: `cargo fmt --all`, `cargo fmt --all -- --check`, `cargo clippy --all-targets -- -D warnings`, then targeted `make test-nextest-*` per changes.  
7) Update examples/fixtures if lifecycle exec behavior or CLI flags impact documented workflows; keep README and exec.sh scripts aligned if touched.
