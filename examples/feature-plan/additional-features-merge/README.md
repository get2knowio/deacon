# Features Plan — CLI Additional Features Merge

What this demonstrates

- How CLI `--additional-features` are merged with workspace features.
- Additive merge semantics: CLI additions appear alongside config features.

How to run

```sh
# Merge a CLI feature map (adds feature-cli)
deacon features plan --config devcontainer.json --additional-features '{"feature-cli": {"option": true}}' --json
```

Expected behavior

- The resulting plan includes `feature-cli` in the order/graph.
