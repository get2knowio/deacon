# OCI Archive Export Example

Shows using `--output` to export the built image as an OCI archive.

## Usage
```sh
cd examples/build/output-archive
deacon build --workspace-folder . --image-name myorg/archive:exp \
  --output type=oci,dest=archive-image.tar --output-format json
```

Verify archive:
```sh
tar tf archive-image.tar | head
```

## Notes
- Demonstrates FR-002 & FR-004.
- Mutually exclusive with `--push`.