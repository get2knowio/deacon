# Compose

Demonstrates reading a configuration that references a Docker Compose file.

Run from this directory:

```
cargo run -p deacon -- read-configuration --workspace-folder "$(pwd)" | jq .
```

What to look for:
- `configuration.dockerComposeFile` points to `./docker-compose.yml`
- `configuration.service` is `app`
