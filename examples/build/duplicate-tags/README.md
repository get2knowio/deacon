# Duplicate Image Tags Validation Example

Demonstrates failure when the same tag is passed multiple times.

## Usage (Expect Error)
```sh
cd examples/build/duplicate-tags
deacon build --workspace-folder . \
  --image-name myorg/dups:latest \
  --image-name myorg/dups:latest
```

## Notes
- Exercises edge case validation (duplicate tag rejection).