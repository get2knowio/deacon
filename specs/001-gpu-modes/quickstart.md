# Quickstart: GPU Mode Handling for Up

1. **Select GPU mode**
   - `--gpu-mode all`: always request GPUs for run/build/compose.
   - `--gpu-mode detect`: auto-detect GPUs; warn once and continue without GPUs if none.
   - `--gpu-mode none` (default): never request GPUs; no GPU warnings.

2. **Run up with GPU mode**
   ```bash
   # Always request GPUs
   cargo run -- up --gpu-mode all

   # Auto-detect GPUs with warning if absent
   cargo run -- up --gpu-mode detect

   # Explicit CPU-only
   cargo run -- up --gpu-mode none
   ```

3. **Compose-aware usage**
   - The selected GPU mode applies to all compose services started by `up`.
   - In detect mode on non-GPU hosts, expect a single warning before startup.

4. **Build path**
   - GPU mode is honored for build steps triggered by `up` where the runtime supports GPU flags.

5. **Validate behavior**
   - Verify GPU flags appear in docker run/build/compose invocations for mode `all`.
   - On GPU-less hosts with `detect`, confirm one warning is printed and no GPU flags are sent.
   - For `none`, ensure no GPU-related output appears.

6. **Development hygiene**
   - After changes: `cargo fmt --all && cargo fmt --all -- --check`
   - Lint: `cargo clippy --all-targets -- -D warnings`
   - Fast tests: `make test-nextest-fast`
