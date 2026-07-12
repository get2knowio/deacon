# Phase 1 Data Model: User-Scoped Profiles

All types live in `crates/core/src/settings.rs` (extending the existing `Settings`), except
where noted. Field names are the on-disk JSON keys (the user-hand-edited contract).

## `Settings` (extended)

The existing read-only settings struct gains three fields. `#[serde(default)]` + no
`deny_unknown_fields` preserves forward compatibility (FR-005/023); `#[serde(flatten) extra`
is added so unknown **top-level** keys round-trip (Constitution IV).

| Field (JSON key) | Type | Rules |
|---|---|---|
| `hostCa` | `Option<String>` | Existing. Root/base corporate-CA activation (`"auto"` or abs PEM path). |
| `browser` | `Option<String>` | Existing. Root/base browser-open program. |
| `mergeConfig` | `Option<MergeConfigPaths>` | **New.** Root-level universal override layer (precedence rung 2). Optional. |
| `profiles` | `IndexMap<String, Profile>` | **New.** `#[serde(default)]`. Declaration order preserved (FR-001). Empty ⇒ today's behavior (FR-008). |
| `defaultProfile` | `Option<String>` | **New.** Names the profile applied when none is selected (FR-004). Dangling ⇒ hard error (FR-016). |
| `extra` | `serde_json::Map<String, Value>` | **New.** `#[serde(flatten)]`. Unknown top-level keys, preserved verbatim. |

**Invariants**
- A missing file ⇒ `Settings::default()` (all `None`/empty) — non-error (FR-018).
- Present-but-corrupt file ⇒ error (unchanged from today).
- `profiles` is typed, so a non-object value fails serde deserialization (Constitution IV
  strict-on-mistakes — consistent with `features`/`customizations`).

## `Profile` (new)

A single entry in `profiles`. Open to unknown keys for future growth (FR-005): `mode`,
`startupCommand`, `restartPolicy`, per-profile mounts, etc. are **not** modeled now.

| Field (JSON key) | Type | Rules |
|---|---|---|
| `mergeConfig` | `Option<MergeConfigPaths>` | Fragments to layer (precedence rung 3). Optional. |
| `hostCa` | `Option<String>` | Overrides root `hostCa` when set; inherits root when unset (FR-013). |
| `browser` | `Option<String>` | Overrides root `browser` when set; inherits root when unset (FR-013). The reserved value `"none"` (case-insensitive) disables port auto-open (FR-013a). |
| `extra` | `serde_json::Map<String, Value>` | `#[serde(flatten)]`. Unknown/future profile keys, preserved. |

**Invariants**
- An "empty" profile (no `mergeConfig`, no scalar override) is **valid**; selecting it
  applies nothing (FR-009a). It is not an error.

## `MergeConfigPaths` (new)

Untagged single-or-list, mirroring `AppPort` (`config.rs:215`).

```rust
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum MergeConfigPaths {
    Single(PathBuf),
    Multiple(Vec<PathBuf>),
}

impl MergeConfigPaths {
    /// Ordered, low→high precedence (FR-012: later entries win).
    pub fn as_slice(&self) -> Cow<'_, [PathBuf]> { /* Single → one-elem, Multiple → borrow */ }
}
```

**Rules**: order is significant (FR-012). Paths resolve relative to the settings-file
directory; absolute paths accepted as-is (FR-021).

## `ResolvedSettings` (new, resolver output)

Produced by `Settings::resolve`. This is what the CLI consumes; it carries no raw profile
map — resolution and validation are already done.

| Field | Type | Meaning |
|---|---|---|
| `active_profile` | `Option<String>` | The applied profile name, or `None` (drives the FR-009b stderr diagnostic and SC-001/SC-007). |
| `merge_paths` | `Vec<PathBuf>` | Ordered, resolved, existence-checked merge fragments (rung 2 then rung 3). Lowest→highest. |
| `host_ca` | `Option<String>` | Effective (profile-else-root) value. |
| `browser` | `Option<String>` | Effective (profile-else-root) value. |

## `ProfileError` (new, `thiserror` in core)

| Variant | Fields | Trigger | FR |
|---|---|---|---|
| `UnknownProfile` | `name: String, available: Vec<String>` | `--profile`/`DEACON_PROFILE` or `defaultProfile` names an undefined profile. Message lists `available` in declaration order. | FR-015, FR-016 |
| `MissingFragment` | `profile: Option<String>, path: PathBuf` | A referenced `mergeConfig` path does not exist. `profile = None` for the root override, `Some(name)` for a profile. | FR-017 |

Mapped to `DeaconError`/`anyhow` at the binary boundary (Constitution V).

## Resolution algorithm (`Settings::resolve`)

Input: `selected: Option<&str>` (from `--profile`/`DEACON_PROFILE`), `settings_dir: &Path`.

1. `active = selected.or(default_profile.as_deref())`.
2. If `active = Some(name)` and `name ∉ profiles` → `UnknownProfile { name, available =
   profiles.keys() }`. (Covers both unknown `--profile` and dangling `defaultProfile`.)
3. `merge_paths = []`; for each source in **[root `mergeConfig`, active profile's
   `mergeConfig`]** (in that order): normalize to ordered list, resolve each vs
   `settings_dir` (or accept absolute), check existence → `MissingFragment` on miss, else
   push.
4. `host_ca = profile.host_ca.or(root.host_ca)`; `browser = profile.browser.or(root.browser)`.
   (When no active profile, both fall back to root — identical to today.) The resolved
   `browser` value is interpreted downstream by `resolve_browser`, where `"none"`
   (case-insensitive) disables auto-open (FR-013a) — the resolver passes the value through
   unchanged; the sentinel is honored at the browser-launch site.
5. Return `ResolvedSettings`.

## Precedence assembly (in `config_loader::load_config`)

> **Post-#285 update.** The base is the discovered config **or the
> `--override-config` file (REPLACE)**; the ordered merge fragments handed to
> `load_with_overrides_and_substitution` are the settings/profile `mergeConfig`
> plus the CLI `--merge-config` (highest).

Base = `--override-config` file (if given, replace) else discovered config; the
merge list handed to the loader (low→high):

```
[ ...ResolvedSettings.merge_paths ,  CLI --merge-config (if any) ]
```

so the layering is: base ⊕ root mergeConfig ⊕ profile mergeConfig ⊕ `--merge-config`
(FR-011). The existing last-writer-wins `merge_configs` yields the correct effective config;
no merge-logic change.

## Entity relationships

```
Settings 1─────* Profile          (profiles: IndexMap)
Settings 0/1─── MergeConfigPaths (root mergeConfig)
Profile  0/1─── MergeConfigPaths (profile mergeConfig)
Settings ──resolve(selected,dir)──> ResolvedSettings  (may raise ProfileError)
ResolvedSettings.merge_paths ──(+ CLI --merge-config)──> ordered chain ──> merge_configs
```

## No state transitions

The settings file is read-only in this feature (FR-022). No lifecycle/state machine; every
invocation reads and resolves fresh.
