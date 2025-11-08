# Features Plan — Cycle Detection

What this demonstrates

- A small cycle in `dependsOn` to show the resolver error path.

Graph created

```
feature-x -> feature-y -> feature-z -> feature-x (cycle)
```

How to run

```sh
deacon features plan --config devcontainer.json --json
```

Expected result

- The command should exit with an error showing the cycle chain.
