# Read-Configuration: Named-Config Discovery

The spec defines three discovery locations for `devcontainer.json`:

1. `.devcontainer/devcontainer.json` (preferred default)
2. `.devcontainer.json` (legacy fallback at workspace root)
3. **`.devcontainer/<name>/devcontainer.json`** (named subdirectory)

The third form lets a workspace ship multiple named devcontainer
variants side by side — e.g., a `python` and a `rust` variant of the
same project — that the tool consumer picks between at apply time.

This example wires three configs:
- `.devcontainer/devcontainer.json` (the default)
- `.devcontainer/python/devcontainer.json`
- `.devcontainer/rust/devcontainer.json`

Each declares a distinct `name` and a `containerEnv.VARIANT` so the
resolved configuration is identifiable.

## Files

- `.devcontainer/devcontainer.json` — default.
- `.devcontainer/python/devcontainer.json` — Python named variant.
- `.devcontainer/rust/devcontainer.json` — Rust named variant.

## Scenarios exercised by `exec.sh`

1. **Default discovery.** `read-configuration --workspace-folder .`
   picks the top-level `devcontainer.json` (VARIANT=default).
2. **Named: python.** Pass the path explicitly via `--config
   .devcontainer/python/devcontainer.json`. VARIANT=python.
3. **Named: rust.** Same, pointing at the rust variant. VARIANT=rust.

## Manual usage

```sh
# Default config.
deacon read-configuration --workspace-folder . | jq '.configuration.name'
# "Default config (.devcontainer/devcontainer.json)"

# Named variant.
deacon read-configuration \
	--workspace-folder . \
	--config .devcontainer/python/devcontainer.json \
	| jq '.configuration.name'
# "Python variant"
```

## Spec references

- Configuration discovery locations:
  <https://github.com/devcontainers/spec/blob/main/docs/specs/devcontainer-reference.md>
