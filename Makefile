# Convenience build/test targets for deacon
# Use `make help` to list targets.

.DEFAULT_GOAL := help

FEATURES_FULL := --no-default-features --features "docker,config,plugins,json-logs"

help: ## Show this help
	@grep -E '^[a-zA-Z_-]+:.*?##' $(MAKEFILE_LIST) | sed -E 's/:.*?##/\t- /'

build: ## Build with default features (release)
	cargo build --release

build-full: ## Build full feature set (docker,config,plugins,json-logs)
	cargo build --release $(FEATURES_FULL)

build-minimal: ## Build with no default features
	cargo build --release --no-default-features

run: ## Run CLI with default features
	cargo run -- --help

run-full: ## Run CLI built with full feature set
	cargo run -- $(FEATURES_FULL) --help

test: ## Run all tests (default features)
	cargo test -- --test-threads=1

test-full: ## Test full feature set
	cargo test $(FEATURES_FULL) -- --test-threads=1

fmt: ## Format all code
	cargo fmt --all

clippy: ## Run clippy with warnings as errors (default features)
	cargo clippy --all-targets -- -D warnings

clippy-full: ## Clippy for full feature build
	cargo clippy $(FEATURES_FULL) --all-targets -- -D warnings

coverage: ## Generate coverage report (default features)
	cargo llvm-cov --workspace --open

clean: ## Clean build artifacts
	cargo clean

release-check: ## Full quality gate (default + full variant)
	cargo fmt --all && cargo fmt --all -- --check && \
	cargo clippy --all-targets -- -D warnings && \
	cargo test -- --test-threads=1 && \
	cargo build --release && \
	cargo build --release $(FEATURES_FULL)
