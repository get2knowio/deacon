# Merge Config

Demonstrates using `--merge-config` to overlay (deep-merge) values on a base
configuration.

> Since [#285](https://github.com/get2knowio/deacon/issues/285), `--override-config`
> **replaces** the base config (reference parity). To *overlay* a fragment onto
> the base — as this example does — use `--merge-config` (repeatable; later wins).

Run from this directory:

```
# Base only
cargo run -p deacon -- read-configuration --workspace-folder "$(pwd)" | jq .

# With merge fragment applied
cargo run -p deacon -- read-configuration --workspace-folder "$(pwd)" \
  --merge-config "$(pwd)/override.jsonc" | jq .
```

What to look for:
- `configuration.remoteEnv.BASE` is `from-override` when override is supplied
- `configuration.remoteEnv.ONLY_OVERRIDE` is present only with override
- `configuration.workspaceFolder` is `/workspaces/override-demo` when override is supplied
