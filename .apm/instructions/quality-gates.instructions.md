---
description: "Build, test, doctest, fmt, and clippy gates to keep CI green"
---

# Quality Gates

Run these after every change:
- cargo build --verbose
- cargo test --verbose -- --test-threads=1
- cargo test --doc
- cargo fmt --all && cargo fmt --all -- --check
- cargo clippy --all-targets -- -D warnings
