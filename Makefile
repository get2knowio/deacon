# Convenience build/test targets for deacon
# Use `make help` to list targets.

.DEFAULT_GOAL := help

help: ## Show this help
	@grep -E '^[a-zA-Z_-]+:.*?##' $(MAKEFILE_LIST) | sed -E 's/:.*?##/\t- /'

build: ## Build (release)
	cargo build --release

run: ## Run CLI
	cargo run -- --help

test: ## Run all tests
	cargo test -- --test-threads=1

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
