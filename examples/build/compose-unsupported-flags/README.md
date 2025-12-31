# Compose Unsupported Flags Example

Shows validation errors for flags not supported in Compose build context (`--push`, `--output`, `--platform`, `--cache-to`).

## Usage (Expect Errors)
```sh
cd examples/build/compose-unsupported-flags
deacon build --workspace-folder . --push --image-name myorg/compose-error:latest
```
```sh
deacon build --workspace-folder . --output type=oci,dest=out.tar --image-name myorg/compose-error:latest
```

## Notes
- Demonstrates FR-006 & FR-010 (pre-build rejection).