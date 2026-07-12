# Feature Specification: User-Scoped Profiles for Host Settings

**Feature Branch**: `017-user-profiles`  
**Created**: 2026-07-12  
**Status**: Draft  
**Input**: User description: "Add user-scoped 'profiles' to deacon's host settings file so a developer can keep several named startup configurations on their machine and select one per run — without polluting the project's devcontainer.json and without re-typing --override-config every time."

> **Post-implementation update ([#285](https://github.com/get2knowio/deacon/issues/285)).**
> The settings/profile config field is named **`mergeConfig`** (it *deep-overlays*
> a fragment). The CLI split into two flags: **`--override-config` now REPLACES**
> the base config (reference parity), and **`--merge-config`** (repeatable) is the
> CLI merge layer. Read every `overrideConfig` / "CLI override" reference below as
> `mergeConfig` / `--merge-config` respectively; `--override-config` selects the
> *base* the merge ladder overlays onto.

## Clarifications

### Session 2026-07-12

- Q: When a profile fragment introduces a host-side hook (`initializeCommand`), which principle governs whether the workspace-trust gate applies? → A: Trust follows the author — a fragment loaded from the trusted user-data folder runs its host hooks without the workspace allowlist check; a fragment loaded from outside the user-data folder (e.g. an absolute path into a repo) remains subject to the existing gate.
- Q: How should an "empty" profile (no `mergeConfig`, no scalar override) behave when selected? → A: Valid no-op — selecting it is allowed and applies nothing (an explicit "plain/vanilla" profile that opts out of a configured default).
- Q: When a profile is applied, should the tool surface which profile is active? → A: Log to stderr — emit a diagnostic naming the applied profile and its source; do not add it to the stdout/JSON output contract.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Select a named profile per run (Priority: P1)

A developer keeps two ways of running the same project: a **dev** setup that mounts
their personal dotfiles and credentials, and an **agent** setup that runs lean with
none of those personal mounts. They define both as named profiles in their personal
host settings file, each pointing at a devcontainer configuration fragment. On any
project, they choose which one applies for a given run by naming it on the command
line — the chosen profile's overrides are layered on top of the project's own
configuration, and the other profile's overrides are cleanly excluded.

**Why this priority**: This is the core capability and the reason the feature exists.
Without it, a developer must repeatedly pass a full explicit-override path (or edit the
project repo) to switch between personal setups. Delivered alone, named selectable
profiles already provide the complete "dev vs agent" value.

**Independent Test**: Define two profiles in the host settings file, each referencing a
distinct configuration fragment, then run the same project twice selecting a different
profile each time and confirm the resolved configuration reflects only the selected
profile's overrides (and never the unselected profile's).

**Acceptance Scenarios**:

1. **Given** a host settings file with profiles `dev` (adds a dotfiles mount) and
   `agent` (adds an agent-appropriate override), **When** the developer runs the project
   selecting `agent`, **Then** the resolved configuration includes the agent override and
   does **not** include the dev dotfiles mount.
2. **Given** the same settings file, **When** the developer selects `dev`, **Then** the
   resolved configuration includes the dotfiles mount and not the agent override.
3. **Given** a selected profile, **When** the developer also passes an explicit
   run-level configuration override, **Then** the run-level override takes precedence over
   the profile's override on any conflicting key.
4. **Given** a project with its own configuration, **When** a profile is selected,
   **Then** the project configuration remains the base and the profile only adds to or
   overrides it (the project is never discarded).
5. **Given** a selected profile, **When** the developer runs any subcommand that already
   honors an explicit configuration override, **Then** that subcommand resolves the
   profile the same way.

---

### User Story 2 - Apply a default profile automatically (Priority: P2)

A developer who almost always wants the same setup names one profile as their default,
so a bare command applies it without having to name it every time — while still being
able to override the default on any single run.

**Why this priority**: A convenience layer on top of Story 1. It reduces friction for
the common case but is not required for the feature to deliver value; explicit selection
already works without it.

**Independent Test**: Mark one profile as the default, run a bare command, and confirm
that profile is applied; then run the same command naming a different profile and confirm
the named one wins over the default.

**Acceptance Scenarios**:

1. **Given** a settings file that designates `dev` as the default profile, **When** the
   developer runs a bare command with no profile named, **Then** the `dev` profile is
   applied.
2. **Given** the same settings file, **When** the developer names `agent` on the command
   line, **Then** `agent` is applied instead of the default `dev`.
3. **Given** a settings file that defines profiles but designates **no** default, **When**
   the developer runs a bare command, **Then** **no** profile is applied and behavior is
   identical to having no profiles at all.
4. **Given** a settings file with **no** profiles section at all, **When** the developer
   runs any command, **Then** behavior is exactly as it is today.

---

### User Story 3 - Override personal machine settings per profile (Priority: P3)

The developer's host settings already hold personal machine preferences (such as which
browser opens forwarded ports and whether a corporate certificate is activated). They
want a profile to be able to change those preferences for the duration of a run — for
example, the agent profile opens no browser — while a profile that says nothing about a
preference simply inherits the machine-wide value.

**Why this priority**: Rounds out the profile model so a profile can express personal
machine preferences, not just configuration-file overrides. Valuable but the smallest
slice; existing single-value settings must keep working untouched, so this is additive.

**Independent Test**: Set a machine-wide value for a preference, define a profile that
overrides it and another that does not, then confirm the overriding profile changes the
effective value while the non-overriding profile inherits the machine-wide value.

**Acceptance Scenarios**:

1. **Given** a machine-wide browser preference and a profile that overrides it, **When**
   that profile is selected, **Then** the profile's browser preference is in effect.
2. **Given** a machine-wide browser preference and a selected profile that does **not**
   mention it, **When** that profile is selected, **Then** the machine-wide preference is
   in effect.
3. **Given** a corresponding command-line flag or environment variable for a preference,
   **When** it is set alongside a profile that also sets the preference, **Then** the
   command-line/environment value takes precedence over the profile value.
4. **Given** a settings file containing only machine-wide preferences and no profiles,
   **When** any command runs, **Then** the preferences behave exactly as they do today.

---

### Edge Cases

- **Unknown selected profile**: Naming a profile that is not defined must fail fast with a
  clear error that lists the available profile names.
- **Dangling default**: Designating a default profile that is not defined must fail fast
  at load with the same "available profiles" error.
- **Missing referenced fragment**: A profile that references a configuration fragment file
  that does not exist must produce a clear error identifying which profile and which path
  is missing.
- **Empty / no-profiles settings**: A missing settings file, an empty settings file, or a
  settings file with no profiles section must all be non-errors and preserve today's
  behavior.
- **Multiple overrides in one profile**: When a profile references an ordered list of
  configuration fragments, later entries win over earlier entries on conflicting keys.
- **Empty profile**: A profile that declares neither a configuration fragment nor any
  scalar-setting override is valid; selecting it applies nothing (an explicit "plain"
  profile a developer can use to opt out of a configured default). It must not be an error.
- **Disabling the browser via a profile**: A profile that sets `browser` to `"none"`
  (case-insensitive) suppresses port auto-open while it is selected; an unselected profile,
  or one that leaves `browser` unset, does not disable it (it inherits the root value).
- **Host-side hook introduced by a profile fragment**: If a selected profile's fragment
  (loaded from the trusted user-data folder) contains a host-side hook, that hook runs
  without the workspace-trust allowlist check — the fragment is machine-owner-authored. But
  if the fragment is loaded from outside the user-data folder (e.g. an absolute path
  pointing into a repo), the existing workspace-trust gate still applies to any host-side
  hook it introduces.
- **Unknown keys**: Unrecognized keys anywhere in the settings file (including inside a
  profile object) must be tolerated, so a settings file authored for a newer version still
  loads on an older version and vice versa.
- **Older reader, newer file**: A version of the tool that predates this feature must load
  a profiles-bearing settings file without error and simply apply no profile (safe
  degradation to plain behavior).
- **Project cannot define or select a profile**: Profiles are only ever read from the
  user's personal host settings location; nothing in a project's devcontainer
  configuration may define or select a profile.

## Requirements *(mandatory)*

### Functional Requirements

**Definition & storage**

- **FR-001**: The host settings file MUST support a `profiles` section that maps a profile
  name to a profile definition, preserving the declared order of profiles.
- **FR-002**: A profile definition MUST support declaring one or more configuration
  fragments (a single path or an ordered list of paths) to be layered onto the resolved
  configuration.
- **FR-003**: A profile definition MUST support overriding the existing machine-wide
  scalar preferences (browser-open program and corporate-certificate activation) for the
  duration of a run.
- **FR-004**: The host settings file MUST support a `defaultProfile` selector naming which
  defined profile applies when the user does not select one.
- **FR-005**: A profile definition MUST tolerate unknown keys so future preferences can be
  added without breaking older readers.

**Selection**

- **FR-006**: The tool MUST provide a command-line way (and an equivalent environment
  variable) to select a profile by name for a single run.
- **FR-007**: Profile selection MUST follow the precedence: explicit selection on the run
  wins over the designated default, which wins over no profile.
- **FR-008**: When no profile is selected and no default is designated, the tool MUST
  apply no profile and behave exactly as it does with no profiles defined.
- **FR-009**: Exactly one profile MUST apply at a time; selecting one profile MUST NOT
  cause any other profile's overrides to be applied. There MUST be no implicitly-special
  profile name.
- **FR-009a**: A profile that declares neither a configuration fragment nor any
  scalar-setting override MUST be valid; selecting it applies nothing and MUST NOT be an
  error.
- **FR-009b**: When a profile is applied (whether via explicit selection or the designated
  default), the tool MUST emit a diagnostic to the log/diagnostic stream (stderr) naming the
  applied profile and its source. This diagnostic MUST NOT be added to the stdout / JSON
  output contract.

**Layering / resolution**

- **FR-010**: When a profile is applied, the tool MUST layer the profile's configuration
  fragments onto the resolved project configuration using the same merge semantics as the
  existing explicit configuration-override mechanism (nested objects deep-merge; scalars
  and lists are replaced by the higher-precedence layer).
- **FR-011**: The configuration layering order, from lowest to highest precedence, MUST be:
  (1) the project configuration and its inheritance chain; (2) an optional machine-wide
  configuration override declared at the root of the settings file; (3) the selected
  profile's configuration fragments; (4) the explicit configuration override provided on
  the run.
- **FR-012**: Within a single profile's ordered list of configuration fragments, entries
  MUST be applied in listed order, with later entries taking precedence over earlier ones.
- **FR-013**: For each scalar machine preference, the effective value MUST resolve as:
  command-line/environment value, then the selected profile's value, then the machine-wide
  (root) value. A profile that does not set a preference MUST inherit the machine-wide
  value.
- **FR-013a**: The browser preference MUST support an explicit "no browser" value so a
  profile (e.g. an agent profile) can disable port auto-open. Setting `browser` to `"none"`
  (case-insensitive), at the root or in a profile, MUST suppress the browser launch rather
  than be treated as a program name. This value participates in the FR-013 precedence like
  any other browser value (a profile `"none"` disables; a profile that leaves `browser`
  unset inherits the root value).
- **FR-014**: Every subcommand that honors an explicit configuration override today MUST
  honor profile selection consistently — specifically the container-start (`up`),
  configuration read-out (`read-configuration`), image-build (`build`), and
  dependency-freshness (`outdated`) flows. `set-up` is excluded because it deliberately
  loads only its local `--config` and does not honor `--override-config` today; profile
  selection MUST apply exactly where `--override-config` applies, and no more (see
  research.md Decision 7).

**Validation (fail-fast)**

- **FR-015**: Selecting a profile name that is not defined MUST produce a hard error whose
  message lists the available profile names.
- **FR-016**: Designating a default profile that is not defined MUST produce the same hard
  error at load time.
- **FR-017**: A referenced configuration fragment that cannot be found MUST produce a
  clear error identifying the owning profile and the missing path.
- **FR-018**: A missing, empty, or profiles-free settings file MUST NOT be an error.

**Trust, scope & compatibility**

- **FR-019**: Profiles MUST be read only from the user's personal host settings location;
  the tool MUST NOT read profiles or profile selection from any project-supplied
  configuration.
- **FR-020**: Because profiles come from the machine-owner-controlled settings location,
  applying a profile MUST NOT require any additional workspace-trust confirmation.
- **FR-020a**: Trust for host-side hooks introduced by a profile fragment follows the
  fragment's author, not the target workspace: a fragment loaded from the user-data folder
  is machine-owner-authored, so any host-side hook it introduces MUST run without the
  workspace-trust allowlist check. A fragment loaded from outside the user-data folder
  (e.g. an absolute path pointing into a repo) is NOT owner-guaranteed, so any host-side
  hook it introduces MUST remain subject to the existing workspace-trust gate.
- **FR-021**: Configuration-fragment paths declared in a profile MUST be resolved relative
  to the settings file's own location, and absolute paths MUST be accepted as-is.
- **FR-022**: The settings file MUST remain read-only for this feature; the tool loads and
  resolves profiles but MUST NOT provide a command to write or edit them (users hand-edit
  the file).
- **FR-023**: An older version of the tool MUST still load a settings file that contains
  profiles without error and simply apply no profile.
- **FR-024**: This feature MUST NOT change how the tool behaves relative to the reference
  DevContainers CLI for any project that does not use profiles; profiles are an additive,
  tool-specific convenience.

### Key Entities *(include if feature involves data)*

- **Host settings file**: The user's personal, machine-owned settings document. Already
  carries machine-wide scalar preferences today; gains a `profiles` map, an optional
  `defaultProfile` selector, and an optional root-level configuration override.
- **Profile**: A named entry in the `profiles` map. May declare an ordered set of
  configuration fragments to layer, may override the scalar machine preferences, and
  tolerates unknown/future keys. Profiles are mutually exclusive at selection time.
- **Profile selection**: The per-run decision of which single profile (if any) applies,
  resolved from the run's explicit selection, else the designated default, else none.
- **Configuration fragment**: A devcontainer-configuration document referenced by a
  profile (or by the root override), layered onto the resolved configuration exactly like
  the existing explicit configuration-override input.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: A developer can switch between two personal setups (e.g. "dev" and "agent")
  on the same project by changing a single word on the command line, with no edits to the
  project and no re-typing of override file paths.
- **SC-002**: Selecting a profile applies 100% of that profile's declared overrides and 0%
  of any unselected profile's overrides, verifiable by inspecting the resolved
  configuration.
- **SC-003**: Every existing settings file that has no profiles produces the same behavior
  as before this feature (zero regressions for non-adopters).
- **SC-004**: Naming an undefined profile (via selection or default) fails immediately with
  an error that names every available profile, so the developer can self-correct without
  consulting documentation.
- **SC-005**: The precedence order is honored in 100% of cases: an explicit run-level
  override always wins over a profile, and a profile always wins over the machine-wide base.
- **SC-006**: Every subcommand that accepts an explicit configuration override resolves a
  selected profile identically, so behavior does not vary by subcommand.
- **SC-007**: Whenever a profile is applied, the developer can see which profile is active
  from the diagnostic output without inspecting the resolved configuration, and the
  stdout/JSON output contract is unchanged by profile application.

## Assumptions

- The existing explicit configuration-override mechanism and its merge semantics (nested
  objects deep-merge; scalars and lists replace) are the model for how profile fragments
  layer; this feature reuses that behavior rather than defining a new merge algorithm.
- The two scalar machine preferences overridable per profile are the two that exist in the
  settings file today (browser-open program and corporate-certificate activation); future
  preferences follow the same root-base / profile-override rule.
- The root-level machine-wide configuration override (precedence rung 2) is included as an
  always-applied base layer. It is optional for a settings author to use; the "dev vs
  agent" value does not depend on it, and it may be trimmed from an initial delivery
  without affecting the primary user stories.
- "Read-only settings file" means this feature adds no write/edit command; a future write
  command is tracked separately (issue #198) and is out of scope here.
