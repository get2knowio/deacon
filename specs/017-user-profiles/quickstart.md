# Quickstart: User-Scoped Profiles

Keep several named startup configurations on your machine and pick one per run — without
touching a project's `devcontainer.json` or retyping `--override-config`.

## 1. Author some override fragments

Fragments are ordinary `devcontainer.json` documents, layered on top of a project's config.
Put them next to your settings file (default `~/.deacon/`):

`~/.deacon/overrides/dotfiles.json` — a dev convenience:

```json
{
  "mounts": [
    "source=${localEnv:HOME}/.dotfiles,target=/home/vscode/.dotfiles,type=bind"
  ]
}
```

`~/.deacon/overrides/agent.json` — a lean agent setup (no personal mounts):

```json
{
  "remoteEnv": { "DEACON_MODE": "agent" }
}
```

## 2. Declare profiles in `~/.deacon/settings.json`

```json
{
  "browser": "firefox",
  "defaultProfile": "dev",
  "profiles": {
    "dev":   { "mergeConfig": "overrides/dotfiles.json" },
    "agent": { "mergeConfig": "overrides/agent.json", "browser": "none" }
  }
}
```

- `defaultProfile: "dev"` means a bare command applies `dev`.
- `agent` sets `browser` to the reserved value `"none"`, which **disables** port auto-open;
  `dev` inherits the root `firefox`.
- Paths resolve relative to `~/.deacon/`.

## 3. Use it

```bash
# Applies the default profile (dev → dotfiles mount)
deacon up

# Explicitly select agent (agent override; NO dotfiles mount)
deacon up --profile agent

# Same selection via env
DEACON_PROFILE=agent deacon up

# Inspect the resolved configuration for a profile without starting anything
deacon read-configuration --profile agent

# Highest-precedence merge layer still wins over the profile
deacon up --profile agent --merge-config ./one-off.json

# Or REPLACE the base config entirely (reference parity, #285)
deacon up --profile agent --override-config ./base-instead.json
```

When a profile is applied, deacon prints a diagnostic to **stderr** naming it (stdout / JSON
output is unchanged), so you always know which profile is active.

## Precedence, at a glance

```
base config                      (discovered devcontainer.json + extends,
                                  OR --override-config file if given — REPLACE)
  └─ root "mergeConfig"          (settings.json, always applied — optional)
      └─ selected profile "mergeConfig"
          └─ --merge-config      (CLI, repeatable, highest merge layer)
```

Scalars (`browser`, `hostCa`): `--flag` / env  >  selected profile value  >  root value.

## Rules worth knowing

- **No profiles, or no `defaultProfile`** → a bare command behaves exactly as it does today.
- **Exactly one** profile applies — selecting `agent` never also pulls in `dev`'s dotfiles.
- **Unknown name** (`--profile nope`, or a `defaultProfile` that isn't defined) → deacon
  errors and lists the available profiles.
- **Empty profile** (`"vanilla": {}`) is valid — a way to explicitly opt out of the default.
- The settings file is **read-only** here — hand-edit it. (A `deacon settings` write command
  is tracked separately, issue #198.)
- Profiles are read only from your machine's user-data folder — never from a project. A repo
  cannot define or select a profile.

## Verifying (for contributors)

```bash
# Point at a throwaway settings dir so you don't touch your real ~/.deacon
mkdir -p /tmp/dp/overrides
printf '{"mounts":["source=/tmp/x,target=/x,type=bind"]}' > /tmp/dp/overrides/dev.json
printf '{"browser":"firefox","defaultProfile":"dev","profiles":{"dev":{"mergeConfig":"overrides/dev.json"},"agent":{"browser":"none"}}}' > /tmp/dp/settings.json

# Should show the dev mount merged in:
deacon read-configuration --user-data-folder /tmp/dp --workspace-folder <proj> --profile dev

# Should error, listing dev, agent:
deacon read-configuration --user-data-folder /tmp/dp --workspace-folder <proj> --profile nope
```
