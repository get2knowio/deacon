# auth-local-registry

What this demonstrates
- Using `DEVCONTAINERS_OCI_AUTH` environment variable to provide registry auth for testing against a local registry.

Files
- `devcontainer-feature.json` — simple feature

Commands
```sh
# Example assumes a local registry running at localhost:5000 and credentials myuser:mypass
export DEVCONTAINERS_OCI_AUTH="localhost:5000|myuser|mypass"

# Dry-run publish (shows planned operations)
deacon features publish . --namespace localtest/myfeatures --registry localhost:5000 --dry-run --progress json

# To actually push, remove --dry-run and ensure the local registry accepts credentials
# deacon features publish . --namespace localtest/myfeatures --registry localhost:5000 --progress json
```

Notes
- This example does not start a local registry; it's intended to show how to set auth env var for the publish command.
