Vendored from [devcontainers/cli](https://github.com/devcontainers/cli) (MIT) at tag
`v0.87.0`, source path `src/test/configs/compose-with-name-using-env-var`.

Upstream's `src/test/cli.up.test.ts` drives this config from TWO tests. Both set
`CUSTOM_NAME` in the environment and assert the interpolated value comes back as the
project name:

```ts
env: { ...process.env, 'CUSTOM_NAME': 'custom-name-with-env-var' }
assert.equal(upResult!.composeProjectName, 'custom-name-with-env-var');
```

```ts
env: { ...process.env, 'CUSTOM_NAME': 'devcontainer' }
assert.equal(upResult!.composeProjectName, 'devcontainer');
```

The claim is that a compose file's top-level `name:` is **interpolated**, not read
literally — `name: ${CUSTOM_NAME}` is a template, and `${CUSTOM_NAME}` is not itself a
legal Compose project name (`must consist only of lowercase alphanumeric characters,
hyphens, and underscores`), so a CLI that takes the line at face value cannot even start
the project.

Adapted for this suite:

- **The variable is supplied by a sibling `.env` file rather than by process
  environment.** The parity runner inherits its own environment and has no per-operation
  `env` field; adding one would be machinery growth to chase a fixture. A `.env` beside
  the compose file is the substitute Compose itself documents: the project directory is
  the directory of the first compose file, and `--env-file` defaults to `.env` there.
  Verified at Compose v2 on this fixture — `docker compose -f .devcontainer/docker-compose.yml config`
  reports `name: custom-name-with-env-var`. The file deliberately does NOT set
  `COMPOSE_PROJECT_NAME`: both CLIs short-circuit on that variable before ever looking at
  the compose document (deacon in `derive_project_name`, the reference in its `Rp` project
  resolver), which would answer a different question.
- **Only the first of the two upstream tests is carried.** The second sets `CUSTOM_NAME`
  to the literal `devcontainer`, which exercises a branch specific to the reference's
  resolver (`if (i.name !== 'devcontainer')` — it re-reads the compose files to
  distinguish an AUTHORED `devcontainer` from Compose's own directory-derived default).
  deacon has no such branch and no such default, so the second test asks about the
  reference's internals rather than about a shared behavior.
- **Base image repinned** from `ubuntu:latest` to `debian:bookworm-slim`;
  **`version: '3.8'` dropped**; **`--docker-compose-path trigger-compose-v2` dropped** —
  same three reasons as the sibling fixtures.
