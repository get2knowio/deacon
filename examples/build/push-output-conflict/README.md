# --push and --output Conflict Example

Demonstrates validation error when `--push` is combined with `--output`.

## Usage (Expect Error)
```sh
cd examples/build/push-output-conflict
deacon build --workspace-folder . --image-name myorg/conflict:latest \
  --push --output type=oci,dest=img.tar
```

Expected: fast-fail with message "--push true cannot be used with --output." (FR-002/FR-006).