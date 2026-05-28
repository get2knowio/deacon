# Duplicate Image Tags Normalization Example

Demonstrates that passing the same `--image-name` more than once is accepted and
normalized: Docker dedups repeated `-t` flags, and `deacon` collapses the
duplicate so the emitted `imageName` reports each tag once.

## Usage
```sh
cd examples/build/duplicate-tags
deacon build --workspace-folder . \
  --image-name myorg/dups:latest \
  --image-name myorg/dups:latest \
  --output-format json
```

## Expected
- Build succeeds.
- `imageName` is the single string `"myorg/dups:latest"` (not a duplicated array).

## Notes
- Exercises edge-case input normalization (duplicate tag de-duplication). This
  matches the reference CLI, which passes tags through to Docker without
  rejecting redundant duplicates.
