# Dockerfile Build With Local Feature

Demonstrates feature installation when building from a Dockerfile.

## Usage
```sh
cd examples/build/dockerfile-with-features
deacon build --workspace-folder . --image-name myorg/feat-dockerfile:latest --output-format json
```

After build, verify feature artifact:
```sh
docker run --rm myorg/feat-dockerfile:latest cat /hello.txt
```

## Notes
- Demonstrates FR-008 (feature install in Dockerfile mode).