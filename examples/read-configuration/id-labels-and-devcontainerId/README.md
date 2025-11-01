# ID Labels and ${devcontainerId}

Demonstrates `${devcontainerId}` substitution using `--id-label` values.

Run from this directory:

```
# Provide one or more id-labels (format name=value)
cargo run -p deacon -- read-configuration --workspace-folder "$(pwd)" \
  --id-label com.example.project=rc-demo \
  --id-label com.example.user=$(whoami) | jq .
```

What to look for:
- `configuration.name` expands `${devcontainerId}` deterministically from the id-labels
- `configuration.remoteEnv.DEVCONTAINER_ID_SAMPLE` matches the same value
- Changing the set of labels changes the computed ID; changing order does not
