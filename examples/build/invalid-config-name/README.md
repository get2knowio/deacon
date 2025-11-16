# Invalid Config Filename Example

Uses an incorrect config filename (`devcontainer_wrong.json`) to trigger validation failure.

## Usage (Expect Error)
```sh
cd examples/build/invalid-config-name
deacon build --workspace-folder . --image-name myorg/invalid-name:latest
```

## Notes
- Demonstrates FR-006 filename enforcement.