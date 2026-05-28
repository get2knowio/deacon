# Extends Chain

Demonstrates chained configuration via `extends` (base -> mid -> leaf).

Structure:
- `base/devcontainer.json` — base config, loaded only via `extends`.
- `mid/devcontainer.json` — extends base, loaded only via `extends`.
- `leaf/.devcontainer.json` — leaf config at the workspace root (the
  spec-mandated discovery location), extends mid.

Run from the leaf directory:

```
# Use workspace-folder discovery (finds leaf/.devcontainer.json)
(cd leaf && cargo run -p deacon -- read-configuration --workspace-folder "$(pwd)" | jq .)

# Or target the leaf config explicitly
cargo run -p deacon -- read-configuration --workspace-folder "$(pwd)/leaf" --config "$(pwd)/leaf/.devcontainer.json" | jq .
```

What to look for:
- `configuration.remoteEnv` contains keys from BASE, MID, and LEAF
- `configuration.name` is `rc-extends-leaf`
