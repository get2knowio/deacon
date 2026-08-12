Vendored from [devcontainers/cli](https://github.com/devcontainers/cli) (MIT) at tag
`v0.87.0`, source path `src/test/configs/compose-with-name-and-custom-yaml`.

Upstream's `src/test/cli.up.test.ts` drives this config from one test, *"for minimal
docker-compose with custom project name and custom yaml"*, asserting
`composeProjectName === 'custom-project-name-custom-yaml'`.

What makes it a DISTINCT fixture from its `compose-with-name` sibling is the one extra
line, `ports: !reset []`. `!reset` is a Compose-specific YAML tag (a *custom* tag, not
standard YAML), so a compose document carrying it cannot be read by a generic YAML parser
that does not know the tag — which is exactly the point of the test. A CLI that recovers
the project name by parsing the compose file itself has to survive a document it cannot
fully understand; a CLI that asks Compose (`docker compose config`) never sees the tag.
Both CLIs choose a strategy that survives it, but for different reasons — the reference
delegates to Compose, deacon reads the top-level `name:` line without parsing the rest —
and this fixture is what keeps that observable.

Adapted for this suite:

- **Base image repinned** from `ubuntu:latest` to `debian:bookworm-slim` — same reason as
  the sibling fixture.
- **`version: '3.8'` dropped** — obsolete in Compose v2, warns on every invocation,
  changes nothing.
- **`--docker-compose-path trigger-compose-v2` dropped** — an upstream test-harness shim.

`ports: !reset []` itself is NOT an adaptation and must not be "cleaned up": it is the
entire reason this fixture exists alongside `fx-upstream-compose-with-name`.
