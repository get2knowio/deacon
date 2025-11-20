# Compose with Profiles Fixture

Test fixture for Docker Compose scenarios with profiles and environment files.

## Purpose
- Validate compose-based devcontainer with profile selection
- Test .env file parsing and project name propagation
- Verify mount conversion to compose volumes
- Test multi-service orchestration

## Usage
```bash
cargo run -- up \
  --workspace-folder fixtures/devcontainer-up/compose-with-profiles \
  --config devcontainer.json \
  --mount type=bind,source=/tmp/cache,target=/cache \
  --id-label project=compose-test
```

## Expected Behavior
- Compose services start with dev profile
- Project name from .env is applied
- Additional mounts converted to compose volumes
- ID labels applied for reconnection
- JSON success output includes composeProjectName
