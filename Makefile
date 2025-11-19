# Convenience build/test targets for deacon
# Use `make help` to list targets.

# Use bash for slightly more robust scripting in recipes
SHELL := /usr/bin/env bash

# Optional: override nextest concurrency from the command line
# Usage examples:
#   make test-nextest THREADS=8
#   make test-nextest-ci THREADS=num-cpus
# If unset, nextest uses profile defaults.
THREAD_ARGS = $(if $(THREADS),-j $(THREADS),)

# Optional: control output verbosity for test-nextest
# Default: quiet (minimal output). To enable regular/verbose statuses:
#   make test-nextest VERBOSE=1
ifeq ($(VERBOSE),1)
SHOW_PROGRESS ?= auto
STATUS_LEVEL ?= pass
else
SHOW_PROGRESS ?= none
STATUS_LEVEL ?= none
endif
OUTPUT_ARGS = --success-output never --failure-output immediate --show-progress $(SHOW_PROGRESS) --status-level $(STATUS_LEVEL)

.DEFAULT_GOAL := help

.PHONY: install-nextest
install-nextest: ## Install cargo-nextest if missing (auto)
	@set -euo pipefail; \
	if command -v cargo-nextest >/dev/null 2>&1; then \
	  echo "cargo-nextest already installed: $$(cargo nextest --version)"; \
	else \
	  echo "Installing cargo-nextest (locked)..."; \
	  cargo install cargo-nextest --locked; \
	  echo "Installed cargo-nextest: $$(cargo nextest --version)"; \
	fi

help: ## Show this help
	@echo "Deacon Makefile - Available Targets"
	@echo ""
	@echo "Build & Run:"
	@grep -E '^(build|run):.*?##' $(MAKEFILE_LIST) | sed -E 's/:.*?##/\t- /'
	@echo ""
	@echo "Testing (Serial):"
	@grep -E '^(test|test-fast|test-non-smoke|test-smoke|test-parity|test-parity-all|parity):.*?##' $(MAKEFILE_LIST) | sed -E 's/:.*?##/\t- /'
	@echo ""
	@echo "Testing (Parallel with cargo-nextest):"
	@grep -E '^(test-nextest-fast|test-nextest-unit|test-nextest-docker|test-nextest|test-nextest-ci|test-nextest-bg|test-nextest-audit):.*?##' $(MAKEFILE_LIST) | sed -E 's/:.*?##/\t- /'
	@echo ""
	@echo "Code Quality:"
	@grep -E '^(dev-fast|fmt|clippy|coverage):.*?##' $(MAKEFILE_LIST) | sed -E 's/:.*?##/\t- /'
	@echo ""
	@echo "Release Management:"
	@grep -E '^(release-check|release-run|release-assets|macos-artifact):.*?##' $(MAKEFILE_LIST) | sed -E 's/:.*?##/\t- /'
	@echo ""
	@echo "Maintenance:"
	@grep -E '^(clean|clean-branches):.*?##' $(MAKEFILE_LIST) | sed -E 's/:.*?##/\t- /'
	@echo ""
	@echo "For detailed nextest usage, see: docs/testing/nextest.md"
	@echo "For timing artifact details, see: artifacts/nextest/README.md"

build: ## Build (release)
	cargo build --release

run: ## Run CLI
	cargo run -- --help

test: ## Run all tests
	cargo test -- --test-threads=1

test-fast: ## Fast tests only: unit + bins + examples + doctests (no integration suites)
	@set -euo pipefail; \
	# Run unit/bins/examples in parallel (faster) \
	cargo test --workspace --lib --bins --examples; \
	# Run doctests separately \
	cargo test --doc

dev-fast: ## Fast local loop: fmt-check + clippy + fast tests (skip slow integration/smoke)
	@set -euo pipefail; \
	cargo fmt --all && cargo fmt --all -- --check; \
	cargo clippy --all-targets -- -D warnings; \
	$(MAKE) test-fast

test-nextest-fast: install-nextest ## Run fast parallel tests with cargo-nextest (excludes smoke/parity/docker)
	@set -euo pipefail; \
	./scripts/nextest/assert-installed.sh; \
	mkdir -p artifacts/nextest; \
	echo "Running nextest with dev-fast profile..."; \
	start_time=$$(date +%s); \
	if cargo nextest run --profile dev-fast $(THREAD_ARGS) --success-output never --failure-output immediate --show-progress none; then \
		end_time=$$(date +%s); \
		duration=$$((end_time - start_time)); \
		timestamp=$$(date -u +"%Y-%m-%dT%H:%M:%SZ"); \
		echo "{\"profile\":\"dev-fast\",\"duration_seconds\":$$duration,\"timestamp_utc\":\"$$timestamp\",\"exit_code\":0}" > artifacts/nextest/dev-fast-timing.json; \
		echo "✓ Tests passed in $${duration}s. Timing data: artifacts/nextest/dev-fast-timing.json"; \
	else \
		exit_code=$$?; \
		end_time=$$(date +%s); \
		duration=$$((end_time - start_time)); \
		timestamp=$$(date -u +"%Y-%m-%dT%H:%M:%SZ"); \
		echo "{\"profile\":\"dev-fast\",\"duration_seconds\":$$duration,\"timestamp_utc\":\"$$timestamp\",\"exit_code\":$$exit_code}" > artifacts/nextest/dev-fast-timing.json; \
		echo "✗ Tests failed after $${duration}s. Timing data: artifacts/nextest/dev-fast-timing.json"; \
		exit $$exit_code; \
	fi

test-nextest-unit: install-nextest ## Run only unit tests with nextest (super fast)
	cargo nextest run --profile unit

test-nextest-docker: install-nextest ## Run only docker integration tests
	cargo nextest run --profile docker

test-nextest: install-nextest ## Run full test suite with cargo-nextest (VERBOSE=1 for regular output)
	@set -euo pipefail; \
	./scripts/nextest/assert-installed.sh; \
	mkdir -p artifacts/nextest; \
	echo "Running nextest with full profile..."; \
	start_time=$$(date +%s); \
	if cargo nextest run --profile full $(THREAD_ARGS) $(OUTPUT_ARGS); then \
		end_time=$$(date +%s); \
		duration=$$((end_time - start_time)); \
		timestamp=$$(date -u +"%Y-%m-%dT%H:%M:%SZ"); \
		echo "{\"profile\":\"full\",\"duration_seconds\":$$duration,\"timestamp_utc\":\"$$timestamp\",\"exit_code\":0}" > artifacts/nextest/full-timing.json; \
		echo "✓ Tests passed in $${duration}s. Timing data: artifacts/nextest/full-timing.json"; \
	else \
		exit_code=$$?; \
		end_time=$$(date +%s); \
		duration=$$((end_time - start_time)); \
		timestamp=$$(date -u +"%Y-%m-%dT%H:%M:%SZ"); \
		echo "{\"profile\":\"full\",\"duration_seconds\":$$duration,\"timestamp_utc\":\"$$timestamp\",\"exit_code\":$$exit_code}" > artifacts/nextest/full-timing.json; \
		echo "✗ Tests failed after $${duration}s. Timing data: artifacts/nextest/full-timing.json"; \
		exit $$exit_code; \
	fi

test-nextest-bg: install-nextest ## Run nextest in background (optional: FILTER='nextest expr'), logging to artifacts/nextest/full-bg-<ts>.log
	@set -euo pipefail; \
	./scripts/nextest/assert-installed.sh; \
	mkdir -p artifacts/nextest; \
	ts=$$(date -u +"%Y%m%dT%H%M%SZ"); \
	log="artifacts/nextest/full-bg-$${ts}.log"; \
	echo "Starting cargo-nextest (profile=full) in background..."; \
	if [[ -n "$$FILTER" ]]; then echo "Filter: $$FILTER"; fi; \
	echo "Log: $$log"; \
	if [[ -n "$$FILTER" ]]; then \
	  nohup cargo nextest run --profile full $(THREAD_ARGS) "$$FILTER" --success-output never --failure-output immediate --show-progress none --status-level none --final-status-reporter json > "$$log" 2>&1 & echo $$! > artifacts/nextest/full-bg.pid; \
	else \
	  nohup cargo nextest run --profile full $(THREAD_ARGS) --success-output never --failure-output immediate --show-progress none --status-level none --final-status-reporter json > "$$log" 2>&1 & echo $$! > artifacts/nextest/full-bg.pid; \
	fi; \
	echo "PID: $$(cat artifacts/nextest/full-bg.pid)"; \
	echo "Tail: tail -f $$log"

.PHONY: test-nextest-bg-smoke
test-nextest-bg-smoke: ## Run only smoke+parity tests in background (most likely long-running)
	@FILTER="test(smoke_) | test(parity_)" $(MAKE) test-nextest-bg

test-nextest-ci: install-nextest ## Run CI test suite with cargo-nextest (two-pass: general + auth-failure tests without token)
	@set -euo pipefail; \
	./scripts/nextest/assert-installed.sh; \
	mkdir -p artifacts/nextest; \
	echo "Running nextest with ci profile (phase 1: general tests)..."; \
	start_time=$$(date +%s); \
	# Exclude auth-failure tests from phase 1; they run in phase 2 with token unset
	PHASE1_FILTER="not ( test(manifest_auth_failure_*) or test(tags_auth_failure_*) or test(verbose_auth_failure_*) )"; \
	cargo nextest run --profile ci $(THREAD_ARGS) --success-output never --failure-output immediate --show-progress none --filter-expr "$$PHASE1_FILTER"; \
	echo "Running nextest with ci profile (phase 2: auth-failure tests, token unset)..."; \
	# Unset DEACON_REGISTRY_TOKEN for this invocation to force unauthenticated flows
	if env -u DEACON_REGISTRY_TOKEN cargo nextest run --profile ci $(THREAD_ARGS) --success-output never --failure-output immediate --show-progress none --filter-expr "test(manifest_auth_failure_*) or test(tags_auth_failure_*) or test(verbose_auth_failure_*)"; then \
		end_time=$$(date +%s); \
		duration=$$((end_time - start_time)); \
		timestamp=$$(date -u +"%Y-%m-%dT%H:%M:%SZ"); \
		echo "{\"profile\":\"ci\",\"duration_seconds\":$$duration,\"timestamp_utc\":\"$$timestamp\",\"exit_code\":0}" > artifacts/nextest/ci-timing.json; \
		echo "✓ Tests passed in $${duration}s. Timing data: artifacts/nextest/ci-timing.json"; \
		if [[ -f artifacts/nextest/baseline-timing.json ]]; then \
			baseline_duration=$$(jq -r '.duration_seconds // 0' artifacts/nextest/baseline-timing.json); \
			if [[ $$baseline_duration -gt 0 ]]; then \
				improvement=$$(awk "BEGIN {printf \"%.1f\", (1 - $$duration / $$baseline_duration) * 100}"); \
				echo "⚡ Runtime improvement: $${improvement}% faster than baseline ($${baseline_duration}s → $${duration}s)"; \
			fi; \
		fi; \
	else \
		exit_code=$$?; \
		end_time=$$(date +%s); \
		duration=$$((end_time - start_time)); \
		timestamp=$$(date -u +"%Y-%m-%dT%H:%M:%SZ"); \
		echo "{\"profile\":\"ci\",\"duration_seconds\":$$duration,\"timestamp_utc\":\"$$timestamp\",\"exit_code\":$$exit_code}" > artifacts/nextest/ci-timing.json; \
		echo "✗ Tests failed after $${duration}s. Timing data: artifacts/nextest/ci-timing.json"; \
		exit $$exit_code; \
	fi

test-nextest-audit: install-nextest ## Audit test group assignments with cargo-nextest
	@set -euo pipefail; \
	./scripts/nextest/assert-installed.sh; \
	echo "Auditing test group assignments..."; \
	echo ""; \
	echo "=== Test Groups Configuration ==="; \
	cargo nextest show-config test-groups; \
	echo ""; \
	echo "=== All Tests (with details) ==="; \
	cargo nextest list --verbose; \
	echo ""; \
	echo "For detailed classification guidelines, see: docs/testing/nextest.md"

test-non-smoke: ## Run unit tests + non-smoke integration tests (matches CI 'test' job)
		@set -euo pipefail; \
		NON_SMOKE_TESTS=$$(find crates -type f -path '*/tests/*.rs' -not -name 'smoke_*.rs' -printf '%f\n' | sed 's/\.rs$$//' | sort -u); \
		echo "Including non-smoke integration tests:"; \
		if [[ -n "$$NON_SMOKE_TESTS" ]]; then printf '%s\n' $$NON_SMOKE_TESTS; else echo "(none found)"; fi; \
		# Run unit/bins/examples first (cannot combine --doc with --test selection)
		cargo test --verbose --workspace --lib --bins --examples -- --test-threads=1; \
		# Then run the non-smoke integration tests by filename stem if any discovered
		if [[ -n "$$NON_SMOKE_TESTS" ]]; then \
			cargo test --verbose $$(printf -- '--test %s ' $$NON_SMOKE_TESTS) -- --test-threads=1; \
		fi

test-smoke: ## Run smoke tests only (all files matching tests/smoke_*.rs) (matches CI 'smoke' job)
	@set -euo pipefail; \
	SMOKE_TESTS=$$(find crates -type f -path '*/tests/smoke_*.rs' -printf '%f\n' | sed 's/\.rs$$//' | sort -u); \
	if [[ -z "$$SMOKE_TESTS" ]]; then echo "No smoke tests found."; exit 1; fi; \
	echo "Found smoke tests:"; printf '%s\n' $$SMOKE_TESTS; \
	cargo test --verbose $$(printf -- '--test %s ' $$SMOKE_TESTS) -- --test-threads=1

test-parity: ## Run parity tests (requires devcontainer CLI and Docker)
	@set -euo pipefail; \
	BIN="$${DEACON_PARITY_DEVCONTAINER:-$$(command -v devcontainer || true)}"; \
	if [[ -z "$$BIN" ]]; then \
	  echo "devcontainer CLI not found. Set DEACON_PARITY_DEVCONTAINER=/path/to/devcontainer or add to PATH."; \
	  exit 1; \
	fi; \
	echo "Using devcontainer: $$BIN"; \
	DEACON_PARITY=1 \
	DEACON_PARITY_DEVCONTAINER="$$BIN" \
	DEACON_PARITY_UPSTREAM_READ_CONFIGURATION='read-configuration --config {config} --workspace-folder {workspace}' \
	cargo test -p deacon \
	  --test parity_read_configuration \
	  --test parity_up_exec \
	  --test parity_exec \
	  --test parity_build \
	  -- --nocapture --test-threads=1

.PHONY: test-parity-all
test-parity-all: ## Alias for test-parity (runs parity read-config, up+exec, exec)
	$(MAKE) test-parity

fmt: ## Format all code
	cargo fmt --all

clippy: ## Run clippy with warnings as errors
	cargo clippy --all-targets -- -D warnings

coverage: ## Generate coverage report
	cargo llvm-cov --workspace --open

clean: ## Clean build artifacts
	cargo clean

release-check: ## Full quality gate
	cargo fmt --all && cargo fmt --all -- --check && \
	cargo clippy --all-targets -- -D warnings && \
	cargo test -- --test-threads=1 && \
	cargo build --release

.PHONY: release-run
release-run: ## Dispatch 'Release' workflow for TAG=vX.Y.Z and watch until completion (requires gh)
	@set -euo pipefail; \
	if ! command -v gh >/dev/null 2>&1; then \
	  echo "Error: GitHub CLI 'gh' not found in PATH."; \
	  exit 1; \
	fi; \
	TAG="$${TAG:-}"; \
	if [[ -z "$$TAG" ]]; then \
	  echo "Usage: make release-run TAG=v0.1.4"; \
	  exit 1; \
	fi; \
	echo "Dispatching Release workflow for $$TAG..."; \
	gh workflow run Release --ref main -f version="$$TAG"; \
	echo "Waiting for workflow run to be registered..."; \
	sleep 2; \
	run_id=$$(gh run list --workflow "Release" --limit 1 --json databaseId --jq '.[0].databaseId' 2>/dev/null || true); \
	if [[ -z "$$run_id" ]]; then \
	  echo "Error: Could not determine dispatched run id."; \
	  exit 1; \
	fi; \
	echo "Watching run $$run_id..."; \
	gh run watch "$$run_id" --interval 10 --exit-status; \
	echo "Run $$run_id completed."

.PHONY: release-assets
release-assets: ## List assets for a release TAG=vX.Y.Z (requires gh)
	@set -euo pipefail; \
	if ! command -v gh >/dev/null 2>&1; then \
	  echo "Error: GitHub CLI 'gh' not found in PATH."; \
	  exit 1; \
	fi; \
	TAG="$${TAG:-}"; \
	if [[ -z "$$TAG" ]]; then \
	  echo "Usage: make release-assets TAG=v0.1.4"; \
	  exit 1; \
	fi; \
	echo "Assets for $$TAG:"; \
	gh release view "$$TAG" --json assets --jq '.assets[].name' | sort || true

.PHONY: macos-artifact
macos-artifact: ## Rebuild macOS artifact via GitHub Actions and download to artifacts/deacon
	@set -euo pipefail; \
	WORKFLOW="Build macOS (Apple Silicon)"; \
	echo "Cleaning previous artifact(s)..."; \
	rm -f ./artifacts/deacon || true; \
	rm -rf ./artifacts/deacon-macos-aarch64 || true; \
	if ! command -v gh >/dev/null 2>&1; then \
	  echo "Error: GitHub CLI 'gh' not found in PATH."; \
	  exit 1; \
	fi; \
	echo "Triggering workflow: $$WORKFLOW"; \
	gh workflow run "$$WORKFLOW"; \
	echo "Waiting for workflow run to start..."; \
	sleep 2; \
	echo "Polling latest run for workflow '$$WORKFLOW'..."; \
	# Poll until the latest run for the workflow completes; capture id/status/conclusion each loop. \
	while :; do \
	  run_id=$$(gh run list --workflow "$$WORKFLOW" --limit 1 --json databaseId --jq '.[0].databaseId' 2>/dev/null || true); \
	  status=$$(gh run list --workflow "$$WORKFLOW" --limit 1 --json status --jq '.[0].status' 2>/dev/null || true); \
	  conclusion=$$(gh run list --workflow "$$WORKFLOW" --limit 1 --json conclusion --jq '.[0].conclusion' 2>/dev/null || true); \
	  if [[ -z "$$run_id" || -z "$$status" ]]; then \
	    printf '.'; sleep 3; continue; \
	  fi; \
	  printf "\rRun ID: %s  Status: %s  Conclusion: %s" "$$run_id" "$$status" "$$conclusion"; \
	  if [[ "$$status" == "completed" ]]; then echo ""; break; fi; \
	  sleep 5; \
	done; \
	if [[ "$$conclusion" != "success" ]]; then \
	  echo "Workflow concluded with status '$$conclusion' (run $$run_id)"; \
	  exit 1; \
	fi; \
	echo "Downloading artifact 'deacon-macos-aarch64' from run $$run_id..."; \
	mkdir -p ./artifacts; \
	gh run download "$$run_id" --name deacon-macos-aarch64 --dir ./artifacts; \
	# Move resulting binary into ./artifacts/deacon if present; otherwise leave directory content intact. \
	if [[ -f ./artifacts/deacon-macos-aarch64/deacon ]]; then \
	  mv -f ./artifacts/deacon-macos-aarch64/deacon ./artifacts/deacon; \
	  rm -rf ./artifacts/deacon-macos-aarch64; \
	elif [[ $$(find ./artifacts/deacon-macos-aarch64 -maxdepth 1 -type f | wc -l) -eq 1 ]]; then \
	  f=$$(find ./artifacts/deacon-macos-aarch64 -maxdepth 1 -type f | head -n1); \
	  mv -f "$$f" ./artifacts/deacon; \
	  rm -rf ./artifacts/deacon-macos-aarch64; \
	else \
	  echo "Downloaded files under ./artifacts/deacon-macos-aarch64/. Please move the desired binary to ./artifacts/deacon manually."; \
	fi; \
	echo "Done. Artifact at ./artifacts/deacon"

.PHONY: test-parity parity
parity: test-parity ## Alias for test-parity

.PHONY: clean-branches
clean-branches: ## Delete local and remote branches fully merged into the default branch
	set -euo pipefail; \
	# Determine default branch from origin/HEAD, fallback to 'main' if undetectable. \
	default_branch=$$(git symbolic-ref --quiet --short refs/remotes/origin/HEAD 2>/dev/null | sed 's|origin/||'); \
	if [[ -z "$${default_branch:-}" ]]; then \
	  default_branch=$$(git remote show origin | sed -n 's/.*HEAD branch: //p'); \
	fi; \
	if [[ -z "$${default_branch:-}" ]]; then default_branch=main; fi; \
	echo "Default branch detected: '$${default_branch}'"; \
	# Ensure we are on the default branch locally and up to date. \
	git fetch --all --prune; \
	git checkout "$${default_branch}"; \
	# Identify remote branches fully merged into origin/<default_branch> (exclude HEAD and default). \
	remote_merged=$$(git for-each-ref 'refs/remotes/origin/*' --merged "refs/remotes/origin/$${default_branch}" --format='%(refname:short)' \
	  | grep -E '^origin/.\+' \
	  | grep -vE "^origin/(HEAD|$${default_branch})$$" \
	  | sort -u || true); \
	echo "Merged remote branches to delete:"; echo "$${remote_merged:-<none>}"; \
	if [[ -n "$${remote_merged:-}" ]]; then \
	  while IFS= read -r rref; do \
	    [[ -z "$${rref}" ]] && continue; \
	    bname=$${rref#origin/}; \
	    echo "Deleting remote branch '$${bname}'"; \
	    git push origin --delete "$${bname}" || echo "Warning: could not delete remote '$${bname}' (may be protected or already gone)"; \
	  done <<< "$${remote_merged}"; \
	fi; \
	# Prune any stale remote refs after deletion. \
	git remote prune origin || true; \
	# Identify local branches fully merged into <default_branch> (exclude the default branch). \
	local_merged=$$(git for-each-ref refs/heads --merged "$${default_branch}" --format='%(refname:short)' | grep -vE "^$${default_branch}$$" || true); \
	echo "Merged local branches to delete:"; echo "$${local_merged:-<none>}"; \
	if [[ -n "$${local_merged:-}" ]]; then \
	  while IFS= read -r lref; do \
	    [[ -z "$${lref}" ]] && continue; \
	    echo "Deleting local branch '$${lref}'"; \
	    git branch -d "$${lref}" || echo "Warning: could not delete local '$${lref}'"; \
	  done <<< "$${local_merged}"; \
	fi; \
	echo "Branch cleanup complete."

.PHONY: test-non-smoke test-smoke
