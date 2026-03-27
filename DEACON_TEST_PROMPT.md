# Maverick End-to-End Test: Deacon Consumer Core Completion

## What This Is

Run the Maverick autonomous development workflow against the Deacon project (a Rust DevContainer CLI) using a PRD that's already committed at `consumer-core-completion-prd.md` in the repo root. This tests the full pipeline: init → seed → plan → refuel → fly → land.

## Prerequisites

- `/workspaces/deacon` — the Deacon repo, checked out at `main` (commit `e49679c` or later, which includes the PRD)
- `/workspaces/maverick` — the Maverick repo, checked out at `main` (commit `a648770` or later, which includes all first-run fixes)
- `bd` CLI installed and on PATH (beads issue tracker)
- `jj` CLI installed and on PATH (Jujutsu VCS)
- `claude-agent-acp` on PATH (Claude Code ACP agent)
- `gh` CLI authenticated

## Critical: Resource Constraints for Rust Projects

Deacon is a Rust project. Cargo compilation is CPU/memory-intensive. If running in a constrained environment (VM, container, codespace), you MUST limit parallelism to prevent host crashes:

After `maverick init` creates `maverick.yaml`, edit it to set:
```yaml
validation:
  sync_cmd: [cargo, build, -j1]
  format_cmd: [cargo, fmt]
  lint_cmd: [cargo, clippy, -j1, --fix, --allow-dirty]
  test_cmd: [cargo, test, -j1]
  timeout_seconds: 600
parallel:
  max_agents: 1
  max_tasks: 1
```

Also add this to `CLAUDE.md` under "Critical Development Principles" after the build green section:
```
**Resource Constraints:**
This project may run in a resource-constrained environment. To avoid
overwhelming the host:
- Always pass `-j1` to cargo build/clippy/test commands to limit parallel jobs
- Do NOT run multiple cargo commands in parallel (run them sequentially with &&)
- Do NOT run cargo commands in background — wait for each to finish
- Prefer `make test-nextest-fast` over `cargo test` (nextest manages parallelism)
```

On a machine with plenty of resources (32+ cores, 64+ GB RAM), you can use `-j4` or higher instead of `-j1`, and `max_agents: 3` / `max_tasks: 5`.

## Steps

### 1. Initialize Maverick
```bash
uv run --project /workspaces/maverick maverick init
```
Then edit `maverick.yaml` as described above for your resource constraints.

### 2. Seed the Runway
```bash
uv run --project /workspaces/maverick maverick runway seed
```
This uses ACP with Read/Glob/Grep/Write tools to explore the codebase and write 4 semantic knowledge files. If it fails on first try with "no semantic files were written", retry — the ACP connection can be transient.

### 3. Generate Flight Plan from PRD
```bash
uv run --project /workspaces/maverick maverick plan generate consumer-core-completion --from-prd consumer-core-completion-prd.md
```
This runs a 4-agent pre-flight briefing room (Scopist, CodebaseAnalyst, CriteriaWriter, Contrarian) then generates a flight plan with success criteria. Takes ~10-15 minutes.

### 4. Decompose into Beads
```bash
uv run --project /workspaces/maverick maverick refuel consumer-core-completion
```
If the briefing agents fail (they return large JSON that can hit output limits), retry with `--skip-briefing`. The decomposer will produce ~10 work units and create beads via `bd`.

**Known issue:** The flight plan generator may use `SC-B1-default:` prefix format for success criteria instead of `[SC-001]` suffix format. The parser and validator handle both formats as of maverick commit `a648770`.

### 5. Fly (Implement Beads)
First, find the epic bead ID:
```bash
bd list --all | grep "epic.*consumer-core"
```
Then run fly with that ID:
```bash
uv run --project /workspaces/maverick maverick fly --epic <EPIC_ID> --auto-commit
```

**What to expect:**
- Each bead goes through: implement → gate check → review → commit
- Beads that fail review get up to 3 retries (MAX_RETRIES_PER_BEAD)
- After 3 failures, the bead is committed and a follow-up bead is created with the reviewer's findings via a `discovered-from` dependency link
- If the follow-up also fails 3x, Tier 2 escalation creates a re-planning bead
- The workflow checkpoints after every bead, so it survives host crashes — just re-run the same `fly` command to resume
- `--auto-commit` snapshots uncommitted changes before cloning the workspace

**The first run took ~4 hours on 16 cores with `-j4`.** With `-j1` it will be significantly slower. Beads that involve creating new Rust modules with unit tests (like the UID mapping build) tend to be the most review-contentious.

### 6. Land
```bash
uv run --project /workspaces/maverick maverick land --eject
```
This curates the commit history and creates a preview branch. Review it, then:
```bash
uv run --project /workspaces/maverick maverick land --finalize --branch maverick/preview/deacon
```

## What the PRD Contains (5 Beads)

1. **updateRemoteUserUID** — Implement UID/GID sync via ephemeral Dockerfile build (High priority, the biggest piece of work)
2. **Docker Compose Profiles** — Forward `--profile` flags to compose commands (High priority, clean additive feature)
3. **runArgs Passthrough** — Wire remaining `docker create` flags (Medium, likely already implemented)
4. **Feature Installation Timing** — Move features from runtime to image build (High, likely already implemented)
5. **License Housekeeping** — Fix Cargo.toml license field (Low, likely already correct)

The briefing agents typically discover that beads 3, 4, and 5 are already implemented and only need verification, so the real work concentrates on beads 1 and 2.

## Key Maverick Behaviors to Observe

- **Checkpoint resilience**: Kill the process mid-fly and restart — it should resume from the last checkpoint without redoing completed beads
- **Follow-up bead creation**: When a bead exhausts MAX_RETRIES_PER_BEAD (3), it commits the work and creates a follow-up task bead with the reviewer's findings, linked via `discovered-from` dependency
- **Tier 2 escalation**: If a follow-up bead also exhausts retries, it creates a re-planning bead instead of cascading indefinitely
- **Review scoping**: Reviews are scoped to the current bead's changed files (not the full workspace diff)
- **Verification beads**: Beads with "verification-only" or "no code changes expected" in their description are constrained to read-only tools
- **Runway episodic store**: After completion, check `.maverick/runway/episodic/bead-outcomes.jsonl` and `review-findings.jsonl` for recorded outcomes with `files_changed` and structured review findings
- **Snapshot warnings**: Large `--auto-commit` snapshots (>10 files) get a WARNING in the commit message

## Troubleshooting

- **"No more ready beads"**: You may have passed the epic name instead of the bead ID to `--epic`. Use `bd list --all | grep epic` to find the actual ID (e.g., `deacon-xyz`).
- **Circuit breaker (tool called N times)**: The limit is 100 same-tool calls. If agents are hitting it, the task may be too broad.
- **ACP stream buffer overflow**: Fixed at 1MB limit. If you see `LimitOverrunError`, the maverick version is too old.
- **SC coverage validation failure**: The parser supports both `[SC-001]` suffix and `SC-B1-default:` prefix formats. If all 19 SCs show as uncovered, check that the maverick version includes the parser fix.
- **Host crashes during fly**: This happened repeatedly with `-j4` and even `-j2` on a 16-core LXC container. Use `-j1` for stability, or ensure your host has adequate resources.
