# `containerEnv` Pass-Through

Demonstrates that `containerEnv` is preserved verbatim in `read-configuration`
output — it is **not** a legacy alias of `remoteEnv`. The two fields are
distinct per the upstream [containers.dev spec](https://containers.dev/implementors/json_reference/):

- `containerEnv` — set on the container at run time (image-level env).
- `remoteEnv` — injected into the user's shell session in the running
  container (process-level env).

Run from this directory:

```
cargo run -p deacon -- read-configuration --workspace-folder "$(pwd)" | jq .
```

What to look for:
- `configuration.containerEnv` is `{ "FOO": "bar" }` (kept intact).
- `configuration.remoteEnv` is `{}` (empty — `containerEnv` is NOT folded into it).
