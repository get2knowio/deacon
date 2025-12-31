# Single Container Fixture

Test fixture for basic single-container devcontainer scenarios.

## Purpose
- Validate basic `deacon up` invocation with image-based devcontainer
- Test flag coverage: workspace folder, remote env, lifecycle hooks
- Verify JSON output contract

## Usage
```bash
cargo run -- up --workspace-folder fixtures/devcontainer-up/single-container --include-configuration
```

## Expected Behavior
- Container created from base Ubuntu image
- Post-create and update content commands execute
- JSON success output emitted to stdout
- Logs appear on stderr only
