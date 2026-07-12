# Contract: `--profile` flag & `settings.json` profiles schema

This is the user-facing contract for the profiles feature: the CLI surface and the
`settings.json` schema a developer hand-edits. It is deacon-specific (not containers.dev).

## CLI surface

### Global flag

```
--profile <NAME>          Select a named profile from settings.json for this run.
                          Env: DEACON_PROFILE
```

- **Scope**: global flag (same tier as `--override-config`, `--user-data-folder`), honored by
  `up`, `read-configuration`, `build`, `outdated`. Not honored by `set-up` (local `--config`
  only — see research.md Decision 7).
- **Precedence of selection**: `--profile` > `DEACON_PROFILE` > `defaultProfile` (settings) >
  none.
- **Unknown value**: `--profile nope` (or `DEACON_PROFILE=nope`) → non-zero exit with an
  error naming `nope` and listing available profiles in declaration order.

### Interaction with `--override-config` and `--merge-config`

> **Post-#285 update.** `--override-config` now **replaces** the base config
> (reference parity), and the *merge* behavior moved to a new repeatable
> `--merge-config` flag. The profile field is named `mergeConfig` (it deep-overlays).

Both `--profile` and the config flags may be given. Final precedence (low→high):

```
base config                        (discovered devcontainer.json + extends,
                                    OR the --override-config file if given — REPLACE)
  <  root mergeConfig
  <  selected profile mergeConfig
  <  --merge-config                 (CLI, repeatable, highest merge layer)
```

`--override-config` chooses the base the merge ladder overlays onto;
`--merge-config` is the highest-precedence overlay escape hatch.

### Diagnostics (stderr only)

When a profile is applied, a `tracing` diagnostic (stderr) names the applied profile and its
source. stdout and `--output json` documents are **unchanged** by profile application.

## `settings.json` schema (additions)

Location: `{user_data_folder}/settings.json` (default `~/.deacon/settings.json`; honors
`--user-data-folder`). Read-only (hand-edited). Unknown keys tolerated.

```jsonc
{
  // existing root scalar settings (base values)
  "hostCa": "auto",
  "browser": "firefox",

  // NEW: optional root-level universal override (always applied, precedence rung 2)
  "mergeConfig": "overrides/base.json",        // string OR array of strings

  // NEW: which profile applies on a bare invocation (omit ⇒ none)
  "defaultProfile": "dev",

  // NEW: named, mutually-exclusive profiles
  "profiles": {
    "dev": {
      "mergeConfig": ["overrides/dotfiles.json"] // string or ordered array
      // hostCa / browser omitted ⇒ inherit root
    },
    "agent": {
      "mergeConfig": "overrides/agent.json",
      "browser": "none"                             // overrides root browser for this profile
    },
    "vanilla": {}                                    // valid empty/no-op profile
  }
}
```

### Field semantics

| Key | Where | Meaning |
|---|---|---|
| `mergeConfig` | root | Universal override layer applied on every run (optional). |
| `defaultProfile` | root | Profile name used when no `--profile`/`DEACON_PROFILE` is given. Must name a defined profile (else hard error). |
| `profiles` | root | Map of name → profile. Declaration order preserved. |
| `mergeConfig` | profile | Fragment(s) layered when this profile is selected. String or ordered array; later array entries win. |
| `hostCa` / `browser` | profile | Override the root scalar for this profile; omit to inherit root. |
| `browser: "none"` | root or profile | Reserved (case-insensitive) value that disables port auto-open, rather than naming a program (FR-013a). |

- Paths in any `mergeConfig` resolve relative to the settings-file directory; absolute
  paths are accepted as-is. A path that does not exist → hard error naming the owning profile
  (or "root") and the path.

## Contract test scenarios (map to spec acceptance)

| # | Setup | Action | Expected |
|---|---|---|---|
| C1 | `profiles.{dev,agent}`, `defaultProfile: dev` | `up --profile agent` | agent override applied; dev override NOT applied |
| C2 | same | bare `up` | dev override applied |
| C3 | `profiles` present, no `defaultProfile` | bare `up` | no override applied (== no-profiles behavior) |
| C4 | no `profiles` key | any command | identical to today |
| C5 | `profiles.{dev,agent}` | `up --profile nope` | error, lists `dev, agent`; non-zero exit |
| C6 | `defaultProfile: typo` (undefined) | any command | error at load, lists available; non-zero exit |
| C7 | `profiles.agent.browser: none` selected | resolve settings | effective browser = `none` → auto-open disabled (FR-013a); unselected `dev` inherits root browser |
| C11 | workspace `devcontainer.json` contains a `profiles`/`defaultProfile` key | any command | ignored — profiles are read only from the user-data folder; the key is treated as an ordinary unknown config field, never as a profile source (FR-019) |
| C8 | profile `mergeConfig: [a, b]` with conflicting key | `read-configuration --profile p` | `b` wins over `a`; both below `--merge-config` |
| C9 | `profiles.vanilla: {}` | `up --profile vanilla` | valid; nothing applied |
| C10 | selected profile + `DEACON_BROWSER` set | resolve | env value wins over profile value |
