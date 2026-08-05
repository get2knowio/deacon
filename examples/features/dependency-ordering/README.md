# Features: Dependency-Driven Install Order

Distinct from `override-install-order/` (which uses the explicit
`overrideFeatureInstallOrder` array), this example proves that deacon resolves
install order **automatically** from feature dependency metadata —
`installsAfter` (soft) and `dependsOn` (hard) — with no manual override.

## The setup

Three local features, kept under `.devcontainer/` as
`devcontainer-features-distribution.md` §Locally Referenced Features requires,
whose **declaration / alphabetical** order would be `app, base, lib`:

- `.devcontainer/feature-base` (id `base`) — no dependencies
- `.devcontainer/feature-lib` (id `lib`) —
  `"installsAfter": ["./.devcontainer/feature-base"]`
- `.devcontainer/feature-app` (id `app`) —
  `"dependsOn": { "./.devcontainer/feature-lib": {} }`

Each `install.sh` appends its name to `/usr/local/share/feature-order/log`.

The dependency graph (`lib` after `base`, `app` after `lib`) forces a
**different** order: `base → lib → app`.

> Note: both `installsAfter` and `dependsOn` name a **local** sibling the same
> way the `features` map does — as a path resolved against the **config
> directory**, here `./.devcontainer/feature-*`. deacon will also match a bare
> metadata `id` (`"base"`), but the reference CLI 0.87.0 rejects that spelling
> with `Legacy feature 'base' not supported`, so these fixtures use the path
> form, which both CLIs accept.

## Scenario exercised by `exec.sh`

After `up`, the recorded install order is exactly `base,lib,app` — confirming
the dependency graph (not declaration order) drove installation.

## Spec references

- Feature install order / `installsAfter` / `dependsOn`:
  <https://containers.dev/implementors/features/#installation-order>
