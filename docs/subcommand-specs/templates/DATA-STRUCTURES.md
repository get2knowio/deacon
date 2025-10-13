# Templates Subcommand — Data Structures

## CLI Argument Structures

- ApplyArgs
  - `workspace_folder: string`
  - `template_id: string` (OCI ref)
  - `template_args: string` (JSON string; parsed to TemplateOptions)
  - `features: string` (JSON string; parsed to TemplateFeatureOption[])
  - `omit_paths?: string` (JSON string; parsed to string[])
  - `tmp_dir?: string`
  - `log_level: 'info' | 'debug' | 'trace'`

- PublishArgs
  - `target: string`
  - `registry: string` (e.g., `ghcr.io`)
  - `namespace: string` (e.g., `owner/repo`)
  - `log_level: 'info' | 'debug' | 'trace'`

- MetadataArgs
  - `template_id: string`
  - `log_level: 'info' | 'debug' | 'trace'`

- GenerateDocsArgs
  - `project_folder: string`
  - `github_owner?: string`
  - `github_repo?: string`
  - `log_level: 'info' | 'debug' | 'trace'`

## Template Metadata

- Template
  - `id: string`
  - `version?: string`
  - `name?: string`
  - `description?: string`
  - `documentationURL?: string`
  - `licenseURL?: string`
  - `featureIds?: string[]`
  - `options?: Record<string, TemplateOption>`
  - `platforms?: string[]`
  - `publisher?: string`
  - `keywords?: string[]`
  - `optionalPaths?: string[]`
  - `files: string[]` (added during packaging)

- TemplateOption (union)
  - Boolean option
    - `type: 'boolean'`
    - `default?: boolean`
    - `description?: string`
  - String option (enum)
    - `type: 'string'`
    - `enum?: string[]`
    - `default?: string`
    - `description?: string`
  - String option (with proposals)
    - `type: 'string'`
    - `proposals?: string[]`
    - `default?: string`
    - `description?: string`

## Apply-Time Selection Structures

- TemplateOptions
  - `Record<string, string>` — All values are strings; booleans are converted to string at defaulting time.

- TemplateFeatureOption
  - `id: string`
  - `options: Record<string, string | boolean | undefined>`

- SelectedTemplate
  - `id: string`
  - `options: TemplateOptions`
  - `features: TemplateFeatureOption[]`
  - `omitPaths: string[]`

## OCI Structures

- OCIRef
  - `registry: string` (e.g., `ghcr.io`)
  - `owner: string` (e.g., `devcontainers`)
  - `namespace: string` (e.g., `devcontainers/templates`)
  - `path: string` (e.g., `devcontainers/templates/name`)
  - `resource: string` (e.g., `ghcr.io/devcontainers/templates/name`)
  - `id: string` (e.g., `name`)
  - `version: string` (tag value or digest)
  - `tag?: string`
  - `digest?: string`

- OCICollectionRef
  - `registry: string`
  - `path: string` (e.g., `devcontainers/templates`)
  - `resource: string` (e.g., `ghcr.io/devcontainers/templates`)
  - `tag: 'latest'`
  - `version: 'latest'`

- OCIManifest
  - `digest?: string`
  - `schemaVersion: number`
  - `mediaType: string`
  - `config: { digest: string, mediaType: string, size: number }`
  - `layers: { mediaType: string, digest: string, size: number, annotations: { 'org.opencontainers.image.title': string } }[]`
  - `annotations?: { 'dev.containers.metadata'?: string, 'com.github.package.type'?: string }`

- ManifestContainer
  - `manifestObj: OCIManifest`
  - `manifestBuffer: Buffer`
  - `contentDigest: string` (canonical)
  - `canonicalId: string` (`<resource>@sha256:...`)

## Command Outputs

- ApplyOutput
  - JSON: `{ files: string[] }`

- PublishOutput
  - JSON: `{ [templateId: string]: { publishedTags?: string[], digest?: string, version?: string } }`

- MetadataOutput
  - JSON: `Template` or `{}`

## JSON Schemas (informal)

- ApplyOutput
```
type ApplyOutput = {
  files: string[]
}
```

- PublishOutput
```
type PublishEntry = {
  publishedTags?: string[]
  digest?: string
  version?: string
}
type PublishOutput = {
  [templateId: string]: PublishEntry
}
```

- TemplateOption
```
type TemplateOption =
  | { type: 'boolean', default?: boolean, description?: string }
  | { type: 'string', enum?: string[], default?: string, description?: string }
  | { type: 'string', proposals?: string[], default?: string, description?: string }
```

