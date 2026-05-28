# Doctor: `hostRequirements.gpu` Shapes

The `hostRequirements.gpu` field is overloaded — the spec accepts three
distinct shapes:

| Shape                              | Meaning                                  |
|------------------------------------|------------------------------------------|
| `true`                             | GPU is required; doctor must fail if absent. |
| `false` *(default if unspecified)* | No GPU required.                         |
| `"optional"`                       | GPU is preferred but not required.       |
| `{ "cores": N, "memory": "Xgb" }`  | GPU required with explicit constraints.  |

Each shape exercises a different code path in the resolver and in
`doctor`'s reporting. This example provides three config files (one per
non-default shape) and drives `doctor` against each with `--config`.

## Files

- `gpu-true.json` — boolean required.
- `gpu-optional.json` — `"optional"` string.
- `gpu-object.json` — `{ cores, memory }` object.

## Scenarios exercised by `exec.sh`

1. **All three shapes parse.** `read-configuration` returns each
   shape with the spec-correct JSON type (`true`, `"optional"`,
   object).
2. **Doctor emits a GPU-aware report.** For each config, `doctor`
   produces output that references "gpu" in some form (text mode) or
   includes a `gpu` field (JSON mode).
3. **Behavior differentiation.** `gpu: true` and the object form fail
   on a host without a GPU; `gpu: "optional"` warns but does not fail.
   `exec.sh` notes the observed exit codes but doesn't hard-fail on
   them — exit-code policy can vary while the spec is still being
   nailed down across implementations.

## Manual usage

```sh
deacon read-configuration --config ./gpu-true.json | jq '.configuration.hostRequirements.gpu'
# true

deacon read-configuration --config ./gpu-optional.json | jq '.configuration.hostRequirements.gpu'
# "optional"

deacon read-configuration --config ./gpu-object.json | jq '.configuration.hostRequirements.gpu'
# { "cores": 2, "memory": "8gb" }

deacon doctor --config ./gpu-true.json
```

## Known deacon issues this example surfaces

- [#65](https://github.com/get2knowio/deacon/issues/65) — `--config` filename
  validation rejects `gpu-true.json` / `gpu-optional.json` / `gpu-object.json`
  even though the upstream `@devcontainers/cli` accepts any filename.
- [#66](https://github.com/get2knowio/deacon/issues/66) — `read-configuration`
  rejects `--config` alone, demanding `--workspace-folder`. The reference
  CLI doesn't require this.

## Spec references

- GPU host requirement:
  <https://github.com/devcontainers/spec/blob/main/docs/specs/gpu-host-requirement.md>
- General host requirements:
  <https://containers.dev/implementors/json_reference/>
