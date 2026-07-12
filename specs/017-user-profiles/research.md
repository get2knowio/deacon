# Phase 0 Research: User-Scoped Profiles

All Technical Context unknowns are resolved below. Each decision records what was chosen,
why, and the alternatives rejected. Decision 6 is a phased-implementation deferral tracked
per Constitution Principle I.

## Decision 1 — Ordered `profiles` map storage

**Decision**: Store `profiles` as `IndexMap<String, Profile>` (declaration order preserved).

**Rationale**: FR-001 requires preserving declared order (it drives the "available profiles"
list in fail-fast errors, which should read back in the order the user wrote them).
`indexmap` 2.x with the `serde` feature is already a `crates/core` dependency
(`crates/core/Cargo.toml:34`), so no new dependency. A `BTreeMap` would alphabetize (a
Constitution VI ordering violation); a `HashMap` would be nondeterministic.

**Alternatives rejected**: `BTreeMap` (reorders); `Vec<(String, Profile)>` (loses map
ergonomics and O(1) lookup by name for selection/validation).

## Decision 2 — `mergeConfig` shape: single-or-list

**Decision**: Model `mergeConfig` (both root-level and per-profile) as an untagged
single-or-list enum, mirroring the existing `AppPort` pattern (`crates/core/src/config.rs:215`):

```rust
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum MergeConfigPaths {
    Single(PathBuf),
    Multiple(Vec<PathBuf>),
}
```

with a helper that normalizes to an ordered `Vec<PathBuf>`.

**Rationale**: FR-002 requires accepting either a single path or an ordered list; the
codebase already uses this exact `#[serde(untagged)]` Single/Multiple idiom (`AppPort`,
`PortSpec`), so it is the idiomatic, consistent choice. FR-012 (later entries win) is
satisfied by preserving list order and pushing onto the merge chain in order.

**Alternatives rejected**: Always-a-list (rejects the ergonomic single-path form users
expect, and diverges from `AppPort`); a custom `Deserialize` impl (unnecessary — untagged
enum handles it).

## Decision 3 — Generalize the override input to an ordered path list

**Decision**: Change `ConfigLoader::load_with_overrides_and_substitution` from
`override_config_path: Option<&Path>` to an ordered `override_config_paths: &[&Path]`
(lowest→highest precedence), pushing each onto the extends-resolved chain in order. The
shared `config_loader::load_config` builds the final ordered list as
`[settings overrides…, CLI --override-config]` so the CLI override stays highest (FR-011).

**Rationale**: The precedence ladder in FR-011 is *n* override layers, not one. The existing
merge (`ConfigMerger::merge_configs`, last-writer-wins) already produces the correct result
for an ordered chain — so the cleanest, lowest-risk change is to feed it an ordered list
rather than invent new merge logic (Constitution I: no shortcut algorithms; VIII: shared
helper). Blast radius is tiny: the only production caller is `config_loader.rs:96`; the
other two call sites are tests (`config.rs:4016`, `integration_override_secrets.rs:62`).

**"Missing base config" fallback**: today, when the discovered base config does not exist,
`config_loader` promotes the single CLI override to the base (`config_loader.rs:84-88`). With
an ordered list, promote the **lowest-precedence** override path to base and stack the
remainder on top; if the list is empty, keep today's behavior.

**Alternatives rejected**: Adding a second `settings_override_paths` parameter alongside the
existing `Option<&Path>` (bloats the signature, duplicates the missing-base fallback across
two inputs, and leaves two ways to express "an override"). Building a pre-merged single
override file (loses per-layer provenance needed for Decision 6 and is opaque to debugging).

## Decision 4 — Selection resolution + fail-fast validation (pure, in core)

**Decision**: Add `Settings::resolve(selected: Option<&str>, settings_dir: &Path) ->
Result<ResolvedSettings, ProfileError>`:
1. Determine the active profile name: `selected` (from `--profile`/`DEACON_PROFILE`), else
   `default_profile`, else none (FR-007/008).
2. If a name is active but absent from `profiles` → `ProfileError::UnknownProfile { name,
   available }` (sorted-as-declared) — covers both an unknown `--profile` (FR-015) and a
   dangling `defaultProfile` (FR-016) with one error type.
3. Build the ordered override `Vec<PathBuf>`: root `override_config` (normalized) then the
   active profile's `override_config` (normalized). Resolve each relative to `settings_dir`;
   accept absolute paths as-is (FR-021). Validate existence → `ProfileError::MissingFragment
   { profile, path }` (FR-017).
4. Compute effective scalars: `host_ca`/`browser` = active profile value, else root value
   (FR-013). An empty/no-op profile yields no override paths and inherits root scalars
   (FR-009a).

`ResolvedSettings { active_profile: Option<String>, override_paths: Vec<PathBuf>, host_ca:
Option<String>, browser: Option<String> }`.

**Rationale**: Keeping resolution pure and in core (IO limited to the existence check) makes
it exhaustively unit-testable (Principle VII) and reusable by every subcommand (Principle
VIII). `ProfileError` as a `thiserror` domain error matches Principle V; the CLI maps it at
the `anyhow` boundary.

**Alternatives rejected**: Resolving per-command in the CLI (duplication, Principle VIII
violation); deferring fragment-existence to the config loader (produces a generic
file-not-found without the owning-profile context FR-017 requires).

## Decision 5 — Scalar readers become profile-aware; daemon threading

**Decision**: The two existing `Settings::load` scalar readers consume the resolved
effective value:
- `resolve_host_ca_activation_cli` (`cli.rs:~1048`) uses `ResolvedSettings.host_ca`.
- The port-forward browser reader (`commands/up/forward.rs:~125`) uses
  `ResolvedSettings.browser`. Because the forwarder runs in a **separate daemon process**,
  the active profile name is threaded to it explicitly (daemon arg), and the daemon
  re-resolves settings with that name rather than relying on ambient `DEACON_PROFILE`
  inheritance.

**Rationale**: FR-013 precedence (CLI/env > profile > root) must hold everywhere the scalar
is read. The daemon is the one out-of-process reader; passing the profile name explicitly
avoids a silent divergence where the daemon reads the root value while the foreground read
the profile value. Existing precedence (flag/env still wins over the resolved profile value)
is preserved by applying the profile resolution *below* the flag/env check that already
exists in each reader.

**Alternatives rejected**: Rely on `DEACON_PROFILE` env inheritance into the daemon
(fragile — breaks if the daemon is launched without inheriting the env, and couples the
selection mechanism to process-env plumbing).

## Decision 6 — Host-hook trust follows author (FR-020a) — IMPLEMENTED

> **Update**: originally scoped as a phased deferral (text below preserved for rationale);
> implemented in full in this delivery. The effective `initializeCommand`'s origin is
> recomputed from the ordered override chain (`initialize_command_author_trusted` in
> `up/lifecycle.rs`): the winner is the highest-precedence layer that sets the field, and a
> settings fragment resolved inside the user-data folder (`override_authored_in_user_data`)
> is owner-authored → the workspace-trust gate is bypassed; the CLI override, the workspace
> base config, and any fragment referenced by an absolute path outside the user-data folder
> stay gated. No new on-disk state; provenance is threaded in-memory. Both branches are
> covered by tests (see tasks.md T028).

**Decision (phased, original)**: The MVP keeps the **existing** workspace-keyed trust gate
unchanged: any host-side hook (`initializeCommand`) in the merged config is gated on the
target workspace's allowlist entry, regardless of which layer contributed it. The FR-020a
refinement — a host hook contributed by a fragment loaded from the trusted user-data folder
runs **without** the workspace allowlist check, while one from a fragment loaded outside the
user-data folder stays gated — is deferred and tracked (see tasks.md "Deferred Work").

**Rationale**: The merge (`ConfigMerger::merge_configs`) collapses per-field provenance, so
honoring FR-020a requires threading the *origin* of the effective `initializeCommand`
(workspace-sourced vs user-data-profile-sourced) through the merge into the existing
`HostTrustSource` gate in `commands/up/lifecycle.rs` — architectural threading beyond MVP
scope, exactly the case Constitution Principle I sanctions for phased delivery.

**Why deferral is safe**: The interim behavior is the *more conservative* option (the
workspace-keyed gate — "trust follows target"), which fails **closed**. It introduces no
security hole; it only adds friction in the narrow case where a profile contributes an
`initializeCommand` **and** the target workspace is untrusted **and** the user has not passed
`--trust-workspace`. The dev-vs-agent use cases are container-scoped (mounts/features/env),
so a profile-contributed host hook is an edge case. When implemented, FR-020a strictly
*relaxes* the gate for owner-authored fragments — a compatible change.

**Acceptance criteria (for the tracked task)**: an `initializeCommand` whose effective value
originates from a user-data-folder profile fragment runs on an untrusted workspace without
prompting; the same command originating from the workspace config, or from a profile fragment
referenced by an absolute path outside the user-data folder, remains gated.

**Alternatives rejected**: Implementing full provenance threading in the MVP (expands scope
and risk for an edge case); adopting "trust follows target" permanently (contradicts the
`/speckit.clarify` decision — the clarified answer is "trust follows author"); a blanket
"a profile vouches for the whole run" (a security defect — it would un-gate a
*workspace-authored* malicious `initializeCommand`).

## Decision 7 — `set-up` scope for `--profile`

**Decision**: `--profile` applies to the subcommands that consume `--override-config` through
the shared config loader: `up`, `read-configuration`, `build`, `outdated`. `set-up` is
**excluded** because it deliberately loads only its local `--config` and does not honor
`--override-config` today (per its SPEC §2 and the current dispatch, which does not pass
`override_config` to the `SetUp` handler). This is a characterized narrowing of spec FR-014,
which optimistically listed `set-up`.

**Rationale**: Constitution Principle I (spec-parity) and VIII (consistency): profile
selection must behave identically to `--override-config`, so it should apply *exactly where*
`--override-config` applies — not more. Making `set-up` suddenly honor overrides via the
profile back-door would be an inconsistent, surprising divergence from its documented
local-config-only contract.

**Action**: Update spec FR-014 to list `up`, `read-configuration`, `build`, `outdated` and
note `set-up`'s exclusion with this rationale. (Recorded here so the spec edit is traceable.)

**Alternatives rejected**: Wiring `set-up` to honor both `--override-config` and `--profile`
(a larger, separate change to `set-up`'s config contract — out of scope, and arguably
undesirable); silently ignoring `--profile` when passed to `set-up` (a silent fallback,
Principle IV violation — if we keep `set-up` excluded, `--profile` simply isn't threaded to
it, so there is nothing to silently ignore).

## Decision 8 — Browser "none" disable sentinel (FR-013a)

**Decision**: Define `browser = "none"` (case-insensitive) as an explicit "do not auto-open"
value, honored in `crate::browser::resolve_browser` (`crates/core/src/browser.rs:29`). It
applies uniformly to the root and per-profile `browser` value and flows through the existing
precedence (`DEACON_BROWSER` env > profile/root `browser` > OS default).

**Rationale**: The motivating agent-mode use case ("opens no browser") is unrealizable today
— `resolve_browser` returns `env > settings.browser > None(OS default)` with no disable path,
so `"none"` would be launched as a program named `none` (best-effort spawn that silently
fails). A reserved sentinel is the minimal, discoverable way to express "off." `"none"` was
already the value used in the spec's US3 example, the contract (C7), and the quickstart, so
this decision makes the documented behavior real rather than introducing a new keyword.

**Scope note**: This is a small, contained addition to `resolve_browser` (a reserved-value
check that returns a disable signal). It is a uniform browser-setting behavior, not
profile-specific — a root `browser: "none"` disables auto-open globally too. Documented here
because it slightly extends the existing `browser` field semantics beyond the profiles
feature proper.

**Alternatives rejected**: Empty string `""` as the sentinel (less discoverable, easy to
author by accident); a separate boolean field like `autoOpenBrowser: false` (a second way to
say the same thing — inconsistent with the single `browser` field); leaving it undefined (the
US3 value stays broken).

## Resolved unknowns summary

| Unknown | Resolution |
|---------|-----------|
| Ordered map type | `IndexMap<String, Profile>` (Decision 1) |
| `mergeConfig` shape | untagged Single/Multiple enum like `AppPort` (Decision 2) |
| Multi-layer override threading | ordered `&[&Path]` into existing merge (Decision 3) |
| Selection + validation home | pure `Settings::resolve` in core, `thiserror` `ProfileError` (Decision 4) |
| Scalar precedence + daemon | profile-aware readers; explicit profile-name threading to daemon (Decision 5) |
| Host-hook trust (FR-020a) | phased deferral; conservative gate in MVP (Decision 6) |
| `set-up` scope | excluded; FR-014 narrowed to override-config consumers (Decision 7) |
| Browser "off" (FR-013a) | reserved `"none"` (case-insensitive) sentinel in `resolve_browser` (Decision 8) |
