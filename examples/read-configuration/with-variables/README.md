# With Variables

Demonstrates variable substitution: `${localEnv:...}` and `${localWorkspaceFolderBasename}`.

Run from this directory:

```
# Without setting env var (defaults used):
cargo run -p deacon -- read-configuration --workspace-folder "$(pwd)" | jq .

# With env var influencing substitution:
MY_VAR=hello cargo run -p deacon -- read-configuration --workspace-folder "$(pwd)" | jq .
```

What to look for:
- `configuration.name` includes this folder name
- `configuration.remoteEnv.FROM_LOCAL_ENV` reflects `MY_VAR` or `default-value`
- `configuration.remoteEnv.WS_BASENAME` equals the folder name
