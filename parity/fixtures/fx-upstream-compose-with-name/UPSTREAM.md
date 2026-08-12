Vendored from [devcontainers/cli](https://github.com/devcontainers/cli) (MIT) at tag
`v0.87.0`, source path `src/test/configs/compose-with-name`.

Upstream's `src/test/cli.up.test.ts` drives this config from one test, *"for minimal
docker-compose with custom project name"*, which brings the workspace up and asserts the
project name the CLI chose:

```ts
assert.equal(upResult!.outcome, 'success');
assert.equal(upResult!.composeProjectName, 'custom-project-name');
```

The whole fixture is the compose file's top-level `name:` key. It is the one input that
overrides a CLI's own project-name derivation, which is why this fixture and its two
siblings are the only place deacon's deacon-namespaced default (`bhv-compose-project-name-robust`,
issues #265/#564) is expected NOT to apply — an explicitly authored name is a user
decision, and both CLIs honour it verbatim.

Adapted for this suite:

- **Base image repinned** from `ubuntu:latest` to `debian:bookworm-slim`, matching every
  other vendored fixture here: a floating `latest` tag makes the fixture's meaning move
  under it, and `debian:bookworm-slim` is the image the suite already pre-pulls.
- **`version: '3.8'` dropped.** Compose v2 ignores the key and warns that it is obsolete
  on every invocation; both CLIs would emit the same warning, so keeping it would add
  noise to `chan-stderr` without adding a claim.
- **`--docker-compose-path trigger-compose-v2` dropped.** Upstream passes it to force its
  test harness down the Compose v2 code path; it names a shim that only exists in that
  repo's test tree, and both CLIs use Compose v2 here anyway.
