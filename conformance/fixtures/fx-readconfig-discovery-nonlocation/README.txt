A plain `devcontainer.json` at the workspace ROOT is deliberately NOT a discovery
location: the spec's locations are `.devcontainer/devcontainer.json`,
`.devcontainer.json`, and `.devcontainer/<folder>/devcontainer.json`. This fixture
carries only the non-location so discovery must fail.
