# context: quality-gates

Quality gates and command order:
- Build: cargo build --verbose
- Tests: cargo test --verbose -- --test-threads=1
- Doctests: cargo test --doc
- Format: cargo fmt --all; verify with cargo fmt --all -- --check
- Lint: cargo clippy --all-targets -- -D warnings
