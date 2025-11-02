# invalid-version

What this demonstrates
- The publish command validates semantic version syntax and fails with exit code `1` for invalid versions.

Files
- `devcontainer-feature.json` — feature with an invalid version string `not-a-semver`

Commands
```sh
# Attempt publish (expected to error)
deacon features publish . --namespace exampleorg/invalid --registry ghcr.io --dry-run --progress json || echo "publish failed as expected"
```

Expected Result
- Command exits non-zero with an error about invalid semantic version.
