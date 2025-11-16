# Multi Tags & Labels Build Example

Demonstrates applying multiple `--image-name` tags and custom `--label` values during `deacon build`.

## Usage

```sh
cd examples/build/multi-tags-and-labels
deacon build --workspace-folder . \
  --image-name myorg/multi:dev \
  --image-name myorg/multi:latest \
  --label env=dev \
  --label team=platform \
  --output-format json
```

Expected: JSON success payload listing both tags; resulting image has devcontainer metadata label plus `env` and `team`.

## Notes
- Demonstrates FR-001 and FR-005.
- Add additional labels with repeated `--label` flags.