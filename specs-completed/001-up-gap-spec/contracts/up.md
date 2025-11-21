# Contract: `deacon up`

Treat the CLI invocation as an API contract. Exactly one JSON document MUST be written to stdout; logs go to stderr.

## Operation
- **Name**: up
- **Style**: CLI wrapper (conceptual API: `POST /up`)
- **Request Body (conceptual)**:
  - workspaceFolder?: string (path)
  - config?: string (path)
  - overrideConfig?: string (path)
  - idLabel[]?: string (name=value)
  - mountWorkspaceGitRoot?: bool (default true)
  - terminalColumns?: u32 (requires terminalRows)
  - terminalRows?: u32 (requires terminalColumns)
  - removeExistingContainer?: bool
  - buildNoCache?: bool
  - expectExistingContainer?: bool
  - workspaceMountConsistency?: enum ["consistent","cached","delegated"]
  - gpuAvailability?: enum ["all","detect","none"]
  - defaultUserEnvProbe?: enum ["none","loginInteractiveShell","interactiveShell","loginShell"]
  - updateRemoteUserUidDefault?: enum ["never","on","off"]
  - mount[]?: string (type=<bind|volume>,source=<...>,target=<...>[,external=<true|false>])
  - remoteEnv[]?: string (NAME=VALUE)
  - cacheFrom[]?: string
  - cacheTo?: string
  - buildkit?: enum ["auto","never"]
  - additionalFeatures?: json
  - skipFeatureAutoMapping?: bool
  - dotfilesRepository?: string
  - dotfilesInstallCommand?: string
  - dotfilesTargetPath?: string
  - containerSessionDataFolder?: string
  - userDataFolder?: string
  - containerDataFolder?: string
  - containerSystemDataFolder?: string
  - omitConfigRemoteEnvFromMetadata?: bool
  - omitSyntaxDirective?: bool
  - includeConfiguration?: bool
  - includeMergedConfiguration?: bool
  - secretsFile[]?: string (path)
  - dockerPath?: string
  - dockerComposePath?: string
  - lifecycle control: skipPostCreate?, skipPostAttach?, prebuild?, skipNonBlocking?

## Success Response (stdout)
```json
{
  "outcome": "success",
  "containerId": "string",
  "composeProjectName": "string (optional)",
  "remoteUser": "string",
  "remoteWorkspaceFolder": "string",
  "configuration": { "object when includeConfiguration" },
  "mergedConfiguration": { "object when includeMergedConfiguration" }
}
```
- **Exit code**: 0

## Error Response (stdout)
```json
{
  "outcome": "error",
  "message": "string",
  "description": "string",
  "containerId": "string (optional)",
  "disallowedFeatureId": "string (optional)",
  "didStopContainer": true,
  "learnMoreUrl": "string (optional)"
}
```
- **Exit code**: 1

## Validation Rules
- Require workspaceFolder or idLabel; require workspaceFolder or overrideConfig.
- mount must match regex `type=(bind|volume),source=([^,]+),target=([^,]+)(,external=(true|false))?`.
- remoteEnv must match `<name>=<value>`.
- terminalColumns implies terminalRows and vice versa.
- expectExistingContainer errors if target container not found.

## Side Effects Guardrails
- No docker/compose/build operations start until validation passes.
- Secrets values must be redacted from stderr logs and never appear in stdout JSON.

## Examples
- **Happy path**:
  - Request: workspaceFolder="/repo", includeConfiguration=true
  - Response: success JSON with containerId, remoteUser, remoteWorkspaceFolder, configuration blob.
- **Invalid mount**:
  - Request: mount="type=bind,source=/tmp" (missing target)
  - Response: error JSON with message describing invalid mount format; exit 1; no runtime calls made.
