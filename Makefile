# Convenience build/test targets for deacon
# Use `make help` to list targets.

# Use bash for slightly more robust scripting in recipes
SHELL := /usr/bin/env bash

.DEFAULT_GOAL := help

help: ## Show this help
	@grep -E '^[a-zA-Z_-]+:.*?##' $(MAKEFILE_LIST) | sed -E 's/:.*?##/\t- /'

build: ## Build (release)
	cargo build --release

run: ## Run CLI
	cargo run -- --help

test: ## Run all tests
	cargo test -- --test-threads=1

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
	cargo test -p deacon --test parity_read_configuration --test parity_up_exec -- --nocapture

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
