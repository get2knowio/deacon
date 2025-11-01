# Basic

Minimal configuration; shows discovery, parsing, and output structure.

Run from this directory:

```
cargo run -p deacon -- read-configuration --workspace-folder "$(pwd)" | jq .
```

What to look for:
- `configuration.name` is `rc-basic`
- `configuration.remoteEnv.EXAMPLE` is `basic`
