# Data Model — Features Package

This model describes entities and validations for the `features package` subcommand.

## Entities

### Feature
- id: string (folder name or `devcontainer-feature.json` `id`)
- version: string (from `devcontainer-feature.json`)
- name: string (optional; from metadata)
- description: string (optional; from metadata)
- options: object (optional; from metadata)
- installsAfter: string[] (optional)
- dependsOn: string[] (optional)
- path: string (absolute path to feature root)

### Collection
- features: Feature[] (≥1 in valid collection)
- sourcePath: string (root path containing `src/`)

### OutputFolder
- path: string (absolute path)
- forceClean: boolean

### CollectionMetadata
- sourceInformation:
  - source: "devcontainer-cli"
- features: Array<FeatureDescriptor>

### FeatureDescriptor (for metadata JSON)
- id: string
- version: string
- name?: string
- description?: string
- options?: object
- installsAfter?: string[]
- dependsOn?: string[]

## Relationships
- Collection 1..1 → OutputFolder 1..1 (destination for artifacts and metadata)
- Collection 1..n → Feature (packaged individually)

## Validation Rules
1) Single Feature Mode
   - `devcontainer-feature.json` MUST exist at target root
   - `version` MUST be present and valid semver (string parsing; exact semver enforcement may be deferred if upstream allows relaxed versions)
2) Collection Mode
   - `src/` MUST exist under target
   - Each candidate subfolder under `src/` MUST contain a valid `devcontainer-feature.json`
   - If any subfolder invalid → entire run fails; list invalids; no artifacts produced
3) Output Folder
   - If `--force-clean-output-folder` true → safely empty folder before writes
   - Otherwise create folder if missing and overwrite existing files deterministically

## State Transitions
```
START
  └─ detect_mode(target)
       ├─ SINGLE → validate_single → package_feature → write_collection_metadata → DONE
       └─ COLLECTION → enumerate_src → validate_all
             ├─ any_invalid → FAIL (list invalid)
             └─ all_valid → package_each → write_collection_metadata → DONE
```

## Error Modes (Non-exhaustive)
- Missing `devcontainer-feature.json` in single mode → InvalidSingleFeature
- `src/` missing or empty → EmptyCollection
- Mixed valid/invalid under `src/` → MixedCollectionInvalid
- Filesystem write error → IoError (with path context)
