# Data Model: Env-Probe Cache

**Feature**: 001-010-env-probe  
**Date**: 2025-11-23

## Overview

This document defines the data structures and relationships for the env-probe caching feature. The caching system stores container environment variables on disk to optimize repeated `deacon up` invocations.

---

## Core Entities

### 1. CacheKey

**Description**: Composite identifier uniquely identifying a cache entry by container and user.

**Type**: `String` (generated, not stored as struct)

**Format**: `{container_id}_{user}`

**Generation Logic**:
```rust
let cache_key = format!("{}_{}", container_id, user.unwrap_or("root"));
```

**Examples**:
- `abc123def456_vscode` - Container abc123def456, user vscode
- `def789ghi012_root` - Container def789ghi012, root user
- `123abc456def_alice` - Container 123abc456def, user alice

**Validation Rules**:
- Container ID must be non-empty (enforced by `probe_container_environment`)
- User is optional; defaults to "root" if None
- No whitespace validation needed (container IDs are hex strings, usernames are alphanumeric)

**Uniqueness**: Guaranteed by Docker container ID uniqueness + user namespace isolation

---

### 2. CacheFile

**Description**: JSON file on disk containing serialized environment variables.

**Location Pattern**: `{cache_folder}/env_probe_{cache_key}.json`

**Schema**: JSON object with string keys and string values

**Rust Representation**: `HashMap<String, String>`

**Example Content**:
```json
{
  "PATH": "/usr/local/bin:/usr/bin:/bin:/usr/local/sbin:/usr/sbin",
  "HOME": "/home/vscode",
  "SHELL": "/bin/bash",
  "USER": "vscode",
  "LANG": "en_US.UTF-8",
  "NVM_DIR": "/home/vscode/.nvm",
  "NODE_VERSION": "18.16.0"
}
```

**Size Constraints**:
- Typical size: 1-5 KB (50-200 environment variables)
- Max size: No enforced limit (bounded by container env size, typically <50KB)

**Lifecycle**:
1. **Created**: When first probe executes with cache_folder provided
2. **Read**: On subsequent probes with matching container_id + user
3. **Updated**: Never (cache is immutable once written; container rebuild creates new cache)
4. **Deleted**: Manual deletion by user OR when cache folder is cleaned

**Permissions**: Inherits from OS filesystem (typically 0644 or 0600)

---

### 3. ContainerProbeResult

**Description**: Result of container environment probe (either from cache or fresh execution).

**Rust Definition** (existing in `crates/core/src/container_env_probe.rs`):
```rust
#[derive(Debug, Clone)]
pub struct ContainerProbeResult {
    /// Environment variables captured from container shell or cache
    pub env_vars: HashMap<String, String>,
    /// Shell used for probing ("bash", "zsh", etc.) or "cache" if loaded
    pub shell_used: String,
    /// Number of variables captured
    pub var_count: usize,
}
```

**Fields**:

| Field | Type | Description | Source |
|-------|------|-------------|--------|
| `env_vars` | `HashMap<String, String>` | Environment variable key-value pairs | Cache file OR shell execution |
| `shell_used` | `String` | Shell used for probing | `/bin/bash`, `/bin/zsh`, or `"cache"` |
| `var_count` | `usize` | Number of variables captured | `env_vars.len()` |

**Invariants**:
- `var_count` MUST equal `env_vars.len()`
- `shell_used` = `"cache"` implies data loaded from cache file
- `shell_used` = shell path implies fresh probe executed

**Example Values**:

Fresh probe:
```rust
ContainerProbeResult {
    env_vars: HashMap::from([
        ("PATH".to_string(), "/usr/bin:/bin".to_string()),
        ("HOME".to_string(), "/root".to_string()),
    ]),
    shell_used: "/bin/bash".to_string(),
    var_count: 2,
}
```

Cache hit:
```rust
ContainerProbeResult {
    env_vars: HashMap::from([
        ("PATH".to_string(), "/usr/bin:/bin".to_string()),
        ("HOME".to_string(), "/root".to_string()),
    ]),
    shell_used: "cache".to_string(),  // Indicates loaded from disk
    var_count: 2,
}
```

---

### 4. ContainerLifecycleConfig (Extended)

**Description**: Configuration for container lifecycle command execution (existing struct, extended with cache_folder).

**Location**: `crates/core/src/container_lifecycle.rs`

**Relevant Fields** (cache-related only):
```rust
pub struct ContainerLifecycleConfig {
    // ... existing fields (container_id, user, workspace, env, etc.)
    
    /// Optional cache folder for env probe results
    pub cache_folder: Option<std::path::PathBuf>,
}
```

**Field Details**:

| Field | Type | Description | Default |
|-------|------|-------------|---------|
| `cache_folder` | `Option<PathBuf>` | Directory for cache files | `None` |

**Usage**:
- Passed to `resolve_env_and_user` when executing lifecycle commands
- Enables cache reuse across lifecycle phases (postCreate, postStart, etc.)
- None = no caching (always fresh probe)

---

## Data Relationships

```
┌─────────────────────────────────────────────────────────────┐
│ User invokes: deacon up --container-data-folder=/tmp/cache  │
└────────────────────┬────────────────────────────────────────┘
                     │
                     ▼
        ┌────────────────────────┐
        │  UpArgs                │
        │  cache_folder: Some(   │
        │    PathBuf("/tmp/cache")│
        │  )                     │
        └────────┬───────────────┘
                 │
                 │ Pass to
                 ▼
        ┌────────────────────────┐
        │ resolve_env_and_user() │
        │ (shared helper)        │
        └────────┬───────────────┘
                 │
                 │ Pass to
                 ▼
┌────────────────────────────────────────────┐
│ probe_container_environment()              │
│                                            │
│ Generate CacheKey:                         │
│   key = "abc123_vscode"                   │
│                                            │
│ Check CacheFile:                           │
│   path = "/tmp/cache/env_probe_abc123_vscode.json" │
└────────┬───────────────────────────────────┘
         │
         │
    ┌────┴──────┐
    │           │
    ▼           ▼
┌───────┐  ┌──────────┐
│ Exists│  │Not Exists│
└───┬───┘  └────┬─────┘
    │           │
    │           │ Execute shell probe
    │           ▼
    │      ┌──────────────────┐
    │      │ Container Shell  │
    │      │ $ /bin/bash -lc  │
    │      │ 'env'            │
    │      └────┬─────────────┘
    │           │
    │           │ Serialize
    │           ▼
    │      ┌──────────────────┐
    │      │ Write CacheFile  │
    │      └────┬─────────────┘
    │           │
    ▼           ▼
┌────────────────────────────┐
│ ContainerProbeResult       │
│                            │
│ env_vars: HashMap<K,V>     │
│ shell_used: "cache"/"bash" │
│ var_count: 150             │
└────────────────────────────┘
```

---

## State Transitions

### Cache File Lifecycle

```
[No Cache]
    │
    │ First probe with cache_folder
    ▼
[Cache Created] ────┐
    │              │ Read on subsequent probes
    │              │
    │◄─────────────┘
    │
    │ Container rebuild (ID changes)
    ▼
[Stale Cache] ───────────► [New Cache Created]
    │                           │
    │ Manual deletion           │
    ▼                           │
[No Cache] ◄────────────────────┘
```

**State Definitions**:

1. **No Cache**: Cache file doesn't exist yet
   - Trigger: First probe OR manual deletion
   - Action: Execute fresh probe + write cache

2. **Cache Created**: Valid cache file exists
   - Trigger: Successful probe + write
   - Action: Read cache on subsequent probes

3. **Stale Cache**: Cache exists but container ID changed
   - Trigger: Container rebuild
   - Action: Ignore old cache, create new cache with new container ID
   - Note: Old file remains on disk (manual cleanup needed)

---

## Serialization Format

### JSON Schema

**File**: `contracts/cache-schema.json`

```json
{
  "$schema": "http://json-schema.org/draft-07/schema#",
  "title": "EnvProbeCache",
  "description": "Container environment variables cache",
  "type": "object",
  "patternProperties": {
    "^[A-Z_][A-Z0-9_]*$": {
      "type": "string",
      "description": "Environment variable value"
    }
  },
  "additionalProperties": false,
  "examples": [
    {
      "PATH": "/usr/local/bin:/usr/bin:/bin",
      "HOME": "/home/vscode",
      "SHELL": "/bin/bash",
      "USER": "vscode"
    }
  ]
}
```

**Key Pattern**: Environment variable names (typically uppercase with underscores)
**Value Type**: Strings (all env vars are strings in shell)

### Rust Serialization

**Type**: `HashMap<String, String>`

**Serialization**:
```rust
let env_vars: HashMap<String, String> = /* ... */;
let json_content = serde_json::to_string(&env_vars)?;
std::fs::write(cache_path, json_content)?;
```

**Deserialization**:
```rust
let json_content = std::fs::read_to_string(cache_path)?;
let env_vars: HashMap<String, String> = serde_json::from_str(&json_content)?;
```

**Error Handling**:
- Serialization failure: Log warning, continue without caching
- Deserialization failure: Log warning, fall back to fresh probe

---

## Cache Key Collision Analysis

**Scenario**: Two containers with same ID + same user

**Likelihood**: Impossible - Docker container IDs are globally unique UUIDs

**Scenario**: User "alice" vs user "alice_admin"

**Likelihood**: Low - standard usernames don't contain underscores typically

**Mitigation**: None needed - underscore separator + container ID uniqueness provides sufficient isolation

**Future Enhancement**: If collision becomes issue, use hash-based key (e.g., SHA256 of container_id+user)

---

## Performance Characteristics

### Cache Hit

**Operation**: Read JSON file from disk
**Time Complexity**: O(n) where n = file size (typically <5KB)
**Expected Latency**: <10ms
**Memory**: O(n) to load HashMap

### Cache Miss

**Operation**: Execute shell probe + parse output + write cache
**Time Complexity**: O(m) where m = probe execution time (typically 100-500ms)
**Expected Latency**: 100-500ms (dominated by shell startup)
**Memory**: O(n) for env vars

### Cache Write

**Operation**: Serialize HashMap to JSON + write to disk
**Time Complexity**: O(n) where n = number of env vars
**Expected Latency**: <10ms
**Memory**: O(n) for serialization buffer

### Speedup Calculation

```
Without cache: 100-500ms (shell probe)
With cache: <10ms (file read)
Speedup: 10-50x faster (90-98% latency reduction)
```

Meets spec requirement: "50%+ faster" ✅

---

## Summary

**3 Core Entities**:
1. **CacheKey** - String identifier `{container_id}_{user}`
2. **CacheFile** - JSON file at `{cache_folder}/env_probe_{key}.json`
3. **ContainerProbeResult** - Struct containing env vars + metadata

**1 Extended Entity**:
4. **ContainerLifecycleConfig** - Added `cache_folder: Option<PathBuf>` field

**Key Properties**:
- Simple filesystem-based storage
- No time-based expiration (container ID change only)
- Best-effort caching (failures don't block operations)
- 10-50x performance improvement on cache hit

**Next Steps**: Implement struct initializer fixes + logging additions per plan.md
