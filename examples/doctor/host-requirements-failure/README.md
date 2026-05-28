# Doctor: Failing Host Requirements

`hostRequirements` lets a config declare minimum CPU/memory/storage the
host must offer before `deacon up` will proceed. `doctor` checks the
host against those requirements and reports per-resource pass/fail.

This example wires absurd minimums (1024 CPUs, 9999 GB RAM, 9999 TB
storage) so the check is guaranteed to fail. It pairs with
`doctor/host-requirements/` (which uses realistic values and passes).

## Files

- `.devcontainer/devcontainer.json` — `hostRequirements` set high enough
  to never pass on a real machine.

## Scenarios exercised by `exec.sh`

1. **Text mode.** `deacon doctor` reports the unmet requirements and
   exits non-zero. The output mentions CPU, memory, and storage.
2. **JSON mode.** `deacon doctor --json` returns structured output with
   each resource's status. We parse it and assert each entry is marked
   failed.
3. **`--ignore-host-requirements`** (if available on doctor or up) is
   noted in the README as the documented escape hatch.

## Manual usage

```sh
deacon doctor --workspace-folder .                 # non-zero exit, text report
deacon doctor --workspace-folder . --json | jq '.host_requirements'
```

## Spec references

- `hostRequirements`: <https://containers.dev/implementors/json_reference/>
- GPU shape (`true` | `"optional"` | `{cores, memory}`):
  <https://github.com/devcontainers/spec/blob/main/docs/specs/devcontainer-reference.md>
  (not exercised here — see a follow-up example).
