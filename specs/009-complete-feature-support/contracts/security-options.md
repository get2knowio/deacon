# Contract: Security Options Merging

**Feature**: 009-complete-feature-support
**Date**: 2025-12-28

## Purpose

Defines the contract for merging security options from devcontainer config and resolved features during container creation.

---

## Input Contract

### DevContainerConfig Security Fields

```rust
pub struct DevContainerConfig {
    pub privileged: Option<bool>,
    pub init: Option<bool>,
    pub cap_add: Vec<String>,
    pub security_opt: Vec<String>,
    // ... other fields
}
```

### FeatureMetadata Security Fields

```rust
pub struct FeatureMetadata {
    pub privileged: Option<bool>,
    pub init: Option<bool>,
    pub cap_add: Vec<String>,
    pub security_opt: Vec<String>,
    // ... other fields
}
```

---

## Output Contract

### MergedSecurityOptions

```rust
pub struct MergedSecurityOptions {
    pub privileged: bool,
    pub init: bool,
    pub cap_add: Vec<String>,
    pub security_opt: Vec<String>,
}
```

---

## Merge Rules

### Rule 1: Privileged Mode (OR Logic)

```
IF config.privileged == Some(true)
   OR any feature.metadata.privileged == Some(true)
THEN result.privileged = true
ELSE result.privileged = false
```

**Examples**:
| Config | Feature 1 | Feature 2 | Result |
|--------|-----------|-----------|--------|
| None | None | None | false |
| Some(false) | None | None | false |
| Some(true) | None | None | true |
| Some(false) | Some(true) | None | true |
| Some(false) | Some(false) | Some(true) | true |

### Rule 2: Init Mode (OR Logic)

Same logic as privileged mode.

### Rule 3: Capabilities (Union + Deduplicate + Uppercase)

```
result.cap_add = DEDUPLICATE(UPPERCASE(
    config.cap_add
    ++ feature1.cap_add
    ++ feature2.cap_add
    ++ ...
))
```

**Processing Steps**:
1. Collect all capability strings from config and features
2. Convert each to uppercase (e.g., "net_admin" → "NET_ADMIN")
3. Deduplicate (preserve first occurrence order)

**Examples**:
| Config | Feature 1 | Feature 2 | Result |
|--------|-----------|-----------|--------|
| [] | [] | [] | [] |
| ["SYS_PTRACE"] | [] | [] | ["SYS_PTRACE"] |
| ["SYS_PTRACE"] | ["sys_ptrace"] | [] | ["SYS_PTRACE"] |
| ["NET_ADMIN"] | ["SYS_PTRACE"] | [] | ["NET_ADMIN", "SYS_PTRACE"] |
| [] | ["net_admin", "sys_ptrace"] | ["NET_ADMIN"] | ["NET_ADMIN", "SYS_PTRACE"] |

### Rule 4: Security Options (Union + Deduplicate)

```
result.security_opt = DEDUPLICATE(
    config.security_opt
    ++ feature1.security_opt
    ++ feature2.security_opt
    ++ ...
)
```

**Processing Steps**:
1. Collect all security option strings from config and features
2. Deduplicate (preserve first occurrence order)
3. No case normalization (security options are case-sensitive)

**Examples**:
| Config | Feature 1 | Feature 2 | Result |
|--------|-----------|-----------|--------|
| [] | [] | [] | [] |
| ["seccomp:unconfined"] | [] | [] | ["seccomp:unconfined"] |
| ["seccomp:unconfined"] | ["seccomp:unconfined"] | [] | ["seccomp:unconfined"] |
| ["apparmor:unconfined"] | ["seccomp:unconfined"] | [] | ["apparmor:unconfined", "seccomp:unconfined"] |

---

## Function Signature

```rust
/// Merge security options from config and resolved features
///
/// # Arguments
/// * `config` - DevContainerConfig with user-specified security options
/// * `features` - Resolved features in installation order
///
/// # Returns
/// MergedSecurityOptions with combined security settings
///
/// # Algorithm
/// - privileged: OR of all sources
/// - init: OR of all sources
/// - cap_add: Union, deduplicated, uppercase-normalized
/// - security_opt: Union, deduplicated, case-preserved
pub fn merge_security_options(
    config: &DevContainerConfig,
    features: &[ResolvedFeature],
) -> MergedSecurityOptions;
```

---

## Docker CLI Mapping

The `MergedSecurityOptions` maps to Docker CLI flags:

| Field | Docker Flag | Example |
|-------|-------------|---------|
| `privileged: true` | `--privileged` | `docker create --privileged ...` |
| `init: true` | `--init` | `docker create --init ...` |
| `cap_add: ["SYS_PTRACE"]` | `--cap-add=SYS_PTRACE` | `docker create --cap-add=SYS_PTRACE ...` |
| `security_opt: ["seccomp:unconfined"]` | `--security-opt=seccomp:unconfined` | `docker create --security-opt=seccomp:unconfined ...` |

---

## Error Handling

No errors from merging itself. Docker validates actual values at runtime.

## Testing Requirements

1. **Unit Tests**: Test merge logic with various input combinations
2. **Integration Tests**: Verify Docker container has correct flags applied
3. **Edge Cases**: Empty inputs, duplicates, mixed case capabilities
