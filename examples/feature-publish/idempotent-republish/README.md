# idempotent-republish

What this demonstrates
- Publishing the same version twice is a no-op for already-published tags.
- This example uses dry-run mode to show which tags would be published and which would be skipped.

Files
- `devcontainer-feature.json` - feature with version `3.0.0`

Commands
```sh
# First run (dry-run) should show tags to publish
deacon features publish . --namespace exampleorg/idempotent --registry ghcr.io --dry-run --progress json

# Second run should indicate `skippedTags` for tags that already exist
# (When using a live registry; dry-run demonstrates logic without network push.)
deacon features publish . --namespace exampleorg/idempotent --registry ghcr.io --dry-run --progress json
```
