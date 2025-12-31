# Extends Chain

Demonstrates chained configuration via `extends` (base -> mid -> leaf).

Structure:
- `base/devcontainer.json`
- `mid/devcontainer.json` extends base
- `leaf/devcontainer.json` extends mid

Run from the leaf directory:

```
# Use workspace-folder discovery
(cd leaf && cargo run -p deacon -- read-configuration --workspace-folder "$(pwd)" | jq .)

# Or target the leaf config explicitly
cargo run -p deacon -- read-configuration --workspace-folder "$(pwd)/leaf" --config "$(pwd)/leaf/devcontainer.json" | jq .
```

What to look for:
- `configuration.remoteEnv` contains keys from BASE, MID, and LEAF
- `configuration.name` is `rc-extends-leaf`
