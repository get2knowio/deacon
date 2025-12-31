# Override Config

Demonstrates using `--override-config` to overlay values on a base configuration.

Run from this directory:

```
# Base only
cargo run -p deacon -- read-configuration --workspace-folder "$(pwd)" | jq .

# With override applied
cargo run -p deacon -- read-configuration --workspace-folder "$(pwd)" \
  --override-config "$(pwd)/override.jsonc" | jq .
```

What to look for:
- `configuration.remoteEnv.BASE` is `from-override` when override is supplied
- `configuration.remoteEnv.ONLY_OVERRIDE` is present only with override
- `configuration.workspaceFolder` is `/workspaces/override-demo` when override is supplied
