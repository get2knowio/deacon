# Features Plan — Simple Chain

What this demonstrates

- A simple dependency chain using `installsAfter` to order features.

Graph

```
feature-a -> feature-b -> feature-c
```

How to run

```sh
# From this directory
deacon features plan --config devcontainer.json --json
```

Expected output (order)

```json
{ "order": ["feature-a", "feature-b", "feature-c"], "graph": {"feature-a": ["feature-b"], "feature-b": ["feature-c"], "feature-c": []} }
```
