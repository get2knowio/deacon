# Legacy Normalization

Demonstrates that legacy `containerEnv` is normalized to `remoteEnv` in the parsed configuration.

Run from this directory:

```
cargo run -p deacon -- read-configuration --workspace-folder "$(pwd)" | jq .
```

What to look for:
- In the output `configuration`, you should see `remoteEnv` populated with `FOO=bar` (legacy `containerEnv` upgraded)
