# Data Model: Complete Feature Support During Up Command

**Feature**: 009-complete-feature-support
**Date**: 2025-12-28

## Overview

This document defines the data structures and relationships for implementing complete feature support during the `deacon up` command.

---

## Entity Definitions

### FeatureRefType (NEW)

Represents the type of feature reference used in devcontainer.json.

```rust
/// Discriminated union for feature reference types
#[derive(Debug, Clone, PartialEq)]
pub enum FeatureRefType {
    /// OCI registry reference: ghcr.io/devcontainers/features/node:18
    Oci(OciFeatureRef),
    /// Local path reference: ./local-feature, ../shared-feature
    LocalPath(PathBuf),
    /// HTTPS tarball URL: https://example.com/feature.tgz
    HttpsTarball(Url),
}

/// Parsed OCI feature reference components
#[derive(Debug, Clone, PartialEq)]
pub struct OciFeatureRef {
    pub registry: String,      // e.g., "ghcr.io"
    pub namespace: String,     // e.g., "devcontainers/features"
    pub name: String,          // e.g., "node"
    pub tag: Option<String>,   // e.g., "18" or None for "latest"
}
```

**Validation Rules**:
- Local paths MUST start with `./` or `../`
- HTTPS URLs MUST start with `https://`
- OCI references MUST NOT start with `./`, `../`, or `https://`

**State Transitions**: N/A (immutable after parsing)

---

### FeatureMetadata (EXISTING - no changes needed)

Parsed content of `devcontainer-feature.json`. Already contains all required fields:

```rust
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FeatureMetadata {
    pub id: String,
    pub version: Option<String>,
    pub name: Option<String>,
    pub description: Option<String>,

    // Security options (used by this feature)
    pub privileged: Option<bool>,
    pub init: Option<bool>,
    #[serde(default)]
    pub cap_add: Vec<String>,
    #[serde(default)]
    pub security_opt: Vec<String>,

    // Container configuration (used by this feature)
    #[serde(default)]
    pub mounts: Vec<String>,
    pub entrypoint: Option<String>,
    #[serde(default)]
    pub container_env: HashMap<String, String>,

    // Lifecycle commands (used by this feature)
    pub on_create_command: Option<serde_json::Value>,
    pub update_content_command: Option<serde_json::Value>,
    pub post_create_command: Option<serde_json::Value>,
    pub post_start_command: Option<serde_json::Value>,
    pub post_attach_command: Option<serde_json::Value>,

    // Dependency resolution
    #[serde(default)]
    pub installs_after: Vec<String>,
    #[serde(default)]
    pub depends_on: HashMap<String, serde_json::Value>,

    // Options schema
    #[serde(default)]
    pub options: HashMap<String, FeatureOption>,
}
```

---

### ResolvedFeature (EXISTING - no changes needed)

A feature that has been fetched and parsed. Already contains all required data:

```rust
#[derive(Debug, Clone)]
pub struct ResolvedFeature {
    /// Canonical feature ID (e.g., "ghcr.io/devcontainers/features/node")
    pub id: String,
    /// Original source reference from config
    pub source: String,
    /// User-provided option values
    pub options: HashMap<String, OptionValue>,
    /// Parsed feature metadata
    pub metadata: FeatureMetadata,
}
```

---

### MergedSecurityOptions (NEW)

Combined security options from config and all features.

```rust
/// Security options merged from config and all resolved features
#[derive(Debug, Clone, Default)]
pub struct MergedSecurityOptions {
    /// True if ANY source declares privileged (OR logic)
    pub privileged: bool,
    /// True if ANY source declares init (OR logic)
    pub init: bool,
    /// Union of all capabilities, deduplicated and uppercase-normalized
    pub cap_add: Vec<String>,
    /// Union of all security options, deduplicated
    pub security_opt: Vec<String>,
}
```

**Merge Algorithm**:
```
privileged = config.privileged OR feature1.privileged OR feature2.privileged OR ...
init = config.init OR feature1.init OR feature2.init OR ...
cap_add = DEDUPLICATE(UPPERCASE(config.cap_add + feature1.cap_add + feature2.cap_add + ...))
security_opt = DEDUPLICATE(config.security_opt + feature1.security_opt + feature2.security_opt + ...)
```

---

### LifecycleCommandSource (NEW)

Tracks the origin of a lifecycle command for error reporting.

```rust
/// Source attribution for a lifecycle command
#[derive(Debug, Clone)]
pub enum LifecycleCommandSource {
    /// Command from a feature (includes feature ID for attribution)
    Feature { id: String },
    /// Command from devcontainer.json config
    Config,
}

impl std::fmt::Display for LifecycleCommandSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Feature { id } => write!(f, "feature:{}", id),
            Self::Config => write!(f, "config"),
        }
    }
}
```

---

### AggregatedLifecycleCommand (NEW)

A lifecycle command with its source attribution.

```rust
/// A lifecycle command ready for execution with source tracking
#[derive(Debug, Clone)]
pub struct AggregatedLifecycleCommand {
    /// The command to execute (can be string, array, or object)
    pub command: serde_json::Value,
    /// Where this command came from
    pub source: LifecycleCommandSource,
}
```

---

### LifecycleCommandList (NEW)

Ordered list of lifecycle commands for a specific phase.

```rust
/// Ordered commands for a lifecycle phase
/// Feature commands come first (in installation order), then config command
#[derive(Debug, Clone, Default)]
pub struct LifecycleCommandList {
    pub commands: Vec<AggregatedLifecycleCommand>,
}

impl LifecycleCommandList {
    /// Filter out empty/null commands
    pub fn filter_empty(self) -> Self {
        Self {
            commands: self.commands.into_iter()
                .filter(|cmd| !is_empty_command(&cmd.command))
                .collect()
        }
    }
}
```

---

### MergedMounts (NEW)

Merged mounts from config and features with precedence handling.

```rust
/// Mounts merged from features and config
/// Config mounts take precedence for same target path
#[derive(Debug, Clone, Default)]
pub struct MergedMounts {
    /// Final mount strings to apply (deduplicated by target)
    pub mounts: Vec<String>,
}
```

---

### EntrypointChain (NEW)

Chained entrypoints from multiple features.

```rust
/// Entrypoint configuration after chaining feature entrypoints
#[derive(Debug, Clone)]
pub enum EntrypointChain {
    /// No entrypoint specified
    None,
    /// Single entrypoint (no chaining needed)
    Single(String),
    /// Multiple entrypoints requiring wrapper script
    Chained {
        /// Path to generated wrapper script in container
        wrapper_path: String,
        /// Original entrypoints in order
        entrypoints: Vec<String>,
    },
}
```

---

### FeatureBuildOutput (EXISTING - extend)

Output from the feature build phase. Needs extension to include new merged data:

```rust
/// Output from building features into the container image
#[derive(Debug, Clone)]
pub struct FeatureBuildOutput {
    /// Extended image tag with features installed
    pub image_tag: String,
    /// Combined environment variables from all features
    pub combined_env: HashMap<String, String>,
    /// Resolved features in installation order
    pub resolved_features: Vec<ResolvedFeature>,

    // NEW: Additional merged configuration
    /// Merged security options from all features
    pub merged_security: MergedSecurityOptions,
    /// Merged mounts from all features
    pub merged_mounts: MergedMounts,
    /// Chained entrypoints from all features
    pub entrypoint_chain: EntrypointChain,
}
```

---

## Relationships

```
devcontainer.json
    │
    ├─────────────────────────────────────────┐
    │ features: {                              │
    │   "ghcr.io/.../node:18": {...},         │──▶ FeatureRefType::Oci
    │   "./local-feature": {...},             │──▶ FeatureRefType::LocalPath
    │   "https://example.com/f.tgz": {...}    │──▶ FeatureRefType::HttpsTarball
    │ }                                        │
    └─────────────────────────────────────────┘
                    │
                    ▼
            ResolvedFeature[]  (in installation order)
                    │
    ┌───────────────┼───────────────┬───────────────┐
    │               │               │               │
    ▼               ▼               ▼               ▼
FeatureMetadata  FeatureMetadata  FeatureMetadata  DevContainerConfig
    │               │               │               │
    └───────────────┴───────────────┴───────────────┘
                            │
            ┌───────────────┼───────────────┬───────────────┐
            │               │               │               │
            ▼               ▼               ▼               ▼
    MergedSecurityOptions  LifecycleCommandList  MergedMounts  EntrypointChain
            │               │               │               │
            └───────────────┴───────────────┴───────────────┘
                                    │
                                    ▼
                            FeatureBuildOutput
                                    │
                                    ▼
                        Container Creation + Lifecycle Execution
```

---

## Validation Rules

### FeatureRefType Validation

| Rule | Check | Error |
|------|-------|-------|
| Local path exists | `path.exists()` | "Local feature not found: {path}" |
| Local path has metadata | `path/devcontainer-feature.json exists` | "Missing devcontainer-feature.json in: {path}" |
| HTTPS URL valid | `Url::parse()` succeeds | "Invalid HTTPS URL: {url}" |
| HTTPS URL is HTTPS | `url.scheme() == "https"` | "HTTP not supported, use HTTPS: {url}" |
| OCI reference valid | Existing `parse_registry_reference()` | "Invalid OCI reference: {ref}" |

### MergedSecurityOptions Validation

| Rule | Check | Error |
|------|-------|-------|
| cap_add values uppercase | Normalized during merge | N/A (auto-normalized) |
| cap_add values valid | Validated by Docker at runtime | Docker error propagated |
| security_opt values valid | Validated by Docker at runtime | Docker error propagated |

### MergedMounts Validation

| Rule | Check | Error |
|------|-------|-------|
| Mount string parseable | `MountParser::parse_mount()` | "Invalid mount in feature {id}: {mount}" |
| Mount target non-empty | Validated during parse | "Mount target cannot be empty" |

---

## State Transitions

### Feature Resolution Flow

```
UNPARSED                 → PARSED                    → RESOLVED
(reference string)         (FeatureRefType)           (ResolvedFeature)
     │                          │                          │
     │ parse_reference()        │ fetch_feature()          │ resolve_dependencies()
     ▼                          ▼                          ▼
"ghcr.io/.../node:18"    Oci{...}                   ResolvedFeature{
                                                      id: "...",
                                                      metadata: {...},
                                                      options: {...}
                                                    }
```

### Security Options Merge Flow

```
FEATURE_SECURITY[]  +  CONFIG_SECURITY  →  MERGED_SECURITY
     │                      │                    │
     │ collect from         │ read from          │ apply to
     │ resolved_features    │ config             │ container creation
     ▼                      ▼                    ▼
[privileged: true]    [privileged: false]   MergedSecurityOptions{
[cap_add: ["NET_ADMIN"]]                       privileged: true,
                                               cap_add: ["NET_ADMIN"],
                                               ...
                                            }
```

### Lifecycle Command Aggregation Flow

```
FEATURE_COMMANDS[]  +  CONFIG_COMMAND  →  COMMAND_LIST  →  EXECUTED
     │                      │                 │              │
     │ in installation      │ last            │ sequentially │ fail-fast
     │ order                │                 │              │
     ▼                      ▼                 ▼              ▼
[feature1.onCreate]   config.onCreate   LifecycleCommandList{  exit 0
[feature2.onCreate]                       [f1.cmd, f2.cmd,     or
                                           config.cmd]        exit 1
                                        }
```

---

## Serialization Notes

### JSON Input (devcontainer-feature.json)

Uses `serde(rename_all = "camelCase")` for spec compliance:
- `privileged`, `init`, `capAdd`, `securityOpt`
- `onCreateCommand`, `postCreateCommand`, etc.

### Internal Representation

Uses Rust naming conventions (snake_case) via serde rename.

### Docker CLI Output

Security options are passed as CLI flags:
- `--privileged`
- `--init`
- `--cap-add=CAPABILITY`
- `--security-opt=option`

Mounts use Docker's mount syntax:
- `--mount type=bind,source=/host,target=/container`
