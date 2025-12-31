# Feature and Dotfiles Fixture

Test fixture for feature installation, dotfiles integration, and lifecycle orchestration.

## Purpose
- Validate feature-driven image extension with BuildKit
- Test dotfiles repository cloning and installation
- Verify lifecycle hook execution order (features → updateContent → dotfiles → postCreate)
- Test prebuild mode (stops after updateContent)

## Usage

### Regular up with features and dotfiles
```bash
cargo run -- up \
  --workspace-folder fixtures/devcontainer-up/feature-and-dotfiles \
  --dotfiles-repository https://github.com/example/dotfiles \
  --dotfiles-install-command "install.sh"
```

### Prebuild mode (CI scenario)
```bash
cargo run -- up \
  --workspace-folder fixtures/devcontainer-up/feature-and-dotfiles \
  --prebuild
```

## Expected Behavior
- Features installed and merged into image metadata
- Dotfiles cloned and installation command executed (if provided)
- Lifecycle hooks execute in correct order
- Prebuild stops after updateContent, does not run postCreate/postAttach
- JSON success output includes feature provenance
