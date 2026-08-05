# Features: Additional (CLI)

Inject a Feature at runtime using `--additional-features`.

Run from this directory:

```
# Include featuresConfiguration with an additional local feature
cargo run -p deacon -- read-configuration --workspace-folder "$(pwd)" \
  --include-features-configuration \
  --additional-features '{"./.devcontainer/extra-feature": {"flag": true}}' | jq .

# Optionally include mergedConfiguration as well
cargo run -p deacon -- read-configuration --workspace-folder "$(pwd)" \
  --include-merged-configuration \
  --additional-features '{"./.devcontainer/extra-feature": {}}' | jq .
```

What to look for:
- `featuresConfiguration` includes `./.devcontainer/extra-feature` with provided option values
