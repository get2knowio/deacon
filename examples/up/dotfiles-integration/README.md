# Dotfiles Integration Example

## Overview

This example demonstrates how to integrate personal dotfiles into a dev container using the `--dotfiles-repository`, `--dotfiles-install-command`, and `--dotfiles-target-path` flags.

## What are Dotfiles?

Dotfiles are personal configuration files (e.g., `.bashrc`, `.vimrc`, `.gitconfig`) that customize your development environment. The `up` command can automatically clone and install dotfiles into the container.

## Usage

### Basic Dotfiles Integration

Clone dotfiles to the default location (`~/dotfiles`):

```bash
deacon up --workspace-folder . \
  --dotfiles-repository https://github.com/your-username/dotfiles
```

This will:
1. Clone the repository to `~/dotfiles`
2. Look for and run `install.sh` in the repository root
3. Continue with the normal up workflow

### Custom Install Command

Specify a custom installation script:

```bash
deacon up --workspace-folder . \
  --dotfiles-repository https://github.com/your-username/dotfiles \
  --dotfiles-install-command "./setup.sh"
```

### Custom Target Path

Install dotfiles to a custom directory:

```bash
deacon up --workspace-folder . \
  --dotfiles-repository https://github.com/your-username/dotfiles \
  --dotfiles-target-path ~/.config/dotfiles
```

### All Options Combined

```bash
deacon up --workspace-folder . \
  --dotfiles-repository https://github.com/your-username/dotfiles \
  --dotfiles-install-command "./custom-install.sh --minimal" \
  --dotfiles-target-path ~/.my-dotfiles
```

## Dotfiles Repository Structure

Your dotfiles repository should follow this structure:

```
dotfiles/
├── install.sh          # Default install script (optional)
├── .bashrc            # Bash configuration
├── .gitconfig         # Git configuration
├── .vimrc             # Vim configuration
└── custom-install.sh  # Custom install script (optional)
```

### Example install.sh

```bash
#!/bin/bash
set -e

# Link configuration files
ln -sf ~/dotfiles/.bashrc ~/.bashrc
ln -sf ~/dotfiles/.gitconfig ~/.gitconfig
ln -sf ~/dotfiles/.vimrc ~/.vimrc

echo "Dotfiles installed successfully!"
```

## Execution Order

Dotfiles are installed during the lifecycle sequence:

```
1. onCreateCommand
2. updateContentCommand
3. Dotfiles Installation ← happens here
4. postCreateCommand
5. postStartCommand
6. postAttachCommand
```

## Idempotency

Dotfiles installation is idempotent:
- Tracked via markers to prevent duplicate installation
- Running `up` again won't reinstall dotfiles
- Use `--remove-existing-container` to force reinstallation

## Skipping Dotfiles

Dotfiles are skipped when:
- `--skip-post-create` flag is used
- No `--dotfiles-repository` is provided

```bash
# This will NOT install dotfiles
deacon up --workspace-folder . --skip-post-create
```

## Testing Dotfiles Installation

After running with dotfiles:

```bash
# Check dotfiles were cloned
docker exec <container-id> ls -la ~/dotfiles

# Verify install script ran
docker exec <container-id> cat ~/.bashrc

# Check custom configurations
docker exec <container-id> git config --get user.name
```

## Expected Output

```json
{
  "outcome": "success",
  "containerId": "<container-id>",
  "remoteUser": "vscode",
  "remoteWorkspaceFolder": "/workspace"
}
```

## Common Dotfiles Repositories

Many developers publish their dotfiles publicly. Examples:
- `https://github.com/username/dotfiles`
- `https://github.com/username/.dotfiles`
- `https://github.com/username/dot-files`

## Private Repositories

For private dotfiles repositories, ensure:
1. SSH keys are configured in your environment
2. Git credentials are available
3. The container has network access to GitHub/GitLab

```bash
# With SSH key
ssh-add ~/.ssh/id_rsa
deacon up --workspace-folder . \
  --dotfiles-repository git@github.com:your-username/dotfiles.git
```

## Cleanup

```bash
docker rm -f <container-id>
```

## Related Examples

- `lifecycle-hooks/` - Lifecycle execution order
- `remote-env-secrets/` - Environment and secrets management
- `basic-image/` - Simple setup without dotfiles
