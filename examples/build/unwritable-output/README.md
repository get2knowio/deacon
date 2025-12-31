# Unwritable Output Destination Example

Shows fast-fail when `--output` destination is not writable.

## Setup & Usage (Expect Error)
```sh
cd examples/build/unwritable-output
mkdir -p readonly
chmod 500 readonly   # remove write for others
deacon build --workspace-folder . --image-name myorg/unwritable:latest \
  --output type=oci,dest=readonly/image.tar
```

## Notes
- Exercises edge case for FR-006 (validation & error messaging).