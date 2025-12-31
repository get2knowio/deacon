# Features: Minimal (Local)

Demonstrates including a local Feature and printing the features configuration.

Run from this directory:

```
# Show base configuration only
cargo run -p deacon -- read-configuration --workspace-folder "$(pwd)" | jq .

# Include computed featuresConfiguration
cargo run -p deacon -- read-configuration --workspace-folder "$(pwd)" \
  --include-features-configuration | jq .

# Optionally include mergedConfiguration (no container)
cargo run -p deacon -- read-configuration --workspace-folder "$(pwd)" \
  --include-merged-configuration | jq .
```

What to look for:
- `featuresConfiguration` includes a plan referencing `./minimal-feature`
- When merged: `mergedConfiguration` reflects any feature-derived metadata available
