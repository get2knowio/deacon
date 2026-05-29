# Features: Dependency-Driven Install Order

Distinct from `override-install-order/` (which uses the explicit
`overrideFeatureInstallOrder` array), this example proves that deacon resolves
install order **automatically** from feature dependency metadata —
`installsAfter` (soft) and `dependsOn` (hard) — with no manual override.

## The setup

Three local features whose **declaration / alphabetical** order would be
`app, base, lib`:

- `feature-base` (id `base`) — no dependencies
- `feature-lib` (id `lib`) — `"installsAfter": ["base"]`
- `feature-app` (id `app`) — `"dependsOn": { "lib": {} }`

Each `install.sh` appends its name to `/usr/local/share/feature-order/log`.

The dependency graph (`lib` after `base`, `app` after `lib`) forces a
**different** order: `base → lib → app`.

> Note: `installsAfter` and `dependsOn` reference a sibling feature by its
> metadata `id` (here `base` / `lib`), which deacon maps onto the local
> `./feature-*` path during resolution.

## Scenario exercised by `exec.sh`

After `up`, the recorded install order is exactly `base,lib,app` — confirming
the dependency graph (not declaration order) drove installation.

## Spec references

- Feature install order / `installsAfter` / `dependsOn`:
  <https://containers.dev/implementors/features/#installation-order>
