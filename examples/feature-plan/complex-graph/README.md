# Features Plan — Complex Graph

What this demonstrates

- A more complex graph using `dependsOn` and `installsAfter` together.
- Lexicographic tie-breakers for independent features.

Graph (conceptual)

```
feature-core -> feature-plugin-1
feature-core -> feature-plugin-2
feature-plugin-1 -> feature-extension
feature-plugin-2 -> feature-extension
feature-aux (independent)
```

How to run

```sh
deacon features plan --config devcontainer.json --json
```

This example uses local feature files so it runs offline.
