# Compose Multi-service Configuration

Demonstrates using a devcontainer with Docker Compose across multiple services: an app plus Postgres and Redis.

What it shows:
- `dockerComposeFile`, `service`, and `runServices`
- Port forwarding with labels and behaviors
- Security options warning example (privileged, capAdd, securityOpt)

Run (from this directory):
```sh
deacon config validate .
```

Optional: bring the environment up (requires Docker running):
```sh
deacon up . --skip-post-create
```